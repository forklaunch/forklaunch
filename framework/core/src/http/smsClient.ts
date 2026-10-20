import {
  createInstanceGatewayTransport,
  InstanceGatewayRequestError,
  isManagedInstance,
  type InstanceGatewayOptions,
  type InstanceGatewayTransport,
  type PlatformEvent
} from './instanceGateway';

/**
 * Client for platform-held SMS (AWS End User Messaging SMS), used from inside
 * a managed instance.
 *
 * The instance never holds an AWS credential or a phone number. It signs each
 * send with its own HMAC key; the platform checks the instance's monthly
 * segment cap, its rate limit and its opt-out list, then sends from the
 * product's origination pool under a configuration set named for the
 * instance, so delivery receipts come back to this instance and no other.
 *
 * Events arrive at `POST /platform-events/sms` (verify them with
 * `verifyPlatformEvent`): `sms.delivered`, `sms.failed`, `sms.received`
 * (a reply) and `sms.opted_out` (the person texted STOP; further sends to that
 * number are refused with 422).
 *
 * A text message is not a secure channel: carriers, the handset's lock screen
 * and anyone holding the phone can read it. Never put protected data (a
 * `.deanon` value, a diagnosis, a result) in `body`; send a link to sign in
 * instead. `forklaunch score` flags it (`sms-protected-data`).
 *
 * Locally, run the gateway mock (`npx -p @forklaunch/core
 * forklaunch-gateway-mock`, or the `gateway-mock` compose service that
 * `forklaunch infra add <service> sms` wires) and point PLATFORM_GATEWAY_URL,
 * INSTANCE_ID and INSTANCE_HMAC_KEY at it.
 *
 * @example
 * const sms = createSmsClient();
 * const { messageId, segments } = await sms.send({
 *   to: '+14155550123',
 *   body: 'Your appointment is confirmed. Sign in for details: https://…',
 *   purpose: 'transactional'
 * });
 */

export interface SmsClientOptions extends InstanceGatewayOptions {
  /**
   * Refuse on the first `send` rather than at creation when the service is
   * not a managed instance. For dependency containers that build singletons
   * at boot, so a local run without the gateway still starts.
   */
  deferRefusal?: boolean;
}

export type SmsPurpose = 'transactional' | 'promotional';

export interface SmsSendRequest {
  /** E.164, e.g. `+14155550123`. */
  to: string;
  /** Plain text. At most `SMS_MAX_SEGMENTS` segments. */
  body: string;
  /** Default `transactional`. Promotional texts need prior marketing consent. */
  purpose?: SmsPurpose;
}

export interface SmsSendResult {
  /** The vendor's message id; delivery events carry it. */
  messageId: string;
  /** Billed segments (a long or non-GSM message is several). */
  segments: number;
}

export interface SmsDeliveredData {
  messageId: string;
  to: string;
  deliveredAt?: string;
}
export interface SmsFailedData {
  messageId: string;
  to: string;
  reason: string;
}
export interface SmsReceivedData {
  from: string;
  body: string;
  receivedAt: string;
  /** The recognized keyword (STOP, HELP, START…), when the text was one. */
  keyword?: string;
}
export interface SmsOptedOutData {
  phone: string;
  optedOutAt: string;
}

/** The events the platform delivers for SMS. */
export type SmsEvent =
  | (PlatformEvent<SmsDeliveredData> & { type: 'sms.delivered' })
  | (PlatformEvent<SmsFailedData> & { type: 'sms.failed' })
  | (PlatformEvent<SmsReceivedData> & { type: 'sms.received' })
  | (PlatformEvent<SmsOptedOutData> & { type: 'sms.opted_out' });

/** A refusal or failure from the SMS gateway, with the status it answered. */
export class SmsRequestError extends InstanceGatewayRequestError {
  override readonly name = 'SmsRequestError' as const;
}

/** An input the client refuses before any network call. */
export class SmsValidationError extends Error {
  override readonly name = 'SmsValidationError' as const;
}

export const SMS_MAX_SEGMENTS = 10;

const E164 = /^\+[1-9]\d{6,14}$/;

export function isE164(phone: string): boolean {
  return E164.test(phone);
}

// GSM 03.38 basic set, and the extension set (each counts as two septets).
const GSM_BASIC = new Set(
  '@£$¥èéùìòÇ\nØø\rÅåΔ_ΦΓΛΩΠΨΣΘΞÆæßÉ !"#¤%&\'()*+,-./0123456789:;<=>?¡ABCDEFGHIJKLMNOPQRSTUVWXYZÄÖÑÜ§¿abcdefghijklmnopqrstuvwxyzäöñüà'
);
const GSM_EXTENDED = new Set('^{}\\[~]|€\f');

/**
 * How many segments a body is billed as: GSM-7 is 160 septets in one
 * segment, 153 per segment when split; anything outside GSM-7 is UCS-2, 70
 * characters in one, 67 per segment (UTF-16 code units).
 */
export function smsSegments(body: string): {
  encoding: 'GSM-7' | 'UCS-2';
  units: number;
  segments: number;
} {
  let septets = 0;
  let gsm = true;
  for (const ch of body) {
    if (GSM_BASIC.has(ch)) septets += 1;
    else if (GSM_EXTENDED.has(ch)) septets += 2;
    else {
      gsm = false;
      break;
    }
  }
  if (gsm) {
    return {
      encoding: 'GSM-7',
      units: septets,
      segments: septets <= 160 ? 1 : Math.ceil(septets / 153)
    };
  }
  const units = body.length;
  return {
    encoding: 'UCS-2',
    units,
    segments: units <= 70 ? 1 : Math.ceil(units / 67)
  };
}

/** Check a send before it leaves the process; throws SmsValidationError. */
export function validateSmsSend(request: SmsSendRequest): void {
  if (typeof request.to !== 'string' || !isE164(request.to)) {
    throw new SmsValidationError(
      `'to' must be an E.164 number like +14155550123 (got ${JSON.stringify(request.to)})`
    );
  }
  if (typeof request.body !== 'string' || !request.body.trim()) {
    throw new SmsValidationError("'body' must be non-empty text");
  }
  const { segments, encoding } = smsSegments(request.body);
  if (segments > SMS_MAX_SEGMENTS) {
    throw new SmsValidationError(
      `'body' is ${segments} ${encoding} segments; the limit is ${SMS_MAX_SEGMENTS}. Send a link instead of long text.`
    );
  }
  if (
    request.purpose !== undefined &&
    request.purpose !== 'transactional' &&
    request.purpose !== 'promotional'
  ) {
    throw new SmsValidationError(
      "'purpose' must be 'transactional' or 'promotional'"
    );
  }
}

const NOT_MANAGED =
  'SMS is platform-held: this service must run as a managed instance ' +
  '(PLATFORM_GATEWAY_URL, INSTANCE_ID and INSTANCE_HMAC_KEY set). ' +
  'Locally, run the gateway mock (npx -p @forklaunch/core forklaunch-gateway-mock) ' +
  'and point those variables at it; do not add a Twilio or AWS SDK.';

/**
 * Sends texts through the platform. A class so a dependency container can
 * register it by type (`type: SmsClient`); build it with `createSmsClient`.
 */
export class SmsClient {
  private readonly transport?: InstanceGatewayTransport;

  constructor(options: SmsClientOptions = {}) {
    const configured =
      Boolean(
        (options.gatewayUrl ?? process.env.PLATFORM_GATEWAY_URL) &&
        (options.instanceId ?? process.env.INSTANCE_ID) &&
        (options.hmacKey ?? process.env.INSTANCE_HMAC_KEY)
      ) || isManagedInstance();
    if (configured) {
      this.transport = createInstanceGatewayTransport(options, 'SMS');
    } else if (!options.deferRefusal) {
      throw new Error(NOT_MANAGED);
    }
  }

  /** Whether sends can reach the gateway (false: every send refuses). */
  get managed(): boolean {
    return this.transport !== undefined;
  }

  async send(request: SmsSendRequest): Promise<SmsSendResult> {
    validateSmsSend(request);
    if (!this.transport) throw new Error(NOT_MANAGED);
    try {
      return await this.transport.json<SmsSendResult>('POST', '/sms/send', {
        to: request.to,
        body: request.body,
        purpose: request.purpose ?? 'transactional'
      });
    } catch (error) {
      if (error instanceof InstanceGatewayRequestError) {
        throw new SmsRequestError(
          error.status,
          error.message,
          error.retryAfterSeconds
        );
      }
      throw error;
    }
  }
}

export function createSmsClient(options: SmsClientOptions = {}): SmsClient {
  return new SmsClient(options);
}
