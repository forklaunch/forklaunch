import {
  createInstanceGatewayTransport,
  InstanceGatewayRequestError
} from './instanceGateway';
import {
  ModelGatewayRequestError,
  type GatewayChatCompletion,
  type GatewayChatRequest,
  type ModelGatewayClientOptions
} from './modelGatewayClient';

/** A local preview's issued ceilings, not a managed product's monthly usage. */
export interface GatewayPreviewModels {
  models: { id: string }[];
  preview: {
    /** Capability expiration, in Unix milliseconds. */
    expiresAt: number;
    /** Initial request ceiling; this is not a remaining-request count. */
    maxCalls: number;
    /** Initial conservative allowance units, not billable or monthly tokens. */
    maxUnits: number;
  };
}

/** Preview relays may omit a provider timestamp; they must not invent one. */
export interface GatewayPreviewChatCompletion extends Pick<
  GatewayChatCompletion,
  'id' | 'object' | 'model' | 'choices' | 'usage'
> {
  created?: number;
  [key: string]: unknown;
}

export interface PreviewModelGatewayClient {
  models(): Promise<GatewayPreviewModels>;
  chat: {
    completions: {
      create(
        request: GatewayChatRequest
      ): Promise<GatewayPreviewChatCompletion>;
    };
  };
}

function record(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}
function invalidCatalog(): never {
  throw new ModelGatewayRequestError(
    502,
    'The local preview model catalog is invalid or expired.'
  );
}
function previewCatalog(raw: unknown): GatewayPreviewModels {
  if (!record(raw) || !record(raw.preview)) return invalidCatalog();
  const preview = raw.preview;
  if (
    Object.keys(raw).some((key) => !['models', 'preview'].includes(key)) ||
    Object.keys(preview).some(
      (key) => !['expiresAt', 'maxCalls', 'maxUnits'].includes(key)
    ) ||
    !Array.isArray(raw.models) ||
    raw.models.length !== 1 ||
    !record(raw.models[0]) ||
    Object.keys(raw.models[0]).some((key) => key !== 'id') ||
    typeof raw.models[0].id !== 'string' ||
    !/^[\w.:-]{1,160}$/.test(raw.models[0].id) ||
    typeof preview.expiresAt !== 'number' ||
    !Number.isSafeInteger(preview.expiresAt) ||
    preview.expiresAt <= Date.now() ||
    preview.expiresAt > Date.now() + 15 * 60_000 ||
    typeof preview.maxCalls !== 'number' ||
    !Number.isSafeInteger(preview.maxCalls) ||
    preview.maxCalls < 1 ||
    preview.maxCalls > 20 ||
    typeof preview.maxUnits !== 'number' ||
    !Number.isSafeInteger(preview.maxUnits) ||
    preview.maxUnits < 1 ||
    preview.maxUnits > 100_000
  )
    return invalidCatalog();
  return {
    models: [{ id: raw.models[0].id }],
    preview: {
      expiresAt: preview.expiresAt,
      maxCalls: preview.maxCalls,
      maxUnits: preview.maxUnits
    }
  };
}
async function readPreviewCatalog(
  response: Response
): Promise<GatewayPreviewModels> {
  if (!response.body) return invalidCatalog();
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let text = '',
    bytes = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > 8192) return invalidCatalog();
      text += decoder.decode(value, { stream: true });
    }
    text += decoder.decode();
    return previewCatalog(JSON.parse(text));
  } catch {
    return invalidCatalog();
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

/**
 * Use only for the desktop's isolated local preview relay. The desktop supplies
 * PLATFORM_GATEWAY_URL, INSTANCE_ID and a preview-only INSTANCE_HMAC_KEY.
 * This client neither holds the desktop login nor calls a provider directly.
 *
 * Unlike createModelGatewayClient (managed instances), this catalog makes no
 * monthly-token or BAA claims. Streaming is not supported by the preview relay.
 * The gateway still applies its own bounded chat request protocol.
 *
 * @example
 * const client = createPreviewModelGatewayClient();
 * const catalog = await client.models();
 * const reply = await client.chat.completions.create({
 *   model: catalog.models[0].id,
 *   messages: [{ role: 'user', content: 'Help plan my task.' }]
 * });
 * // catalog.preview.maxUnits is an issued ceiling, not remaining monthly usage.
 */
export function createPreviewModelGatewayClient(
  options: ModelGatewayClientOptions = {}
): PreviewModelGatewayClient {
  const transport = createInstanceGatewayTransport(
    options,
    'The local preview model gateway'
  );
  async function send(
    method: 'GET' | 'POST',
    route: string,
    body?: Record<string, unknown>
  ): Promise<Response> {
    try {
      return await transport.request(method, route, body);
    } catch (error) {
      if (error instanceof InstanceGatewayRequestError)
        throw new ModelGatewayRequestError(
          error.status,
          'The local preview model request was refused or is unavailable.',
          error.retryAfterSeconds
        );
      throw error;
    }
  }
  return {
    async models() {
      const response = await send('GET', '/models');
      return readPreviewCatalog(response);
    },
    chat: {
      completions: {
        async create(request) {
          return (await (
            await send(
              'POST',
              '/models/chat/completions',
              request as unknown as Record<string, unknown>
            )
          ).json()) as GatewayPreviewChatCompletion;
        }
      }
    }
  };
}
