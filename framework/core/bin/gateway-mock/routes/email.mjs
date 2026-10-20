/**
 * Email: the platform's SES-backed email gateway (createEmailClient).
 *
 * Messages are recorded, never sent. Outcomes follow the SES mailbox
 * simulator's spirit with addresses anyone can type:
 *   - `bounce@<any>` or any address at `bounce.test`: a permanent bounce
 *     (`email.bounced`), and the address joins the instance's suppression
 *     list — GET /email/suppressions/:address answers true and a later send
 *     to it is refused with 422;
 *   - `complaint@<any>`: delivered, then a complaint (`email.delivered`,
 *     `email.complained`), and the address is suppressed;
 *   - anything else: `email.delivered`.
 * Suppression is applied when the send is accepted; events are delivered
 * just after the send answers, as SES reports them after accepting.
 *
 * Refusals, as the platform answers them: 400 for a malformed request, 422
 * for a suppressed recipient, 429 (with retry-after) past the daily quota.
 * The quota counts recipients, as SES does.
 *
 * Vendor route (unauthenticated): GET /__mock/email/messages[?to=address]
 * lists what was "sent", with its from address and configuration set.
 *
 *   MOCK_EMAIL_DAILY_QUOTA     recipients per UTC day, default 200
 *   MOCK_EMAIL_SENDING_DOMAIN  default mail.forklaunch.test; the from address
 *                              is no-reply@<instance id>.<this>
 *   MOCK_EMAIL_EVENT_DELAY_MS  wait before delivering events, default 0
 */
import { randomUUID } from 'node:crypto';

const env = process.env;
const DAILY_QUOTA = Number(env.MOCK_EMAIL_DAILY_QUOTA ?? 200);
const SENDING_DOMAIN = env.MOCK_EMAIL_SENDING_DOMAIN ?? 'mail.forklaunch.test';
const EVENT_DELAY_MS = Number(env.MOCK_EMAIL_EVENT_DELAY_MS ?? 0);

const LIMITS = {
  maxRecipients: 50,
  maxSubjectLength: 998,
  maxBodyBytes: 512 * 1024,
  maxTags: 10
};
const ADDRESS =
  /^[^\s@<>()[\]\\,;:"]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+$/;
const TAG_PART = /^[A-Za-z0-9_-]{1,256}$/;

const isAddress = (a) =>
  typeof a === 'string' && a.length <= 254 && ADDRESS.test(a);
const today = () => new Date().toISOString().slice(0, 10);
const secondsToMidnight = () => {
  const now = new Date();
  const next = Date.UTC(
    now.getUTCFullYear(),
    now.getUTCMonth(),
    now.getUTCDate() + 1
  );
  return Math.max(1, Math.ceil((next - now.getTime()) / 1000));
};

function outcome(address) {
  const [local, domain] = address.toLowerCase().split('@');
  if (local === 'bounce' || domain === 'bounce.test') return 'bounce';
  if (local === 'complaint') return 'complaint';
  return 'deliver';
}

function init(state) {
  state.messages ??= [];
  state.suppressed ??= new Map();
  state.day ??= today();
  state.sent ??= 0;
  if (state.day !== today()) {
    state.day = today();
    state.sent = 0;
  }
  return state;
}

/** The gateway's validation: a message, or the reason it is refused. */
function validate(body) {
  const list = (v) =>
    v === undefined ? undefined : Array.isArray(v) ? v : [v];
  const to = list(body.to);
  const replyTo = list(body.replyTo);
  if (!to || to.length === 0) return 'to is required';
  if (to.length > LIMITS.maxRecipients)
    return `to has ${to.length} addresses; the limit is ${LIMITS.maxRecipients}`;
  for (const a of [...to, ...(replyTo ?? [])]) {
    if (!isAddress(a))
      return `'${String(a).slice(0, 80)}' is not an email address`;
  }
  if (typeof body.subject !== 'string' || !body.subject.trim())
    return 'subject is required';
  if (/[\r\n]/.test(body.subject)) return 'subject must be one line';
  if (body.subject.length > LIMITS.maxSubjectLength)
    return 'subject is too long';
  if (!body.text && !body.html) return 'text or html is required';
  const bytes =
    Buffer.byteLength(String(body.text ?? '')) +
    Buffer.byteLength(String(body.html ?? ''));
  if (bytes > LIMITS.maxBodyBytes)
    return `body is ${bytes} bytes; the limit is ${LIMITS.maxBodyBytes}`;
  const tags = body.tags ?? {};
  if (typeof tags !== 'object' || Array.isArray(tags))
    return 'tags must be an object';
  if (Object.keys(tags).length > LIMITS.maxTags)
    return `the limit is ${LIMITS.maxTags} tags`;
  for (const [name, value] of Object.entries(tags)) {
    if (!TAG_PART.test(name) || !TAG_PART.test(String(value)))
      return `tag '${name}' is malformed`;
    if (name.startsWith('fl-') || name.startsWith('ses:'))
      return `tag '${name}' is reserved`;
  }
  return { to, replyTo, tags };
}

export default {
  feature: 'email',
  routes: [
    {
      method: 'POST',
      path: '/email/send',
      handler: (ctx) => {
        const state = init(ctx.state);
        const checked = validate(ctx.body);
        if (typeof checked === 'string') return ctx.send(400, checked);
        const { to, replyTo, tags } = checked;
        const refused = to.filter((a) => state.suppressed.has(a.toLowerCase()));
        if (refused.length) {
          return ctx.send(
            422,
            `Suppressed after a bounce or complaint: ${refused.join(', ')}`
          );
        }
        if (state.sent + to.length > DAILY_QUOTA) {
          return ctx.send(
            429,
            `Daily email quota of ${DAILY_QUOTA} recipients is spent`,
            { 'retry-after': String(secondsToMidnight()) }
          );
        }
        state.sent += to.length;
        const messageId = `mock-${randomUUID()}`;
        const configurationSet = `fl-${ctx.instanceId}`;
        state.messages.push({
          messageId,
          instanceId: ctx.instanceId,
          configurationSet,
          from: `no-reply@${ctx.instanceId}.${SENDING_DOMAIN}`,
          to,
          replyTo,
          subject: ctx.body.subject,
          text: ctx.body.text,
          html: ctx.body.html,
          tags,
          at: new Date().toISOString()
        });

        // What SES will report, decided now so a follow-up call sees it.
        const events = [];
        const delivered = to.filter((a) => outcome(a) !== 'bounce');
        const bounced = to.filter((a) => outcome(a) === 'bounce');
        const complained = to.filter((a) => outcome(a) === 'complaint');
        for (const a of bounced)
          state.suppressed.set(a.toLowerCase(), 'bounce');
        for (const a of complained)
          state.suppressed.set(a.toLowerCase(), 'complaint');
        const base = {
          messageId,
          ...(Object.keys(tags).length ? { tags } : {})
        };
        if (delivered.length)
          events.push(['email.delivered', { ...base, recipients: delivered }]);
        if (bounced.length) {
          events.push([
            'email.bounced',
            {
              ...base,
              recipients: bounced,
              bounceType: 'Permanent',
              bounceSubType: 'General',
              permanent: true
            }
          ]);
        }
        if (complained.length) {
          events.push([
            'email.complained',
            { ...base, recipients: complained, feedbackType: 'abuse' }
          ]);
        }

        ctx.send(200, { messageId });
        setTimeout(async () => {
          for (const [type, data] of events) await ctx.emit(type, data);
        }, EVENT_DELAY_MS);
      }
    },
    {
      method: 'GET',
      path: /^\/email\/suppressions\/(?<address>[^/]+)$/,
      handler: (ctx) => {
        const state = init(ctx.state);
        const address = decodeURIComponent(ctx.params.address).toLowerCase();
        if (!isAddress(address)) return ctx.send(400, 'Not an email address');
        const reason = state.suppressed.get(address);
        return ctx.send(200, {
          address,
          suppressed: Boolean(reason),
          ...(reason ? { reason } : {})
        });
      }
    }
  ],
  vendorRoutes: [
    {
      method: 'GET',
      path: '/__mock/email/messages',
      handler: (ctx) => {
        const state = init(ctx.state);
        const to = new URL(ctx.req.url ?? '/', 'http://mock').searchParams.get(
          'to'
        );
        return ctx.send(
          200,
          to
            ? state.messages.filter((m) =>
                m.to.some((a) => a.toLowerCase() === to.toLowerCase())
              )
            : state.messages
        );
      }
    }
  ]
};
