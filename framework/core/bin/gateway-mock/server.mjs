#!/usr/bin/env node
/**
 * A local stand-in for the ForkLaunch instance gateway — and for the vendors
 * behind it — so managed features run on a laptop and in CI with no accounts,
 * no keys and no network. No dependencies; deterministic answers.
 *
 * Each feature (models, payments, email, sms, whatsapp, voice, …) is one file
 * in ./routes, discovered at start-up. A route file exports:
 *
 *   export default {
 *     feature: 'payments',
 *     // Instance-gateway routes, router-relative (the part after
 *     // /instance-gateway). Requests are HMAC-verified exactly as the platform
 *     // verifies them before the handler runs.
 *     routes: [{ method: 'POST', path: '/payments/checkout-sessions', handler }],
 *     // Optional vendor emulation (Foundry, Entra, …), unauthenticated here.
 *     vendorRoutes: [{ method: 'POST', path: /\/v1\/chat\/completions$/, handler }],
 *     // Optional: reset per-feature state (POST /__mock/reset).
 *     reset() {}
 *   }
 *
 * A handler gets `ctx` = { req, res, body, raw, path, params, instanceId, env,
 * send(status, body, headers), stream(chunks, headers), emit(type, data),
 * state } and answers through `send`/`stream`. `state` is a per-feature object
 * that survives between requests until a reset. `path` may be a string or a
 * RegExp; a RegExp's named groups arrive in `params`.
 *
 * Events: `ctx.emit(type, data)` — or POST /__mock/events/<feature> with
 * { type, data } — delivers a signed platform event to the app at
 * `${MOCK_EVENTS_URL}/platform-events/<feature>`, signed with the instance
 * key under key id `platform`, exactly as the platform does. The app verifies
 * it with `verifyPlatformEvent` from @forklaunch/core/http.
 *
 * Control endpoints (for tests):
 *   GET  /health
 *   GET  /__mock/requests[?feature=x]   gateway calls received, in order
 *   GET  /__mock/events[?feature=x]     events delivered and their outcomes
 *   POST /__mock/events/<feature>       deliver an event now
 *   POST /__mock/reset                  forget requests, events and state
 *
 * Environment:
 *   PORT                    default 8080
 *   MOCK_INSTANCE_HMAC_KEY  verify instance signatures and sign events
 *                           (unset: accept any signature; events unsigned)
 *   MOCK_EVENTS_URL         the app's base URL for event delivery
 *   MOCK_EVENTS_URL_<FEATURE>  per-feature override (the service that declared it)
 *   (plus each feature's own MOCK_* settings; see its route file)
 *
 * Usage: npx -p @forklaunch/core forklaunch-gateway-mock
 */
import { createHmac, randomUUID, timingSafeEqual } from 'node:crypto';
import { readdirSync } from 'node:fs';
import { createServer } from 'node:http';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const MAX_SKEW_MS = 5 * 60 * 1000;
const GATEWAY_MOUNT = '/instance-gateway';

/** createHmacToken's canonical string, reimplemented without the framework. */
function sign(key, method, path, bodyPart, timestamp, nonce) {
  return createHmac('sha256', key)
    .update(`${method}\n${path}\n${bodyPart}${timestamp}\n${nonce}`)
    .digest('base64');
}

export async function loadFeatures(dir = join(HERE, 'routes')) {
  const files = readdirSync(dir)
    .filter((f) => f.endsWith('.mjs'))
    .sort();
  const features = [];
  for (const file of files) {
    const mod = await import(pathToFileURL(join(dir, file)).href);
    if (mod.default?.feature) features.push(mod.default);
  }
  return features;
}

function matchPath(pattern, path) {
  if (typeof pattern === 'string') return pattern === path ? {} : null;
  const m = pattern.exec(path);
  return m ? { ...(m.groups ?? {}) } : null;
}

export async function startGatewayMock(env = process.env) {
  const port = Number(env.PORT ?? 8080);
  const hmacKey = env.MOCK_INSTANCE_HMAC_KEY || '';
  const eventsUrl = env.MOCK_EVENTS_URL || '';
  const features = await loadFeatures();
  const state = new Map(features.map((f) => [f.feature, {}]));
  const seenNonces = new Map();
  let requests = [];
  let events = [];

  function verifyHmac(req, raw, routerPath) {
    if (!hmacKey) return { ok: true, instanceId: 'local-instance' };
    const header = req.headers.authorization ?? '';
    const m = /^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(
      header
    );
    if (!m) return { ok: false, message: 'Missing or malformed HMAC authorization' };
    const [, keyId, ts, nonce, signature] = m;
    const timestamp = new Date(ts);
    if (
      Number.isNaN(timestamp.getTime()) ||
      Math.abs(Date.now() - timestamp.getTime()) > MAX_SKEW_MS
    ) {
      return { ok: false, message: 'Stale or invalid timestamp' };
    }
    // createHmacToken: an absent body contributes the string "undefined",
    // a present one its JSON text plus a newline.
    const bodyPart = raw ? `${JSON.stringify(JSON.parse(raw))}\n` : 'undefined';
    const expected = Buffer.from(
      sign(hmacKey, req.method, routerPath, bodyPart, timestamp.toISOString(), nonce)
    );
    const received = Buffer.from(signature);
    if (expected.length !== received.length || !timingSafeEqual(expected, received)) {
      return { ok: false, message: 'Invalid signature' };
    }
    const now = Date.now();
    for (const [seen, at] of seenNonces) if (now - at > MAX_SKEW_MS) seenNonces.delete(seen);
    if (seenNonces.has(nonce)) return { ok: false, message: 'Replayed request' };
    seenNonces.set(nonce, now);
    return { ok: true, instanceId: keyId };
  }

  async function emit(feature, type, data) {
    const event = {
      id: `evt_mock_${randomUUID()}`,
      feature,
      type,
      occurredAt: new Date().toISOString(),
      data: data ?? {}
    };
    const record = { event, status: null, error: null };
    events.push(record);
    // A feature's events can go to its own service (MOCK_EVENTS_URL_<FEATURE>).
    const target =
      env[`MOCK_EVENTS_URL_${feature.toUpperCase().replace(/-/g, '_')}`] ||
      eventsUrl;
    if (!target) {
      record.error = 'MOCK_EVENTS_URL is not set; event recorded, not delivered';
      return record;
    }
    const path = `/platform-events/${feature}`;
    const body = JSON.stringify(event);
    const headers = { 'content-type': 'application/json' };
    if (hmacKey) {
      const ts = new Date().toISOString();
      const nonce = randomUUID();
      headers.authorization = `HMAC keyId=platform ts=${ts} nonce=${nonce} signature=${sign(hmacKey, 'POST', path, `${body}\n`, ts, nonce)}`;
    }
    try {
      const response = await fetch(`${target.replace(/\/$/, '')}${path}`, {
        method: 'POST',
        headers,
        body
      });
      record.status = response.status;
    } catch (error) {
      record.error = String(error?.message ?? error);
    }
    return record;
  }

  function send(res, status, body, headers = {}) {
    const text = typeof body === 'string' ? body : JSON.stringify(body);
    res.writeHead(status, {
      'content-type': typeof body === 'string' ? 'text/plain' : 'application/json',
      ...headers
    });
    res.end(text);
  }

  function stream(res, chunks, headers = {}, withIds = false) {
    res.writeHead(200, {
      'content-type': 'text/event-stream',
      'cache-control': 'no-cache',
      ...headers
    });
    let id = 0;
    for (const chunk of chunks) {
      res.write(`${withIds ? `id: ${id++}\n` : ''}data: ${typeof chunk === 'string' ? chunk : JSON.stringify(chunk)}\n\n`);
    }
    res.end();
  }

  const server = createServer((req, res) => {
    let raw = '';
    req.setEncoding('utf8');
    req.on('data', (chunk) => {
      raw += chunk;
    });
    req.on('end', async () => {
      try {
        const url = new URL(req.url ?? '/', 'http://mock');
        const path = url.pathname;
        let body = {};
        let parsed = true;
        if (raw) {
          try {
            body = JSON.parse(raw);
          } catch {
            parsed = false;
          }
        }
        const reply = (status, b, h) => send(res, status, b, h);

        if (req.method === 'GET' && path === '/health') return reply(200, { ok: true });
        if (req.method === 'GET' && path === '/__mock/requests') {
          const f = url.searchParams.get('feature');
          return reply(200, f ? requests.filter((r) => r.feature === f) : requests);
        }
        if (req.method === 'GET' && path === '/__mock/events') {
          const f = url.searchParams.get('feature');
          return reply(200, f ? events.filter((e) => e.event.feature === f) : events);
        }
        if (req.method === 'POST' && path === '/__mock/reset') {
          requests = [];
          events = [];
          for (const f of features) {
            state.set(f.feature, {});
            f.reset?.();
          }
          return reply(200, { ok: true });
        }
        const eventMatch = /^\/__mock\/events\/([a-z-]+)$/.exec(path);
        if (req.method === 'POST' && eventMatch) {
          return reply(200, await emit(eventMatch[1], body.type, body.data));
        }

        const ctxBase = (feature, params, instanceId) => ({
          req,
          res,
          body,
          raw,
          path,
          params,
          instanceId,
          env,
          state: state.get(feature),
          send: (status, b, h) => send(res, status, b, h),
          stream: (chunks, h, withIds) => stream(res, chunks, h, withIds),
          emit: (type, data) => emit(feature, type, data)
        });

        if (path.startsWith(`${GATEWAY_MOUNT}/`)) {
          if (!parsed) return reply(400, 'Body is not JSON');
          const routerPath = path.slice(GATEWAY_MOUNT.length);
          const verified = verifyHmac(req, raw, routerPath);
          if (!verified.ok) return reply(401, verified.message);
          for (const feature of features) {
            for (const route of feature.routes ?? []) {
              if (route.method !== req.method) continue;
              const params = matchPath(route.path, routerPath);
              if (!params) continue;
              requests.push({
                feature: feature.feature,
                method: req.method,
                path: routerPath,
                instanceId: verified.instanceId,
                body: raw ? body : undefined,
                at: new Date().toISOString()
              });
              return await route.handler(
                ctxBase(feature.feature, params, verified.instanceId)
              );
            }
          }
          return reply(404, 'No such gateway route');
        }

        for (const feature of features) {
          for (const route of feature.vendorRoutes ?? []) {
            if (route.method !== req.method) continue;
            const params = matchPath(route.path, path);
            if (!params) continue;
            return await route.handler(ctxBase(feature.feature, params, undefined));
          }
        }
        return reply(404, 'Not found');
      } catch (error) {
        if (!res.headersSent) send(res, 500, `Mock error: ${error?.message ?? error}`);
      }
    });
  });

  await new Promise((resolve) => server.listen(port, resolve));
  console.log(
    `gateway mock on :${port} — features ${features.map((f) => f.feature).join(', ')}; ` +
      (hmacKey
        ? 'verifying instance signatures'
        : 'NOT verifying signatures (set MOCK_INSTANCE_HMAC_KEY)') +
      (eventsUrl ? `; events to ${eventsUrl}` : '')
  );
  return server;
}

const isMain =
  process.argv[1] &&
  (fileURLToPath(import.meta.url) === process.argv[1] ||
    process.argv[1].endsWith('forklaunch-gateway-mock'));
if (isMain) {
  const server = await startGatewayMock();
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => server.close(() => process.exit(0)));
  }
}
