/**
 * WhatsApp: the instance gateway's WhatsApp routes (createWhatsAppClient),
 * standing in for the platform and AWS End User Messaging Social / Meta.
 *
 * Gateway routes (HMAC-verified like every gateway route):
 *   GET  /whatsapp/templates   { templates: [{ name, language, status, category }] }
 *   POST /whatsapp/messages    { to, type: 'template', template, language, components? }
 *                              { to, type: 'text', body }
 *                              → { messageId }
 *
 * What it refuses, as the platform does:
 *   400  bad recipient (not E.164), unknown template or language, a template
 *        that is not APPROVED, empty text, an unknown type
 *   403  MOCK_WHATSAPP_HIPAA=1: a HIPAA product may not send WhatsApp at all
 *   409  MOCK_WHATSAPP_UNCONFIGURED=1: no phone number linked for the instance
 *   422  free-form text outside the 24-hour customer-service window
 *   429  more than MOCK_WHATSAPP_RPM sends in a minute (retry-after set)
 *
 * Events (to /platform-events/whatsapp on the app):
 *   whatsapp.status   { messageId, status, recipient, timestamp } — `sent`
 *                     then `delivered` after every accepted send
 *   whatsapp.received { from, text, type, receivedAt, messageId } — on inbound
 *
 * Control (unauthenticated, for tests and local development):
 *   POST /__mock/whatsapp/inbound  { from, text, receivedAt? }
 *        simulates a person writing to the business number: emits
 *        whatsapp.received and opens the 24-hour window for `from` (from
 *        `receivedAt` when given, so an expired window can be simulated).
 *   GET  /__mock/whatsapp/messages the messages sent so far
 *
 * Environment:
 *   MOCK_WHATSAPP_TEMPLATES     name:language:STATUS[:CATEGORY], comma-separated;
 *                               default appointment_reminder:en_US:APPROVED:UTILITY
 *   MOCK_WHATSAPP_HIPAA         1: refuse every send with 403
 *   MOCK_WHATSAPP_UNCONFIGURED  1: refuse every call with 409
 *   MOCK_WHATSAPP_RPM           sends per minute, default 60
 */
import { randomUUID } from 'node:crypto';

const WINDOW_MS = 24 * 60 * 60 * 1000;
const E164 = /^\+[1-9]\d{6,14}$/;

function templatesFrom(env) {
  return String(
    env.MOCK_WHATSAPP_TEMPLATES ?? 'appointment_reminder:en_US:APPROVED:UTILITY'
  )
    .split(',')
    .map((t) => t.trim())
    .filter(Boolean)
    .map((t) => {
      const [name, language = 'en_US', status = 'APPROVED', category] =
        t.split(':');
      return { name, language, status, ...(category ? { category } : {}) };
    });
}

const on = (value) => value === '1' || value === 'true';

function st(ctx) {
  ctx.state.windows ??= {};
  ctx.state.sent ??= [];
  ctx.state.minute ??= { start: 0, count: 0 };
  return ctx.state;
}

/** Platform-level refusals that apply to every call. */
function refused(ctx) {
  if (on(ctx.env.MOCK_WHATSAPP_UNCONFIGURED)) {
    ctx.send(
      409,
      'No WhatsApp phone number is linked for this instance or its product: link a WhatsApp Business Account (Meta embedded signup in AWS End User Messaging Social) and set its phone number id in the product settings'
    );
    return true;
  }
  return false;
}

function statusEvents(ctx, messageId, to) {
  // After the answer, as the vendor reports: sent, then delivered.
  return (async () => {
    for (const status of ['sent', 'delivered']) {
      await ctx.emit('whatsapp.status', {
        messageId,
        status,
        recipient: to,
        timestamp: new Date().toISOString()
      });
    }
  })();
}

async function sendMessage(ctx) {
  if (refused(ctx)) return;
  const state = st(ctx);
  const { body, env } = ctx;
  if (on(env.MOCK_WHATSAPP_HIPAA)) {
    return ctx.send(
      403,
      'WhatsApp is disabled for HIPAA products: it is not covered by the AWS BAA and Meta signs no BAA, so no message may be sent'
    );
  }
  if (typeof body.to !== 'string' || !E164.test(body.to)) {
    return ctx.send(400, "'to' must be an E.164 phone number");
  }
  const rpm = Number(env.MOCK_WHATSAPP_RPM ?? 60);
  const now = Date.now();
  if (now - state.minute.start >= 60_000)
    state.minute = { start: now, count: 0 };
  if (state.minute.count >= rpm) {
    const wait = Math.max(
      1,
      Math.ceil((state.minute.start + 60_000 - now) / 1000)
    );
    return ctx.send(429, `Rate limited: ${rpm} WhatsApp sends per minute`, {
      'retry-after': String(wait)
    });
  }

  let record;
  if (body.type === 'template') {
    const templates = templatesFrom(env);
    const named = templates.filter((t) => t.name === body.template);
    if (named.length === 0) {
      return ctx.send(
        400,
        `Unknown template '${body.template}'; known: ${templates.map((t) => t.name).join(', ')}`
      );
    }
    const template = named.find((t) => t.language === body.language);
    if (!template) {
      return ctx.send(
        400,
        `Template '${body.template}' has no '${body.language}' translation; it has ${named.map((t) => t.language).join(', ')}`
      );
    }
    if (template.status !== 'APPROVED') {
      return ctx.send(
        400,
        `Template '${body.template}' (${body.language}) is ${template.status}, not APPROVED by Meta`
      );
    }
    record = {
      type: 'template',
      template: body.template,
      language: body.language,
      components: body.components ?? []
    };
  } else if (body.type === 'text') {
    if (typeof body.body !== 'string' || !body.body.trim()) {
      return ctx.send(400, 'body must be non-empty text');
    }
    const lastInbound = state.windows[body.to];
    if (!lastInbound || now - lastInbound > WINDOW_MS) {
      return ctx.send(
        422,
        `Free-form text is only allowed within 24 hours of ${body.to}'s last message; send an approved template instead`
      );
    }
    record = { type: 'text', body: body.body };
  } else {
    return ctx.send(400, "type must be 'template' or 'text'");
  }

  state.minute.count += 1;
  const messageId = `wamid.mock-${randomUUID()}`;
  state.sent.push({
    messageId,
    to: body.to,
    instanceId: ctx.instanceId,
    ...record,
    at: new Date(now).toISOString()
  });
  ctx.send(200, { messageId });
  await statusEvents(ctx, messageId, body.to);
}

export default {
  feature: 'whatsapp',
  routes: [
    {
      method: 'GET',
      path: '/whatsapp/templates',
      handler: (ctx) => {
        if (refused(ctx)) return;
        return ctx.send(200, { templates: templatesFrom(ctx.env) });
      }
    },
    { method: 'POST', path: '/whatsapp/messages', handler: sendMessage }
  ],
  vendorRoutes: [
    {
      method: 'POST',
      path: '/__mock/whatsapp/inbound',
      handler: async (ctx) => {
        const state = st(ctx);
        const { from, text } = ctx.body ?? {};
        if (typeof from !== 'string' || !E164.test(from)) {
          return ctx.send(400, "'from' must be an E.164 phone number");
        }
        const at = ctx.body.receivedAt
          ? new Date(ctx.body.receivedAt)
          : new Date();
        if (Number.isNaN(at.getTime()))
          return ctx.send(400, 'receivedAt is not a date');
        state.windows[from] = Math.max(state.windows[from] ?? 0, at.getTime());
        const messageId = `wamid.mock-in-${randomUUID()}`;
        const delivery = await ctx.emit('whatsapp.received', {
          from,
          ...(typeof text === 'string' ? { text } : {}),
          type: typeof text === 'string' ? 'text' : 'unknown',
          receivedAt: at.toISOString(),
          messageId
        });
        return ctx.send(200, {
          messageId,
          delivered: delivery.status,
          error: delivery.error
        });
      }
    },
    {
      method: 'GET',
      path: '/__mock/whatsapp/messages',
      handler: (ctx) => ctx.send(200, st(ctx).sent)
    }
  ]
};
