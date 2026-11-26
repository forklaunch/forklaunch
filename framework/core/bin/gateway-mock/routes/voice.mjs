/**
 * Voice: outbound calls (createVoiceClient) as the platform places them
 * through Amazon Connect, and the call events Connect reports back.
 *
 *   POST   /voice/calls       { to, flow, attributes? } -> { callId }
 *   GET    /voice/calls/:id   -> { status, startedAt, endedAt?, durationSeconds?, disconnectReason? }
 *   DELETE /voice/calls/:id   hang up (reason `api`)
 *
 * A call starts `in_progress` and emits `voice.call.started` at once; after
 * MOCK_VOICE_CALL_MS it ends by itself with `voice.call.ended`
 * { callId, durationSeconds, disconnectReason: 'customer' }, then, with
 * MOCK_VOICE_RECORD=1, `voice.recording.ready` { callId, recordingKey }.
 * A number ending in 9999 is busy: it ends at once, reason `busy`, 0 seconds.
 * Events are always delivered in that order.
 *
 * Refusals, as the platform answers them:
 *   400  bad number, a flow not in the catalog, a contact-flow ARN, bad attributes
 *   404  a call this instance did not place
 *   409  hanging up a call that already ended; no number claimed (MOCK_VOICE_NO_NUMBER=1)
 *   429  too many calls in progress, or the month's minutes are spent
 *
 *   MOCK_VOICE_FLOWS             comma-separated flow names, default appointment_reminder
 *   MOCK_VOICE_CALL_MS           how long a call lasts in real time, default 200
 *   MOCK_VOICE_DURATION_SECONDS  the duration a completed call reports, default 42
 *   MOCK_VOICE_MAX_CONCURRENT    calls in progress per instance, default 2
 *   MOCK_VOICE_MONTHLY_MINUTES   billed minutes per instance, default 1000
 *   MOCK_VOICE_RECORD            1: emit voice.recording.ready after each answered call
 *   MOCK_VOICE_NO_NUMBER         1: the instance has no claimed number (409)
 */
import { randomUUID } from 'node:crypto';

const env = process.env;
const FLOWS = (env.MOCK_VOICE_FLOWS ?? 'appointment_reminder')
  .split(',')
  .map((f) => f.trim())
  .filter(Boolean);
const CALL_MS = Number(env.MOCK_VOICE_CALL_MS ?? 200);
const DURATION_SECONDS = Number(env.MOCK_VOICE_DURATION_SECONDS ?? 42);
const MAX_CONCURRENT = Number(env.MOCK_VOICE_MAX_CONCURRENT ?? 2);
const MONTHLY_MINUTES = Number(env.MOCK_VOICE_MONTHLY_MINUTES ?? 1000);
const RECORD = env.MOCK_VOICE_RECORD === '1';
const NO_NUMBER = env.MOCK_VOICE_NO_NUMBER === '1';

// The gateway's attribute limits (VOICE_ATTRIBUTE_LIMITS in the framework).
const MAX_KEYS = 20;
const MAX_KEY_LENGTH = 64;
const MAX_VALUE_LENGTH = 256;
const MAX_TOTAL_BYTES = 4096;

const timers = new Set();

function problem(body) {
  if (typeof body.to !== 'string' || !/^\+[1-9]\d{6,14}$/.test(body.to)) {
    return '`to` must be an E.164 number such as +15551230000';
  }
  if (typeof body.flow !== 'string' || body.flow.startsWith('arn:')) {
    return '`flow` is a flow name from the product catalog, not a contact-flow ARN';
  }
  if (!FLOWS.includes(body.flow)) {
    return `Unknown flow '${body.flow}'; this product's flows: ${FLOWS.join(', ')}`;
  }
  const attributes = body.attributes ?? {};
  if (typeof attributes !== 'object' || Array.isArray(attributes)) {
    return '`attributes` must be an object of strings';
  }
  const entries = Object.entries(attributes);
  if (entries.length > MAX_KEYS) return `at most ${MAX_KEYS} attributes`;
  let total = 0;
  for (const [key, value] of entries) {
    if (!/^[A-Za-z][A-Za-z0-9_]*$/.test(key) || key.length > MAX_KEY_LENGTH) {
      return `attribute key '${key}' must be letters, digits or _`;
    }
    if (key.toLowerCase().startsWith('fl_')) {
      return "attribute keys starting with 'fl_' are reserved for the platform";
    }
    if (typeof value !== 'string') return `attribute '${key}' must be a string`;
    if (value.length > MAX_VALUE_LENGTH) {
      return `attribute '${key}' is longer than ${MAX_VALUE_LENGTH} characters`;
    }
    total += Buffer.byteLength(key) + Buffer.byteLength(value);
  }
  if (total > MAX_TOTAL_BYTES)
    return `attributes exceed ${MAX_TOTAL_BYTES} bytes`;
  return undefined;
}

function calls(state) {
  state.calls ??= new Map();
  return state.calls;
}

function minutesUsed(state, instanceId) {
  state.minutes ??= new Map();
  return state.minutes.get(instanceId) ?? 0;
}

/** End a call once: record it, bill it, then emit ended (after started). */
function finish(ctx, call, reason, durationSeconds) {
  if (call.status === 'ended') return;
  if (call.timer) {
    clearTimeout(call.timer);
    timers.delete(call.timer);
  }
  call.status = 'ended';
  call.endedAt = new Date().toISOString();
  call.durationSeconds = durationSeconds;
  call.disconnectReason = reason;
  ctx.state.minutes ??= new Map();
  ctx.state.minutes.set(
    call.instanceId,
    minutesUsed(ctx.state, call.instanceId) + Math.ceil(durationSeconds / 60)
  );
  call.events = call.events.then(async () => {
    await ctx.emit('voice.call.ended', {
      callId: call.id,
      durationSeconds,
      disconnectReason: reason
    });
    if (RECORD && durationSeconds > 0) {
      await ctx.emit('voice.recording.ready', {
        callId: call.id,
        recordingKey: `voice/recordings/${call.id}.wav`
      });
    }
  });
}

function view(call) {
  return {
    status: call.status,
    startedAt: call.startedAt,
    ...(call.endedAt ? { endedAt: call.endedAt } : {}),
    ...(call.durationSeconds !== undefined
      ? { durationSeconds: call.durationSeconds }
      : {}),
    ...(call.disconnectReason
      ? { disconnectReason: call.disconnectReason }
      : {})
  };
}

function ownCall(ctx) {
  const call = calls(ctx.state).get(ctx.params.id);
  return call && call.instanceId === ctx.instanceId ? call : undefined;
}

async function startCall(ctx) {
  const body = ctx.body ?? {};
  const refused = problem(body);
  if (refused) return ctx.send(400, refused);
  if (NO_NUMBER) {
    return ctx.send(
      409,
      'No phone number is claimed for this instance; the platform claims one when voice is provisioned'
    );
  }
  const active = [...calls(ctx.state).values()].filter(
    (c) => c.instanceId === ctx.instanceId && c.status !== 'ended'
  ).length;
  if (active >= MAX_CONCURRENT) {
    return ctx.send(
      429,
      `${active} calls in progress; this instance may have ${MAX_CONCURRENT} at once`,
      { 'retry-after': '1' }
    );
  }
  if (minutesUsed(ctx.state, ctx.instanceId) >= MONTHLY_MINUTES) {
    return ctx.send(
      429,
      `This month's ${MONTHLY_MINUTES} call minutes are spent`,
      {
        'retry-after': '86400'
      }
    );
  }

  const call = {
    id: `mock-call-${randomUUID()}`,
    instanceId: ctx.instanceId,
    flow: body.flow,
    to: body.to,
    status: 'in_progress',
    startedAt: new Date().toISOString(),
    startedMs: Date.now(),
    events: Promise.resolve()
  };
  calls(ctx.state).set(call.id, call);
  ctx.send(200, { callId: call.id });

  call.events = call.events.then(() =>
    ctx.emit('voice.call.started', { callId: call.id, flow: call.flow })
  );
  if (call.to.endsWith('9999')) {
    finish(ctx, call, 'busy', 0);
    return;
  }
  call.timer = setTimeout(() => {
    timers.delete(call.timer);
    call.timer = undefined;
    finish(ctx, call, 'customer', DURATION_SECONDS);
  }, CALL_MS);
  timers.add(call.timer);
}

function endCall(ctx) {
  const call = ownCall(ctx);
  if (!call) return ctx.send(404, 'No such call for this instance');
  if (call.status === 'ended') return ctx.send(409, 'The call already ended');
  finish(
    ctx,
    call,
    'api',
    Math.max(1, Math.ceil((Date.now() - call.startedMs) / 1000))
  );
  return ctx.send(200, { callId: call.id, status: 'ended' });
}

function getCall(ctx) {
  const call = ownCall(ctx);
  if (!call) return ctx.send(404, 'No such call for this instance');
  return ctx.send(200, view(call));
}

const CALL_ID = /^\/voice\/calls\/(?<id>[A-Za-z0-9_-]{1,128})$/;

export default {
  feature: 'voice',
  routes: [
    { method: 'POST', path: '/voice/calls', handler: startCall },
    { method: 'GET', path: CALL_ID, handler: getCall },
    { method: 'DELETE', path: CALL_ID, handler: endCall }
  ],
  reset() {
    for (const timer of timers) clearTimeout(timer);
    timers.clear();
  }
};
