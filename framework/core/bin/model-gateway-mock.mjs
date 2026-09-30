#!/usr/bin/env node
/**
 * A local stand-in for the ForkLaunch model gateway, and for the Azure AI
 * Foundry API behind it. No dependencies, deterministic answers, no network.
 *
 * Two roles, one server:
 *
 *   App developers (a managed template, run locally): point the instance at
 *   it and `createModelGatewayClient()` works as it does when hosted.
 *     PLATFORM_GATEWAY_URL=http://model-gateway-mock:8080
 *     INSTANCE_ID=local-instance
 *     INSTANCE_HMAC_KEY=<same as MOCK_INSTANCE_HMAC_KEY>
 *   Routes: GET  /instance-gateway/models
 *           POST /instance-gateway/models/chat/completions
 *           POST /instance-gateway/models/chat/completions/stream
 *   Requests are HMAC-verified when MOCK_INSTANCE_HMAC_KEY is set, exactly as
 *   the platform verifies them (router-relative path, ±5 minutes, single-use
 *   nonce), so a signing bug shows up locally rather than in production.
 *
 *   Platform developers (the gateway itself, run locally): point the gateway
 *   at it as its Foundry endpoint.
 *     MODEL_GATEWAY_FOUNDRY_BASE_URL=http://model-gateway-mock:8080/openai
 *     AZURE_OPENAI_API_KEY=anything
 *   Routes: POST /openai/v1/chat/completions (stream or not, with usage)
 *           POST /<tenant>/oauth2/v2.0/token (an Entra token endpoint)
 *
 * Answers are `[mock <model>] <the last user message>`; token counts are
 * words, so budgets move predictably. Environment:
 *   PORT                    default 8080
 *   MOCK_INSTANCE_HMAC_KEY  verify instance signatures (unset: accept any)
 *   MOCK_MODELS             comma-separated aliases, default sol,terra,luna
 *   MOCK_MONTHLY_TOKENS     default 2000000
 *   MOCK_FAIL_EVERY         answer 503 to every Nth completion (chaos, default off)
 *
 * Usage: npx -p @forklaunch/core forklaunch-model-gateway-mock
 *    or: node node_modules/@forklaunch/core/bin/model-gateway-mock.mjs
 */
import { createHmac, randomUUID, timingSafeEqual } from 'node:crypto';
import { createServer } from 'node:http';

const PORT = Number(process.env.PORT ?? 8080);
const HMAC_KEY = process.env.MOCK_INSTANCE_HMAC_KEY || '';
const MODELS = (process.env.MOCK_MODELS ?? 'sol,terra,luna')
  .split(',')
  .map((m) => m.trim())
  .filter(Boolean);
const BUDGET = Number(process.env.MOCK_MONTHLY_TOKENS ?? 2_000_000);
const FAIL_EVERY = Number(process.env.MOCK_FAIL_EVERY ?? 0);
const MAX_SKEW_MS = 5 * 60 * 1000;

const seenNonces = new Map();
let tokensUsed = 0;
let completions = 0;

const words = (text) =>
  String(text ?? '')
    .split(/\s+/)
    .filter(Boolean).length;

function lastUserText(messages) {
  const user = [...(messages ?? [])].reverse().find((m) => m?.role === 'user');
  if (!user) return '';
  if (typeof user.content === 'string') return user.content;
  if (Array.isArray(user.content)) {
    return user.content
      .map((part) => (typeof part?.text === 'string' ? part.text : ''))
      .join(' ');
  }
  return '';
}

function answer(body) {
  const model = String(body.model ?? 'mock');
  const prompt = lastUserText(body.messages);
  const content = `[mock ${model}] ${prompt}`.slice(0, 2000);
  const usage = {
    prompt_tokens: (body.messages ?? []).reduce(
      (n, m) => n + words(typeof m?.content === 'string' ? m.content : ''),
      0
    ),
    completion_tokens: words(content)
  };
  usage.total_tokens = usage.prompt_tokens + usage.completion_tokens;
  return { model, content, usage };
}

function completion(body) {
  const { model, content, usage } = answer(body);
  return {
    id: `chatcmpl-mock-${randomUUID()}`,
    object: 'chat.completion',
    created: Math.floor(Date.now() / 1000),
    model,
    choices: [
      {
        index: 0,
        message: { role: 'assistant', content },
        finish_reason: 'stop'
      }
    ],
    usage
  };
}

function* chunks(body) {
  const { model, content, usage } = answer(body);
  const id = `chatcmpl-mock-${randomUUID()}`;
  const created = Math.floor(Date.now() / 1000);
  const base = { id, object: 'chat.completion.chunk', created, model };
  yield { ...base, choices: [{ index: 0, delta: { role: 'assistant' } }] };
  for (const word of content.split(/(?<= )/)) {
    yield { ...base, choices: [{ index: 0, delta: { content: word } }] };
  }
  yield {
    ...base,
    choices: [{ index: 0, delta: {}, finish_reason: 'stop' }]
  };
  if (body.stream_options?.include_usage !== false) {
    yield { ...base, choices: [], usage };
  }
}

/** The platform's check, reimplemented without the framework. */
function verifyHmac(req, rawBody, routerPath) {
  if (!HMAC_KEY) return null;
  const header = req.headers.authorization ?? '';
  const match = /^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(
    header
  );
  if (!match) return 'Missing or malformed HMAC authorization';
  const [, , ts, nonce, signature] = match;
  const timestamp = new Date(ts);
  if (
    Number.isNaN(timestamp.getTime()) ||
    Math.abs(Date.now() - timestamp.getTime()) > MAX_SKEW_MS
  ) {
    return 'Stale or invalid timestamp';
  }
  // createHmacToken: an absent body contributes the string "undefined",
  // a present one its JSON text plus a newline.
  const bodyPart = rawBody
    ? `${JSON.stringify(JSON.parse(rawBody))}\n`
    : 'undefined';
  const expected = createHmac('sha256', HMAC_KEY)
    .update(
      `${req.method}\n${routerPath}\n${bodyPart}${timestamp.toISOString()}\n${nonce}`
    )
    .digest('base64');
  const a = Buffer.from(expected);
  const b = Buffer.from(signature);
  if (a.length !== b.length || !timingSafeEqual(a, b))
    return 'Invalid signature';
  const now = Date.now();
  for (const [seen, at] of seenNonces)
    if (now - at > MAX_SKEW_MS) seenNonces.delete(seen);
  if (seenNonces.has(nonce)) return 'Replayed request';
  seenNonces.set(nonce, now);
  return null;
}

function send(res, status, body, headers = {}) {
  const text = typeof body === 'string' ? body : JSON.stringify(body);
  res.writeHead(status, {
    'content-type':
      typeof body === 'string' ? 'text/plain' : 'application/json',
    ...headers
  });
  res.end(text);
}

function stream(res, body, headers = {}, withIds = false) {
  res.writeHead(200, {
    'content-type': 'text/event-stream',
    'cache-control': 'no-cache',
    ...headers
  });
  let id = 0;
  for (const chunk of chunks(body)) {
    res.write(
      `${withIds ? `id: ${id++}\n` : ''}data: ${JSON.stringify(chunk)}\n\n`
    );
  }
  res.write(`${withIds ? `id: ${id++}\n` : ''}data: [DONE]\n\n`);
  res.end();
}

function account(body) {
  const { usage } = answer(body);
  tokensUsed += usage.total_tokens;
  return {
    'x-forklaunch-model': String(body.model),
    'x-forklaunch-tokens-remaining': String(Math.max(0, BUDGET - tokensUsed))
  };
}

function maybeFail(res) {
  completions += 1;
  if (FAIL_EVERY > 0 && completions % FAIL_EVERY === 0) {
    send(res, 503, 'Mock outage (MOCK_FAIL_EVERY)');
    return true;
  }
  return false;
}

const server = createServer((req, res) => {
  let raw = '';
  req.setEncoding('utf8');
  req.on('data', (chunk) => {
    raw += chunk;
  });
  req.on('end', () => {
    const url = new URL(req.url ?? '/', 'http://mock');
    const path = url.pathname;
    let body = {};
    if (raw) {
      try {
        body = JSON.parse(raw);
      } catch {
        if (!path.endsWith('/token')) return send(res, 400, 'Body is not JSON');
      }
    }

    if (req.method === 'GET' && path === '/health')
      return send(res, 200, { ok: true });

    // --- The instance gateway contract --------------------------------------
    if (path.startsWith('/instance-gateway/')) {
      const routerPath = path.slice('/instance-gateway'.length);
      const refused = verifyHmac(req, raw, routerPath);
      if (refused) return send(res, 401, refused);

      if (req.method === 'GET' && routerPath === '/models') {
        return send(res, 200, {
          models: MODELS.map((id) => ({ id, baa: true })),
          monthlyTokenBudget: BUDGET,
          tokensUsedThisMonth: tokensUsed,
          requestsPerMinute: 600
        });
      }
      if (
        req.method === 'POST' &&
        routerPath.startsWith('/models/chat/completions')
      ) {
        if (!MODELS.includes(body.model)) {
          return send(
            res,
            403,
            `'${body.model}' is not enabled; allowed: ${MODELS.join(', ')}`
          );
        }
        if (!Array.isArray(body.messages) || body.messages.length === 0) {
          return send(res, 400, 'messages must be a non-empty array');
        }
        if (tokensUsed >= BUDGET) {
          return send(res, 429, `Monthly token budget of ${BUDGET} is spent`, {
            'retry-after': '3600'
          });
        }
        if (maybeFail(res)) return;
        const headers = account(body);
        if (routerPath === '/models/chat/completions/stream') {
          return stream(res, body, headers, true);
        }
        if (body.stream === true) {
          return send(
            res,
            400,
            'For streaming, POST /instance-gateway/models/chat/completions/stream'
          );
        }
        return send(res, 200, completion(body), headers);
      }
      return send(res, 404, 'No such gateway route');
    }

    // --- The Foundry (Azure OpenAI v1) API ------------------------------------
    if (req.method === 'POST' && path.endsWith('/v1/chat/completions')) {
      if (!req.headers.authorization?.startsWith('Bearer ')) {
        return send(res, 401, {
          error: { message: 'Missing bearer credential' }
        });
      }
      if (maybeFail(res)) return;
      return body.stream === true
        ? stream(res, body)
        : send(res, 200, completion(body));
    }

    // --- An Entra token endpoint ---------------------------------------------
    if (req.method === 'POST' && /\/oauth2\/v2\.0\/token$/.test(path)) {
      const form = new URLSearchParams(raw);
      if (
        !form.get('client_id') ||
        !(form.get('client_secret') || form.get('client_assertion'))
      ) {
        return send(res, 401, {
          error_description:
            'AADSTS7000215: mock requires a secret or an assertion'
        });
      }
      return send(res, 200, {
        token_type: 'Bearer',
        expires_in: 3600,
        access_token: `mock-entra-token-${randomUUID()}`
      });
    }

    return send(res, 404, 'Not found');
  });
});

server.listen(PORT, () => {
  console.log(
    `model gateway mock on :${PORT} — models ${MODELS.join(', ')}; ` +
      (HMAC_KEY
        ? 'verifying instance signatures'
        : 'NOT verifying signatures (set MOCK_INSTANCE_HMAC_KEY)')
  );
});

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => server.close(() => process.exit(0)));
}
