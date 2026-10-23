import {
  createInstanceGatewayTransport,
  InstanceGatewayRequestError,
  isManagedInstance
} from './instanceGateway';

/**
 * Client for platform-held email through the managed gateway, from a managed
 * instance or any app deployed on ForkLaunch.
 *
 * The instance never holds a mail credential. It signs each request with its
 * own HMAC key; the platform sends through Amazon SES from the instance's own
 * sending identity (`no-reply@<instance>.<platform sending domain>`, a claimed
 * custom domain later) under a per-instance configuration set, enforces the
 * product's daily quota and rate, and refuses addresses on the instance's
 * suppression list (hard bounces and complaints).
 *
 * Delivery outcomes come back as platform events on
 * `/platform-events/email`: `email.delivered`, `email.bounced` and
 * `email.complained`, each carrying the `messageId` `send` returned. A bounced
 * or complained address is suppressed by the platform; mark it undeliverable
 * in the app too so the person can be asked for a new one.
 *
 * Locally, `forklaunch infra add <service> email` points the service at the
 * gateway mock in docker-compose (PLATFORM_GATEWAY_URL, INSTANCE_ID,
 * INSTANCE_HMAC_KEY), which records messages (GET /__mock/email/messages) and
 * emits the same events. Outside managed mode and without those settings,
 * `createEmailClient` refuses to be created.
 *
 * What to put where: the body may hold what the recipient is entitled to
 * read; the subject shows on lock screens, in notification previews and in
 * mail logs, so never put protected data (a `.deanon` value) in it.
 *
 * @example
 * const email = createEmailClient();
 * const { messageId } = await email.send({
 *   to: 'pat@example.com',
 *   subject: 'Your appointment is confirmed',
 *   text: 'See you on Tuesday at 10:00.'
 * });
 * if (await email.suppressed('pat@example.com')) { … ask for another address }
 *
 * @example One-click unsubscribe (RFC 8058), which Gmail and Yahoo require of
 * bulk senders: the platform adds `List-Unsubscribe` and
 * `List-Unsubscribe-Post: List-Unsubscribe=One-Click`.
 * await email.send({
 *   to: member.email,
 *   subject: 'Your weekly recap',
 *   html,
 *   listUnsubscribe: {
 *     url: `https://app.example.com/unsubscribe?token=${token}`, // accepts POST
 *     oneClick: true
 *   }
 * });
 */

export interface EmailClientOptions {
  /** Platform base URL. Default: `PLATFORM_GATEWAY_URL`. */
  gatewayUrl?: string;
  /** This instance's id. Default: `INSTANCE_ID`. */
  instanceId?: string;
  /** This instance's HMAC key. Default: `INSTANCE_HMAC_KEY`. */
  hmacKey?: string;
  fetch?: typeof fetch;
}

export interface EmailSendRequest {
  /** One address or up to 50. */
  to: string | string[];
  /** Plain text, one line; shown in previews and logs — no protected data. */
  subject: string;
  /** At least one of `text` and `html`. */
  text?: string;
  html?: string;
  replyTo?: string | string[];
  /**
   * Up to 10 tags, echoed on the delivery events. Names and values:
   * letters, digits, `_` and `-`. Names starting `fl-` or `ses:` are reserved.
   */
  tags?: Record<string, string>;
  /** `List-Unsubscribe` targets; see {@link EmailListUnsubscribe}. */
  listUnsubscribe?: EmailListUnsubscribe;
}

/**
 * Where a recipient unsubscribes: the `List-Unsubscribe` header (RFC 2369),
 * plus `List-Unsubscribe-Post: List-Unsubscribe=One-Click` (RFC 8058) with
 * `oneClick`. The platform writes the headers; these are targets, not header
 * text.
 *
 * - `url`: https only, printable ASCII up to 512 characters, no `< > , " \`
 *   (percent-encode them). With `oneClick`, mailbox providers POST
 *   `List-Unsubscribe=One-Click` to it (form-encoded, no cookies): it must
 *   unsubscribe from that alone, without a confirmation page.
 * - `mailto`: a plain mailbox (letters, digits, `. _ + -` before the `@`).
 *
 * The headers go to every address in `to`; for a per-recipient link, send
 * one message per recipient.
 */
export type EmailListUnsubscribe =
  | { url: string; mailto?: string; oneClick?: boolean }
  | { url?: undefined; mailto: string; oneClick?: false };

export interface EmailSendResult {
  /** The provider's message id; delivery events carry it. */
  messageId: string;
}

/** Data on `email.delivered`, `email.bounced` and `email.complained` events. */
export interface EmailEventData {
  messageId: string;
  recipients: string[];
  tags?: Record<string, string>;
  /** `email.bounced`: `Permanent`, `Transient` or `Undetermined`. */
  bounceType?: string;
  bounceSubType?: string;
  /** `email.bounced`: whether the platform suppressed the addresses. */
  permanent?: boolean;
  /** `email.complained`: the feedback type the mailbox provider reported. */
  feedbackType?: string;
}

/** A refusal or failure, from the gateway or from validation before sending. */
export class EmailRequestError extends InstanceGatewayRequestError {
  override readonly name = 'EmailRequestError' as const;
}

export interface EmailClient {
  /** Send one message; throws `EmailRequestError` on a refusal. */
  send(request: EmailSendRequest): Promise<EmailSendResult>;
  /** Whether the platform refuses to send to `address` (bounce/complaint). */
  suppressed(address: string): Promise<boolean>;
}

/** Limits the platform enforces too; checked here so a bad call fails fast. */
export const EMAIL_LIMITS = {
  maxRecipients: 50,
  maxAddressLength: 254,
  maxSubjectLength: 998,
  /** text + html, UTF-8 bytes. */
  maxBodyBytes: 512 * 1024,
  maxTags: 10,
  /** `listUnsubscribe.url`; keeps the header within SES's limit. */
  maxUnsubscribeUrlLength: 512
} as const;

const ADDRESS =
  /^[^\s@<>()[\]\\,;:"]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+$/;
const TAG_PART = /^[A-Za-z0-9_-]{1,256}$/;
/** Printable ASCII, no space: nothing that can end or fold a header line. */
const HEADER_SAFE = /^[\x21-\x7e]+$/;
/** Delimiters of the header's `<…>, <…>` list; percent-encode them instead. */
const URL_DELIMITERS = /[<>,"\\]/;
const MAILBOX =
  /^[A-Za-z0-9._+-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+$/;
const UNSUBSCRIBE_FIELDS = new Set(['url', 'mailto', 'oneClick']);

/** Whether `address` is a plain mailbox address (`local@domain.tld`). */
export function isEmailAddress(address: unknown): address is string {
  return (
    typeof address === 'string' &&
    address.length <= EMAIL_LIMITS.maxAddressLength &&
    ADDRESS.test(address)
  );
}

function refuse(message: string): never {
  throw new EmailRequestError(400, message);
}

function addresses(value: string | string[] | undefined, field: string) {
  if (value === undefined) return undefined;
  const list = Array.isArray(value) ? value : [value];
  if (list.length === 0) refuse(`${field} must name at least one address`);
  if (list.length > EMAIL_LIMITS.maxRecipients) {
    refuse(
      `${field} has ${list.length} addresses; the limit is ${EMAIL_LIMITS.maxRecipients}`
    );
  }
  for (const address of list) {
    if (!isEmailAddress(address)) {
      refuse(
        `${field}: '${String(address).slice(0, 80)}' is not an email address`
      );
    }
  }
  return list;
}

function fail(problem: string): never {
  refuse(`listUnsubscribe: ${problem}`);
}

function unsubscribeUrlProblem(url: unknown): string | null {
  if (typeof url !== 'string' || url === '') return 'url must be a string';
  if (url.length > EMAIL_LIMITS.maxUnsubscribeUrlLength) {
    return `url is longer than ${EMAIL_LIMITS.maxUnsubscribeUrlLength} characters`;
  }
  if (!HEADER_SAFE.test(url)) {
    return 'url must be printable ASCII with no spaces or line breaks; percent-encode anything else';
  }
  if (URL_DELIMITERS.test(url)) {
    return 'url must not contain < > , " or \\; percent-encode them';
  }
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return 'url is not a URL';
  }
  if (parsed.protocol !== 'https:') return 'url must be https';
  if (parsed.username || parsed.password) {
    return 'url must not carry a user name or password';
  }
  return null;
}

/**
 * Check `listUnsubscribe` the way the gateway does; returns its wire form.
 * At least one of `url` and `mailto`; `oneClick` only with `url`.
 */
export function validateListUnsubscribe(
  value: unknown
): EmailListUnsubscribe | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== 'object' || Array.isArray(value)) {
    fail('must be an object');
  }
  const unknownFields = Object.keys(value).filter(
    (key) => !UNSUBSCRIBE_FIELDS.has(key)
  );
  if (unknownFields.length) {
    fail(`unknown field ${unknownFields.map((k) => `'${k}'`).join(', ')}`);
  }
  const url = 'url' in value ? value.url : undefined;
  const mailto = 'mailto' in value ? value.mailto : undefined;
  const oneClick = 'oneClick' in value ? value.oneClick : undefined;
  if (url === undefined && mailto === undefined) {
    fail('needs a url or a mailto');
  }
  if (url !== undefined) {
    const problem = unsubscribeUrlProblem(url);
    if (problem) fail(problem);
  }
  if (
    mailto !== undefined &&
    (typeof mailto !== 'string' ||
      mailto.length > EMAIL_LIMITS.maxAddressLength ||
      !MAILBOX.test(mailto))
  ) {
    fail(
      `mailto: '${String(mailto).slice(0, 80)}' is not a plain mailbox (letters, digits, . _ + - before the @)`
    );
  }
  if (oneClick !== undefined && typeof oneClick !== 'boolean') {
    fail('oneClick must be true or false');
  }
  if (typeof url === 'string') {
    return {
      url,
      ...(typeof mailto === 'string' ? { mailto } : {}),
      ...(oneClick === true ? { oneClick: true } : {})
    };
  }
  if (oneClick === true) fail('oneClick needs a url (RFC 8058 posts to it)');
  if (typeof mailto === 'string') return { mailto };
  return fail('needs a url or a mailto');
}

/** Validate a send request the way the gateway does; returns the wire body. */
export function validateEmailSendRequest(
  request: EmailSendRequest
): Record<string, unknown> {
  const to = addresses(request.to, 'to') ?? refuse('to is required');
  const replyTo = addresses(request.replyTo, 'replyTo');
  const subject = request.subject;
  if (typeof subject !== 'string' || subject.trim() === '') {
    refuse('subject is required');
  }
  if (/[\r\n]/.test(subject)) refuse('subject must be one line');
  if (subject.length > EMAIL_LIMITS.maxSubjectLength) {
    refuse(
      `subject is longer than ${EMAIL_LIMITS.maxSubjectLength} characters`
    );
  }
  if (!request.text && !request.html) refuse('text or html is required');
  const bytes =
    Buffer.byteLength(request.text ?? '', 'utf8') +
    Buffer.byteLength(request.html ?? '', 'utf8');
  if (bytes > EMAIL_LIMITS.maxBodyBytes) {
    refuse(`body is ${bytes} bytes; the limit is ${EMAIL_LIMITS.maxBodyBytes}`);
  }
  const tags = request.tags ?? {};
  const names = Object.keys(tags);
  if (names.length > EMAIL_LIMITS.maxTags) {
    refuse(`${names.length} tags; the limit is ${EMAIL_LIMITS.maxTags}`);
  }
  for (const name of names) {
    if (!TAG_PART.test(name) || !TAG_PART.test(String(tags[name]))) {
      refuse(`tag '${name}': names and values are letters, digits, _ and -`);
    }
    if (name.startsWith('fl-') || name.startsWith('ses:')) {
      refuse(`tag '${name}' is reserved`);
    }
  }
  const listUnsubscribe = validateListUnsubscribe(request.listUnsubscribe);
  return {
    to,
    subject,
    ...(request.text !== undefined ? { text: request.text } : {}),
    ...(request.html !== undefined ? { html: request.html } : {}),
    ...(replyTo ? { replyTo } : {}),
    ...(names.length ? { tags } : {}),
    ...(listUnsubscribe ? { listUnsubscribe } : {})
  };
}

export function createEmailClient(
  options: EmailClientOptions = {}
): EmailClient {
  if (
    !isManagedInstance({
      PLATFORM_GATEWAY_URL:
        options.gatewayUrl ?? process.env.PLATFORM_GATEWAY_URL,
      INSTANCE_ID: options.instanceId ?? process.env.INSTANCE_ID,
      INSTANCE_HMAC_KEY: options.hmacKey ?? process.env.INSTANCE_HMAC_KEY
    })
  ) {
    throw new Error(
      'Email is sent by the ForkLaunch platform: PLATFORM_GATEWAY_URL, INSTANCE_ID and INSTANCE_HMAC_KEY must be set. ' +
        'A managed instance and an app deployed with `forklaunch deploy create` get them from the platform at deploy. ' +
        'Locally, `forklaunch infra add <service> email` sets them to the gateway mock in docker-compose.'
    );
  }
  const transport = createInstanceGatewayTransport(options, 'Email');

  async function call<T>(
    method: 'GET' | 'POST',
    route: string,
    body?: Record<string, unknown>
  ): Promise<T> {
    try {
      return await transport.json<T>(method, route, body);
    } catch (error) {
      if (error instanceof InstanceGatewayRequestError) {
        throw new EmailRequestError(
          error.status,
          error.message,
          error.retryAfterSeconds
        );
      }
      throw error;
    }
  }

  return {
    async send(request) {
      return call<EmailSendResult>(
        'POST',
        '/email/send',
        validateEmailSendRequest(request)
      );
    },
    async suppressed(address) {
      if (!isEmailAddress(address)) {
        refuse(`'${String(address).slice(0, 80)}' is not an email address`);
      }
      const answer = await call<{ suppressed: boolean }>(
        'GET',
        `/email/suppressions/${encodeURIComponent(address.toLowerCase())}`
      );
      return answer.suppressed === true;
    }
  };
}
