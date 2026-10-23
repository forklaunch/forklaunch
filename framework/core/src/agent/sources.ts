import { toJsonSchema, toolNameSegment } from './tools';
import type {
  AgentTool,
  AgentToolContext,
  JsonSchema,
  ToolAnnotations,
  ToolSource
} from './types';

const CACHE_MS = 5 * 60_000;
const SKIPPED_PATH =
  /\/(openapi|docs|health|ready|compliance|agent)(\/|$)|\/mcp(\/|$)/;

type OpenApiOperation = {
  operationId?: string;
  summary?: string;
  description?: string;
  parameters?: {
    name: string;
    in: string;
    required?: boolean;
    schema?: JsonSchema;
  }[];
  requestBody?: { content?: Record<string, { schema?: JsonSchema }> };
  'x-mcp-annotations'?: ToolAnnotations;
};
type OpenApiDocument = {
  paths?: Record<string, Record<string, OpenApiOperation>>;
  components?: { schemas?: Record<string, JsonSchema> };
};

function cached<T>(load: () => Promise<T>) {
  let value: { at: number; data: T } | undefined;
  return async (fresh = false) => {
    if (!fresh && value && Date.now() - value.at < CACHE_MS) return value.data;
    const data = await load();
    value = { at: Date.now(), data };
    return data;
  };
}

/** Inlines local `#/components/schemas/X` references (cycles become plain objects). */
function inline(
  schema: unknown,
  doc: OpenApiDocument,
  seen = new Set<string>()
): unknown {
  if (Array.isArray(schema)) return schema.map((s) => inline(s, doc, seen));
  if (!schema || typeof schema !== 'object') return schema;
  const ref = (schema as { $ref?: unknown }).$ref;
  if (typeof ref === 'string' && ref.startsWith('#/components/schemas/')) {
    const key = ref.slice('#/components/schemas/'.length);
    if (seen.has(key)) return { type: 'object' };
    return inline(
      doc.components?.schemas?.[key] ?? {},
      doc,
      new Set([...seen, key])
    );
  }
  return Object.fromEntries(
    Object.entries(schema).map(([k, v]) => [k, inline(v, doc, seen)])
  );
}

function annotationsFor(
  method: string,
  operation: OpenApiOperation
): ToolAnnotations {
  if (operation['x-mcp-annotations']) return operation['x-mcp-annotations'];
  switch (method) {
    case 'get':
    case 'head':
      return { readOnlyHint: true };
    case 'delete':
      return { readOnlyHint: false, destructiveHint: true };
    case 'put':
      return { readOnlyHint: false, idempotentHint: true };
    default:
      return { readOnlyHint: false };
  }
}

/**
 * Exposes an app service's routes as tools, read from its OpenAPI document and
 * called over its normal HTTP port with the caller's own Authorization header,
 * so the service's auth and roles apply. GET routes are read-only; everything
 * else needs the user's approval unless the route declares `x-mcp-annotations`.
 */
export function serviceTools(
  service: string,
  options: {
    /** Base URL, e.g. `RECORDS_URL`. Default: the `<SERVICE>_URL` setting. */
    url?: string;
    /** Version segment of the OpenAPI route. Default `VERSION` or `v1`. */
    version?: string;
    /** Operation ids to expose; default all. */
    include?: string[];
    exclude?: string[];
    fetch?: typeof fetch;
  } = {}
): ToolSource {
  const doFetch = options.fetch ?? fetch;
  const setting = `${service.replace(/-/g, '_').toUpperCase()}_URL`;
  const baseUrl = () => {
    const url = options.url ?? process.env[setting];
    if (!url)
      throw new Error(`Set ${setting} to reach the ${service} service.`);
    // Trim trailing slashes without a regex: /\/+$/ backtracks on long runs.
    let end = url.length;
    while (end > 0 && url[end - 1] === '/') end--;
    return url.slice(0, end);
  };
  const version = options.version ?? process.env.VERSION ?? 'v1';
  const load = cached(async () => {
    const response = await doFetch(`${baseUrl()}/api/${version}/openapi`, {
      signal: AbortSignal.timeout(10_000)
    });
    if (!response.ok)
      throw new Error(
        `The ${service} service did not return its API description.`
      );
    const body = (await response.json()) as {
      latest?: OpenApiDocument;
    } & OpenApiDocument;
    return body.latest ?? body;
  });
  return {
    kind: 'service',
    name: service,
    async list(_context, listOptions) {
      const doc = await load(listOptions?.fresh);
      const tools: AgentTool[] = [];
      for (const [path, methods] of Object.entries(doc.paths ?? {})) {
        if (SKIPPED_PATH.test(path)) continue;
        for (const [method, operation] of Object.entries(methods)) {
          if (!['get', 'post', 'put', 'patch', 'delete'].includes(method))
            continue;
          const id = operation.operationId ?? `${method}_${path}`;
          if (options.include && !options.include.includes(id)) continue;
          if (options.exclude?.includes(id)) continue;
          const content = operation.requestBody?.content ?? {};
          if (Object.keys(content).length && !content['application/json'])
            continue;
          const params = operation.parameters ?? [];
          const group = (where: string) => {
            const entries = params.filter((p) => p.in === where);
            if (!entries.length) return undefined;
            return {
              type: 'object',
              properties: Object.fromEntries(
                entries.map((p) => [
                  p.name,
                  inline(p.schema ?? { type: 'string' }, doc)
                ])
              ),
              required: entries
                .filter((p) => p.required || where === 'path')
                .map((p) => p.name)
            };
          };
          const properties: Record<string, unknown> = {};
          const required: string[] = [];
          const pathParams = group('path');
          const query = group('query');
          const body = content['application/json']?.schema;
          if (pathParams)
            ((properties.params = pathParams), required.push('params'));
          if (query) properties.query = query;
          if (body)
            ((properties.body = inline(body, doc)), required.push('body'));
          const name =
            `${toolNameSegment(service)}__${toolNameSegment(id)}`.slice(0, 64);
          tools.push({
            name,
            title: operation.summary,
            description:
              operation.description ||
              operation.summary ||
              `${method.toUpperCase()} ${path} on ${service}`,
            inputSchema: toJsonSchema({ type: 'object', properties, required }),
            annotations: annotationsFor(method, operation),
            handler: async (input, context) =>
              callRoute(
                doFetch,
                baseUrl(),
                method,
                path,
                input as Record<string, unknown>,
                context
              )
          });
        }
      }
      return tools;
    }
  };
}

async function callRoute(
  doFetch: typeof fetch,
  baseUrl: string,
  method: string,
  path: string,
  input: Record<string, unknown>,
  context: AgentToolContext
) {
  const params = (input.params ?? {}) as Record<string, unknown>;
  const filled = path.replace(/\{([^}]+)\}/g, (_, key: string) => {
    const value = params[key];
    if (value === undefined) throw new Error(`Missing ${key}.`);
    return encodeURIComponent(String(value));
  });
  const url = new URL(baseUrl + filled);
  for (const [key, value] of Object.entries(
    (input.query ?? {}) as Record<string, unknown>
  ))
    if (value !== undefined)
      url.searchParams.set(
        key,
        typeof value === 'string' ? value : JSON.stringify(value)
      );
  const response = await doFetch(url, {
    method: method.toUpperCase(),
    redirect: 'error',
    signal: context.signal,
    headers: {
      ...(context.authorization
        ? { authorization: context.authorization }
        : {}),
      ...(input.body !== undefined
        ? { 'content-type': 'application/json' }
        : {})
    },
    body: input.body !== undefined ? JSON.stringify(input.body) : undefined
  });
  const text = await response.text();
  if (!response.ok)
    throw new Error(
      `The service answered ${response.status}: ${text.slice(0, 500)}`
    );
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

/**
 * Tools of an external MCP server over streamable HTTP, named `<server>__<tool>`.
 * Credentials come from settings: pass header values read from the environment.
 */
export function mcpServer(options: {
  name: string;
  url: string | undefined;
  headers?: Record<string, string | undefined>;
}): ToolSource {
  const prefix = toolNameSegment(options.name);
  const connect = async (signal?: AbortSignal) => {
    if (!options.url)
      throw new Error(
        `Set the URL setting for the ${options.name} MCP server.`
      );
    const { Client } =
      await import('@modelcontextprotocol/sdk/client/index.js');
    const { StreamableHTTPClientTransport } =
      await import('@modelcontextprotocol/sdk/client/streamableHttp.js');
    const headers = Object.fromEntries(
      Object.entries(options.headers ?? {}).filter(
        (entry): entry is [string, string] =>
          typeof entry[1] === 'string' && !!entry[1]
      )
    );
    const client = new Client({ name: 'forklaunch-agent', version: '1.0.0' });
    await client.connect(
      new StreamableHTTPClientTransport(new URL(options.url), {
        requestInit: { headers, signal }
      })
    );
    return client;
  };
  const load = cached(async () => {
    const client = await connect(AbortSignal.timeout(15_000));
    try {
      const tools: {
        name: string;
        title?: string;
        description?: string;
        inputSchema: JsonSchema;
        annotations?: ToolAnnotations;
      }[] = [];
      let cursor: string | undefined;
      do {
        const page = await client.listTools(cursor ? { cursor } : {});
        tools.push(...(page.tools as typeof tools));
        cursor = page.nextCursor;
      } while (cursor && tools.length < 500);
      return tools;
    } finally {
      await client.close().catch(() => undefined);
    }
  });
  return {
    kind: 'mcp',
    name: options.name,
    async list(_context, listOptions) {
      return (await load(listOptions?.fresh)).map((tool) => ({
        name: `${prefix}__${toolNameSegment(tool.name)}`.slice(0, 64),
        title: tool.title ?? tool.annotations?.title,
        description: tool.description || tool.name,
        inputSchema: toJsonSchema(tool.inputSchema),
        annotations: { openWorldHint: true, ...tool.annotations },
        handler: async (input, context) => {
          const client = await connect(context.signal);
          try {
            const result = await client.callTool({
              name: tool.name,
              arguments: (input ?? {}) as Record<string, unknown>
            });
            if (result.isError)
              throw new Error(
                JSON.stringify(result.content).slice(0, 500) ||
                  'The MCP tool failed.'
              );
            return result.structuredContent ?? result.content;
          } finally {
            await client.close().catch(() => undefined);
          }
        }
      }));
    }
  };
}
