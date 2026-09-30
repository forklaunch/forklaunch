import { generateHmacAuthHeaders } from './generateHmacAuthHeaders';

/**
 * Client for the ForkLaunch model gateway, used from inside a managed
 * instance.
 *
 * A managed instance calls platform-hosted AI models (Azure AI Foundry)
 * without holding a provider key: each request is signed with the
 * instance's own HMAC key, and the platform enforces the product's model
 * allowlist, monthly token budget and rate limit. For a HIPAA product the
 * platform only offers models covered by its BAA.
 *
 * The platform injects everything this needs into a hosted instance:
 * `PLATFORM_GATEWAY_URL` (the platform's public base), `INSTANCE_ID` and
 * `INSTANCE_HMAC_KEY`. Their absence means the instance is not hosted in
 * managed mode, and the client refuses to be created rather than failing on
 * the first call.
 *
 * @example
 * const models = createModelGatewayClient();
 * const reply = await models.chat.completions.create({
 *   model: 'terra',
 *   messages: [{ role: 'user', content: 'Summarize this intake form: …' }]
 * });
 * for await (const chunk of models.chat.completions.stream({ model: 'luna', messages })) {
 *   process.stdout.write(chunk.choices[0]?.delta?.content ?? '');
 * }
 */

/** Where the gateway router is mounted on the platform service. */
const GATEWAY_MOUNT = '/instance-gateway';

export interface ModelGatewayClientOptions {
  /** Platform base URL. Default: `PLATFORM_GATEWAY_URL`. */
  gatewayUrl?: string;
  /** This instance's id. Default: `INSTANCE_ID`. */
  instanceId?: string;
  /** This instance's HMAC key. Default: `INSTANCE_HMAC_KEY`. */
  hmacKey?: string;
  fetch?: typeof fetch;
}

/** An OpenAI-style chat message. */
export interface GatewayChatMessage {
  role: 'system' | 'developer' | 'user' | 'assistant' | 'tool';
  content: unknown;
  [key: string]: unknown;
}

/** An OpenAI-compatible chat completions request; `model` is a catalog alias. */
export interface GatewayChatRequest {
  model: string;
  messages: GatewayChatMessage[];
  temperature?: number;
  top_p?: number;
  max_tokens?: number;
  max_completion_tokens?: number;
  response_format?: unknown;
  tools?: unknown[];
  tool_choice?: unknown;
  parallel_tool_calls?: boolean;
  stop?: unknown;
  seed?: number;
  presence_penalty?: number;
  frequency_penalty?: number;
  reasoning_effort?: string;
}

export interface GatewayChatCompletion {
  id: string;
  object: string;
  created: number;
  model: string;
  choices: {
    index: number;
    message: { role: string; content: string | null; [key: string]: unknown };
    finish_reason: string | null;
    [key: string]: unknown;
  }[];
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
  [key: string]: unknown;
}

export interface GatewayChatChunk {
  id: string;
  object: string;
  model: string;
  choices: {
    index: number;
    delta: { role?: string; content?: string | null; [key: string]: unknown };
    finish_reason?: string | null;
  }[];
  usage?: GatewayChatCompletion['usage'];
  [key: string]: unknown;
}

export interface GatewayModels {
  models: { id: string; baa: boolean }[];
  monthlyTokenBudget: number;
  tokensUsedThisMonth: number;
  requestsPerMinute: number;
  withheldForHipaa?: string[];
}

/** A refusal or failure from the gateway, with the status it answered. */
export class ModelGatewayRequestError extends Error {
  readonly name = 'ModelGatewayRequestError' as const;
  constructor(
    readonly status: number,
    message: string,
    /** Seconds to wait before retrying, on a 429. */
    readonly retryAfterSeconds?: number
  ) {
    super(message);
  }
}

export interface ModelGatewayClient {
  /** The models this instance may call, and where its monthly budget stands. */
  models(): Promise<GatewayModels>;
  chat: {
    completions: {
      create(request: GatewayChatRequest): Promise<GatewayChatCompletion>;
      stream(
        request: GatewayChatRequest,
        options?: { signal?: AbortSignal }
      ): AsyncGenerator<GatewayChatChunk>;
    };
  };
}

export function createModelGatewayClient(
  options: ModelGatewayClientOptions = {}
): ModelGatewayClient {
  const gatewayUrl = options.gatewayUrl ?? process.env.PLATFORM_GATEWAY_URL;
  const instanceId = options.instanceId ?? process.env.INSTANCE_ID;
  const hmacKey = options.hmacKey ?? process.env.INSTANCE_HMAC_KEY;
  if (!gatewayUrl || !instanceId || !hmacKey) {
    throw new Error(
      'The model gateway is only available to instances hosted in managed mode: ' +
        'PLATFORM_GATEWAY_URL, INSTANCE_ID and INSTANCE_HMAC_KEY must be set'
    );
  }
  const fetchImpl = options.fetch ?? fetch;
  // Trim trailing slashes without a regex (a `/\/+$/` backtracks on long runs).
  let end = gatewayUrl.length;
  while (end > 0 && gatewayUrl[end - 1] === '/') end -= 1;
  const base = gatewayUrl.slice(0, end);

  async function send(
    method: 'GET' | 'POST',
    route: string,
    body?: Record<string, unknown>,
    signal?: AbortSignal
  ): Promise<Response> {
    // The gateway verifies the router-relative path, and the body as parsed.
    const { authorization } = generateHmacAuthHeaders({
      secretKey: hmacKey!,
      method,
      path: route,
      body,
      keyId: instanceId
    });
    const response = await fetchImpl(`${base}${GATEWAY_MOUNT}${route}`, {
      method,
      headers: {
        authorization,
        ...(body ? { 'content-type': 'application/json' } : {})
      },
      body: body ? JSON.stringify(body) : undefined,
      signal
    });
    if (!response.ok) {
      const message = (await response.text().catch(() => '')).trim();
      const retryAfter = Number(response.headers.get('retry-after'));
      throw new ModelGatewayRequestError(
        response.status,
        message || `Model gateway answered ${response.status}`,
        Number.isFinite(retryAfter) && retryAfter > 0 ? retryAfter : undefined
      );
    }
    return response;
  }

  return {
    async models() {
      return (await (await send('GET', '/models')).json()) as GatewayModels;
    },
    chat: {
      completions: {
        async create(request) {
          const response = await send(
            'POST',
            '/models/chat/completions',
            request as unknown as Record<string, unknown>
          );
          return (await response.json()) as GatewayChatCompletion;
        },
        async *stream(request, streamOptions = {}) {
          const response = await send(
            'POST',
            '/models/chat/completions/stream',
            request as unknown as Record<string, unknown>,
            streamOptions.signal
          );
          if (!response.body) return;
          for await (const data of sseData(response.body)) {
            if (data === '[DONE]') return;
            try {
              yield JSON.parse(data) as GatewayChatChunk;
            } catch {
              // not a chunk; skip it
            }
          }
        }
      }
    }
  };
}

/** The `data:` payloads of a server-sent-event stream, in order. */
async function* sseData(
  stream: ReadableStream<Uint8Array>
): AsyncGenerator<string> {
  const decoder = new TextDecoder();
  let buffer = '';
  const reader = stream.getReader();
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      let boundary: number;
      while ((boundary = buffer.search(/\r?\n\r?\n/)) !== -1) {
        const event = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary).replace(/^\r?\n\r?\n/, '');
        const data = event
          .split(/\r?\n/)
          .filter((line) => line.startsWith('data:'))
          .map((line) => line.slice(5).trimStart())
          .join('\n');
        if (data) yield data;
      }
    }
    const tail = buffer.trim();
    if (tail.startsWith('data:')) yield tail.slice(5).trimStart();
  } finally {
    reader.releaseLock();
  }
}
