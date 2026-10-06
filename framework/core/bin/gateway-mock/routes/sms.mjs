/**
 * SMS: the platform's End User Messaging SMS gateway (createSmsClient), and
 * the handset on the other end.
 *
 * Gateway route (HMAC-verified like every gateway route):
 *   POST /sms/send { to, body, purpose? } -> { messageId, segments }
 *     400  `to` is not E.164, the body is empty or over 10 segments, or
 *          `purpose` is not transactional/promotional
 *     422  the number opted out (texted STOP) — the platform refuses it too
 *     429  the monthly segment cap or the per-minute limit is spent
 *          (with retry-after)
 *   After answering, emits `sms.delivered` — or `sms.failed` for a number
 *   ending in 0000, as a carrier would report an unreachable handset.
 *
 * Vendor-side routes (unauthenticated, for tests and local play):
 *   GET  /__mock/sms/messages            every accepted send, in order
 *   POST /__mock/sms/inbound { from, body }
 *        a text arriving from a handset: emits `sms.received`; STOP (and the
 *        other opt-out keywords) also records the opt-out and emits
 *        `sms.opted_out`; START/UNSTOP removes it again. HELP is answered by
 *        the carrier-required help reply and still relayed as received.
 *
 * Settings:
 *   MOCK_SMS_MONTHLY_CAP   segments per calendar month, default 100
 *   MOCK_SMS_PER_MINUTE    sends per minute, default 60
 */
import { randomUUID } from 'node:crypto';

const E164 = /^\+[1-9]\d{6,14}$/;
const MAX_SEGMENTS = 10;
// The keywords AWS End User Messaging treats as opt-out / opt-in by default.
const OPT_OUT = new Set([
  'STOP',
  'STOPALL',
  'UNSUBSCRIBE',
  'CANCEL',
  'END',
  'QUIT',
  'OPTOUT',
  'OPT-OUT',
  'REVOKE'
]);
const OPT_IN = new Set(['START', 'UNSTOP', 'YES']);
const HELP = new Set(['HELP', 'INFO']);

const GSM_BASIC = new Set(
  '@£$¥èéùìòÇ\nØø\rÅåΔ_ΦΓΛΩΠΨΣΘΞÆæßÉ !"#¤%&\'()*+,-./0123456789:;<=>?¡ABCDEFGHIJKLMNOPQRSTUVWXYZÄÖÑÜ§¿abcdefghijklmnopqrstuvwxyzäöñüà'
);
const GSM_EXTENDED = new Set('^{}\\[~]|€\f');

/** The same segment arithmetic as smsSegments in @forklaunch/core/http. */
function segmentsOf(body) {
  let septets = 0;
  for (const ch of body) {
    if (GSM_BASIC.has(ch)) septets += 1;
    else if (GSM_EXTENDED.has(ch)) septets += 2;
    else return body.length <= 70 ? 1 : Math.ceil(body.length / 67);
  }
  return septets <= 160 ? 1 : Math.ceil(septets / 153);
}

const monthOf = (d) =>
  `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, '0')}`;

function secondsUntilNextMonth(now) {
  const next = Date.UTC(now.getUTCFullYear(), now.getUTCMonth() + 1, 1);
  return Math.max(1, Math.ceil((next - now.getTime()) / 1000));
}

function init(state) {
  state.messages ??= [];
  state.optedOut ??= new Set();
  state.segmentsByMonth ??= {};
  state.sendTimes ??= [];
  return state;
}

const keywordOf = (body) =>
  String(body ?? '')
    .trim()
    .split(/\s+/)[0]
    ?.toUpperCase() ?? '';

async function send(ctx) {
  const state = init(ctx.state);
  const { to, body, purpose = 'transactional' } = ctx.body ?? {};
  if (typeof to !== 'string' || !E164.test(to)) {
    return ctx.send(400, `'to' must be an E.164 number like +14155550123`);
  }
  if (typeof body !== 'string' || !body.trim()) {
    return ctx.send(400, "'body' must be non-empty text");
  }
  if (purpose !== 'transactional' && purpose !== 'promotional') {
    return ctx.send(400, "'purpose' must be 'transactional' or 'promotional'");
  }
  const segments = segmentsOf(body);
  if (segments > MAX_SEGMENTS) {
    return ctx.send(
      400,
      `'body' is ${segments} segments; the limit is ${MAX_SEGMENTS}`
    );
  }
  if (state.optedOut.has(to)) {
    return ctx.send(
      422,
      `${to} has opted out of texts from this product (STOP)`
    );
  }

  const now = new Date();
  const perMinute = Number(ctx.env.MOCK_SMS_PER_MINUTE ?? 60);
  state.sendTimes = state.sendTimes.filter((t) => now.getTime() - t < 60_000);
  if (state.sendTimes.length >= perMinute) {
    return ctx.send(429, `Rate limited: ${perMinute} texts per minute`, {
      'retry-after': '60'
    });
  }
  const cap = Number(ctx.env.MOCK_SMS_MONTHLY_CAP ?? 100);
  const month = monthOf(now);
  const used = state.segmentsByMonth[month] ?? 0;
  if (used + segments > cap) {
    return ctx.send(
      429,
      `Monthly SMS cap reached: ${used} of ${cap} segments used this month`,
      { 'retry-after': String(secondsUntilNextMonth(now)) }
    );
  }
  state.segmentsByMonth[month] = used + segments;
  state.sendTimes.push(now.getTime());

  const messageId = `mock-sms-${randomUUID()}`;
  const failed = to.endsWith('0000');
  state.messages.push({
    messageId,
    instanceId: ctx.instanceId,
    to,
    body,
    purpose,
    segments,
    status: failed ? 'failed' : 'delivered',
    sentAt: now.toISOString()
  });
  ctx.send(200, { messageId, segments });

  // The carrier's receipt, after the send was accepted.
  if (failed) {
    await ctx.emit('sms.failed', {
      messageId,
      to,
      reason: 'UNREACHABLE: the handset is not reachable'
    });
  } else {
    await ctx.emit('sms.delivered', {
      messageId,
      to,
      deliveredAt: new Date().toISOString()
    });
  }
}

async function inbound(ctx) {
  const state = init(ctx.state);
  const { from, body } = ctx.body ?? {};
  if (typeof from !== 'string' || !E164.test(from)) {
    return ctx.send(400, "'from' must be an E.164 number");
  }
  if (typeof body !== 'string') {
    return ctx.send(400, "'body' must be text");
  }
  const receivedAt = new Date().toISOString();
  const word = keywordOf(body);
  const keyword =
    OPT_OUT.has(word) || OPT_IN.has(word) || HELP.has(word) ? word : undefined;
  const delivered = [];
  delivered.push(
    await ctx.emit('sms.received', {
      from,
      body,
      receivedAt,
      ...(keyword ? { keyword } : {})
    })
  );
  if (OPT_OUT.has(word)) {
    state.optedOut.add(from);
    delivered.push(
      await ctx.emit('sms.opted_out', { phone: from, optedOutAt: receivedAt })
    );
  } else if (OPT_IN.has(word)) {
    state.optedOut.delete(from);
  }
  return ctx.send(200, {
    keyword: keyword ?? null,
    optedOut: state.optedOut.has(from),
    events: delivered.map((d) => ({ type: d.event.type, status: d.status }))
  });
}

export default {
  feature: 'sms',
  routes: [{ method: 'POST', path: '/sms/send', handler: send }],
  vendorRoutes: [
    {
      method: 'GET',
      path: '/__mock/sms/messages',
      handler: (ctx) => ctx.send(200, init(ctx.state).messages)
    },
    { method: 'POST', path: '/__mock/sms/inbound', handler: inbound }
  ]
};
