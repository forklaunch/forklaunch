import {
  createInstanceGatewayTransport,
  type InstanceGatewayOptions,
  InstanceGatewayRequestError
} from './instanceGateway';

/**
 * Client for WhatsApp through the ForkLaunch instance gateway, used from
 * inside a managed instance.
 *
 * The platform holds the WhatsApp Business Account link (AWS End User
 * Messaging Social) and sends on the instance's behalf from the phone number
 * configured for its product or for this instance. The instance never holds
 * a Meta token: each request is signed with its own HMAC key, and the
 * platform enforces WhatsApp's rules and its own:
 *
 *   - Business-initiated messages must use a Meta-approved template
 *     (`sendTemplate`).
 *   - Free-form text (`sendText`) is allowed only inside the 24-hour
 *     customer-service window, opened by the recipient's last inbound
 *     message. Outside it the gateway answers 422.
 *   - A per-instance rate limit (429 with `retryAfterSeconds`).
 *   - HIPAA products cannot send at all (403): WhatsApp is not covered by
 *     the AWS BAA and Meta signs none. Never put protected health data in a
 *     WhatsApp message.
 *   - No phone number configured for the product or instance: 409.
 *
 * Events arrive at `/platform-events/whatsapp` (see `verifyPlatformEvent`):
 *   - `whatsapp.received` `{ from, text?, type, receivedAt, messageId }`, an
 *     inbound message; it opens the 24-hour window for `from`.
 *   - `whatsapp.status` `{ messageId, status, recipient?, timestamp? }`,
 *     `status` being Meta's `sent`, `delivered`, `read` or `failed`.
 *
 * Locally, `forklaunch infra add <service> whatsapp` points the service at
 * the gateway mock (`forklaunch-gateway-mock`, routes/whatsapp.mjs), which
 * serves templates, emits these events and applies the same refusals.
 *
 * The shape is channel-neutral on purpose (`ConversationChannelClient`):
 * another conversational channel (for example Apple Messages for Business)
 * would be another channel id behind the same three calls.
 *
 * @example
 * const whatsapp = createWhatsAppClient();
 * await whatsapp.sendTemplate({
 *   to: '+14155550123',
 *   template: 'appointment_reminder',
 *   language: 'en_US',
 *   components: [{ type: 'body', parameters: [{ type: 'text', text: 'Tuesday 3pm' }] }]
 * });
 */

export type WhatsAppClientOptions = InstanceGatewayOptions;

/** A Meta template component (header, body, button), passed through as is. */
export interface WhatsAppTemplateComponent {
  type: string;
  parameters?: Record<string, unknown>[];
  [key: string]: unknown;
}

export interface SendTemplateRequest {
  /** E.164 recipient, e.g. `+14155550123`. */
  to: string;
  /** The approved template's name. */
  template: string;
  /** The template's language code, e.g. `en_US`. */
  language: string;
  /** Variable values for the template, in Meta's component format. */
  components?: WhatsAppTemplateComponent[];
}

export interface SendTextRequest {
  /** E.164 recipient; must have written to this number in the last 24 hours. */
  to: string;
  body: string;
}

export interface SentMessage {
  /** Matches `messageId` on later `whatsapp.status` events. */
  messageId: string;
}

export interface MessageTemplate {
  name: string;
  language: string;
  /** Meta's review status: `APPROVED`, `PENDING`, `REJECTED`, … */
  status: string;
  category?: string;
}

/** The data of a `whatsapp.received` event. */
export interface WhatsAppReceivedEvent {
  from: string;
  /** Present for `type: 'text'` messages. */
  text?: string;
  type: string;
  receivedAt: string;
  messageId?: string;
}

/** The data of a `whatsapp.status` event. */
export interface WhatsAppStatusEvent {
  messageId: string;
  status: string;
  recipient?: string;
  timestamp?: string;
}

/** A refusal or failure from the WhatsApp gateway, with its status. */
export class WhatsAppRequestError extends InstanceGatewayRequestError {
  override readonly name = 'WhatsAppRequestError' as const;
}

/**
 * The calls every conversational channel offers. WhatsApp is the only
 * implementation today.
 */
export interface ConversationChannelClient {
  /** Business-initiated message from an approved template. */
  sendTemplate(request: SendTemplateRequest): Promise<SentMessage>;
  /** Free-form text, inside the 24-hour customer-service window only. */
  sendText(request: SendTextRequest): Promise<SentMessage>;
  /** The templates the configured account has, with their review status. */
  templates(): Promise<MessageTemplate[]>;
}

export type WhatsAppClient = ConversationChannelClient;

const E164 = /^\+[1-9]\d{6,14}$/;

function requireRecipient(to: unknown): void {
  if (typeof to !== 'string' || !E164.test(to)) {
    throw new WhatsAppRequestError(
      400,
      `'to' must be an E.164 phone number such as +14155550123 (got ${JSON.stringify(to)})`
    );
  }
}

function createConversationChannelClient(
  channel: string,
  label: string,
  options: WhatsAppClientOptions
): ConversationChannelClient {
  const transport = createInstanceGatewayTransport(options, label);

  async function call<T>(
    method: 'GET' | 'POST',
    route: string,
    body?: Record<string, unknown>
  ): Promise<T> {
    try {
      return await transport.json<T>(method, `/${channel}${route}`, body);
    } catch (error) {
      if (error instanceof InstanceGatewayRequestError) {
        throw new WhatsAppRequestError(
          error.status,
          error.message,
          error.retryAfterSeconds
        );
      }
      throw error;
    }
  }

  return {
    async sendTemplate(request) {
      requireRecipient(request.to);
      if (!request.template || !request.language) {
        throw new WhatsAppRequestError(
          400,
          'template and language are required'
        );
      }
      return call<SentMessage>('POST', '/messages', {
        to: request.to,
        type: 'template',
        template: request.template,
        language: request.language,
        ...(request.components ? { components: request.components } : {})
      });
    },
    async sendText(request) {
      requireRecipient(request.to);
      if (typeof request.body !== 'string' || !request.body.trim()) {
        throw new WhatsAppRequestError(400, 'body must be non-empty text');
      }
      return call<SentMessage>('POST', '/messages', {
        to: request.to,
        type: 'text',
        body: request.body
      });
    },
    async templates() {
      const listing = await call<{ templates: MessageTemplate[] }>(
        'GET',
        '/templates'
      );
      return listing.templates;
    }
  };
}

/**
 * A WhatsApp client for this managed instance. Throws when the service is
 * not running in managed mode (`PLATFORM_GATEWAY_URL`, `INSTANCE_ID`,
 * `INSTANCE_HMAC_KEY` unset); run the local gateway mock to develop.
 */
export function createWhatsAppClient(
  options: WhatsAppClientOptions = {}
): WhatsAppClient {
  return createConversationChannelClient('whatsapp', 'WhatsApp', options);
}
