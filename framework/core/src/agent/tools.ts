import type { AnySchemaValidator } from '@forklaunch/validator';
import type {
  AgentTool,
  AgentToolContext,
  JsonSchema,
  ToolAnnotations,
  ToolSource
} from './types';

export const TOOL_NAME = /^[a-z][a-z0-9_]{0,63}$/;

/** Only tools that declare themselves read-only run without the user's approval. */
export function requiresApproval(annotations: ToolAnnotations): boolean {
  return annotations.readOnlyHint !== true;
}

/** Turns any label into a tool-name segment: `GetRecord` → `get_record`. */
export function toolNameSegment(value: string): string {
  return value
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .replace(/[^A-Za-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .toLowerCase();
}

/**
 * Plain JSON Schema for model providers: single-entry `type` arrays become a
 * string and OpenAPI-only keys (`example`, `examples`, `xml`) are dropped.
 */
export function toJsonSchema(schema: unknown): JsonSchema {
  const visit = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(visit);
    if (!value || typeof value !== 'object') return value;
    const out: Record<string, unknown> = {};
    for (const [key, entry] of Object.entries(value)) {
      if (key === 'example' || key === 'examples' || key === 'xml') continue;
      out[key] =
        key === 'type' && Array.isArray(entry) && entry.length === 1
          ? entry[0]
          : visit(entry);
    }
    return out;
  };
  return visit(schema) as JsonSchema;
}

export function checkTool(tool: AgentTool): AgentTool {
  if (!TOOL_NAME.test(tool.name))
    throw new Error(
      `Agent tool name "${tool.name}" must be lowercase snake_case, up to 64 characters.`
    );
  if (!tool.description.trim())
    throw new Error(`Agent tool "${tool.name}" needs a description.`);
  return tool;
}

/**
 * Returns `defineTool` bound to the app's schema validator, so tool inputs use
 * the same schema descriptors as routes and reach the model as JSON Schema.
 *
 * ```ts
 * export const defineTool = toolFactory(SchemaValidator());
 * export const currentTime = defineTool({
 *   name: 'current_time',
 *   description: 'The current date and time in a time zone.',
 *   inputSchema: { timeZone: optional(string) },
 *   annotations: { readOnlyHint: true },
 *   handler: async ({ timeZone }) => ({ now: new Date().toLocaleString('en-US', { timeZone }) })
 * });
 * ```
 */
export function toolFactory<SV extends AnySchemaValidator>(
  schemaValidator: SV
) {
  return function defineTool<
    Schema extends Record<string, unknown>,
    Input = Record<string, unknown>
  >(definition: {
    name: string;
    title?: string;
    description: string;
    inputSchema: Schema;
    annotations: ToolAnnotations;
    handler: (input: Input, context: AgentToolContext) => Promise<unknown>;
  }): AgentTool<Input> {
    const validator = schemaValidator as unknown as {
      openapi(schema: unknown): JsonSchema;
      parse(
        schema: unknown,
        value: unknown
      ):
        | { ok: true; value: unknown }
        | { ok: false; errors?: { path: string[]; message: string }[] };
    };
    const inputSchema = validator.openapi(definition.inputSchema);
    return checkTool({
      ...(definition as unknown as AgentTool),
      inputSchema: toJsonSchema({ ...inputSchema, type: 'object' }),
      parse: (input) => {
        const result = validator.parse(definition.inputSchema, input ?? {});
        if (result.ok) return { ok: true, value: result.value };
        const first = result.errors?.[0];
        return {
          ok: false,
          message: first
            ? `${first.path.join('.') || 'input'}: ${first.message}`
            : 'Invalid tool input.'
        };
      }
    }) as AgentTool<Input>;
  };
}

/** The agent's own tools, defined with `defineTool`. */
export function localTools(
  name: string,
  // Each tool narrows its own input type; the registry only sees unknown input.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  tools: AgentTool<any>[]
): ToolSource {
  const checked = (tools as AgentTool[]).map(checkTool);
  return { kind: 'local', name, list: async () => checked };
}
