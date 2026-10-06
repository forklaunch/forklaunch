/**
 * Models: the model gateway (createModelGatewayClient) and, for platform
 * developers, the Azure AI Foundry / Entra APIs behind it.
 *
 * Answers are `[mock <model>] <the last user message>`; token counts are
 * words, so budgets move predictably.
 *
 *   MOCK_MODELS          comma-separated aliases, default sol,terra,luna
 *   MOCK_MONTHLY_TOKENS  default 2000000
 *   MOCK_FAIL_EVERY      answer 503 to every Nth completion (default off)
 */
import { randomUUID } from 'node:crypto';

const env = process.env;
const MODELS = (env.MOCK_MODELS ?? 'sol,terra,luna')
  .split(',')
  .map((m) => m.trim())
  .filter(Boolean);
const BUDGET = Number(env.MOCK_MONTHLY_TOKENS ?? 2_000_000);
const FAIL_EVERY = Number(env.MOCK_FAIL_EVERY ?? 0);

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
  const content = `[mock ${model}] ${lastUserText(body.messages)}`.slice(0, 2000);
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
      { index: 0, message: { role: 'assistant', content }, finish_reason: 'stop' }
    ],
    usage
  };
}

function chunks(body) {
  const { model, content, usage } = answer(body);
  const base = {
    id: `chatcmpl-mock-${randomUUID()}`,
    object: 'chat.completion.chunk',
    created: Math.floor(Date.now() / 1000),
    model
  };
  const out = [{ ...base, choices: [{ index: 0, delta: { role: 'assistant' } }] }];
  for (const word of content.split(/(?<= )/)) {
    out.push({ ...base, choices: [{ index: 0, delta: { content: word } }] });
  }
  out.push({ ...base, choices: [{ index: 0, delta: {}, finish_reason: 'stop' }] });
  if (body.stream_options?.include_usage !== false) {
    out.push({ ...base, choices: [], usage });
  }
  out.push('[DONE]');
  return out;
}

function failed(ctx) {
  completions += 1;
  if (FAIL_EVERY > 0 && completions % FAIL_EVERY === 0) {
    ctx.send(503, 'Mock outage (MOCK_FAIL_EVERY)');
    return true;
  }
  return false;
}

function completionRoute(streaming) {
  return (ctx) => {
    const { body } = ctx;
    if (!MODELS.includes(body.model)) {
      return ctx.send(403, `'${body.model}' is not enabled; allowed: ${MODELS.join(', ')}`);
    }
    if (!Array.isArray(body.messages) || body.messages.length === 0) {
      return ctx.send(400, 'messages must be a non-empty array');
    }
    if (tokensUsed >= BUDGET) {
      return ctx.send(429, `Monthly token budget of ${BUDGET} is spent`, {
        'retry-after': '3600'
      });
    }
    if (failed(ctx)) return;
    tokensUsed += answer(body).usage.total_tokens;
    const headers = {
      'x-forklaunch-model': String(body.model),
      'x-forklaunch-tokens-remaining': String(Math.max(0, BUDGET - tokensUsed))
    };
    if (streaming) return ctx.stream(chunks(body), headers, true);
    if (body.stream === true) {
      return ctx.send(
        400,
        'For streaming, POST /instance-gateway/models/chat/completions/stream'
      );
    }
    return ctx.send(200, completion(body), headers);
  };
}

export default {
  feature: 'models',
  reset() {
    tokensUsed = 0;
    completions = 0;
  },
  routes: [
    {
      method: 'GET',
      path: '/models',
      handler: (ctx) =>
        ctx.send(200, {
          models: MODELS.map((id) => ({ id, baa: true })),
          monthlyTokenBudget: BUDGET,
          tokensUsedThisMonth: tokensUsed,
          requestsPerMinute: 600
        })
    },
    { method: 'POST', path: '/models/chat/completions', handler: completionRoute(false) },
    {
      method: 'POST',
      path: '/models/chat/completions/stream',
      handler: completionRoute(true)
    }
  ],
  vendorRoutes: [
    // The Foundry (Azure OpenAI v1) API.
    {
      method: 'POST',
      path: /\/v1\/chat\/completions$/,
      handler: (ctx) => {
        if (!ctx.req.headers.authorization?.startsWith('Bearer ')) {
          return ctx.send(401, { error: { message: 'Missing bearer credential' } });
        }
        if (failed(ctx)) return;
        return ctx.body.stream === true
          ? ctx.stream(chunks(ctx.body))
          : ctx.send(200, completion(ctx.body));
      }
    },
    // An Entra token endpoint.
    {
      method: 'POST',
      path: /\/oauth2\/v2\.0\/token$/,
      handler: (ctx) => {
        const form = new URLSearchParams(ctx.raw);
        if (
          !form.get('client_id') ||
          !(form.get('client_secret') || form.get('client_assertion'))
        ) {
          return ctx.send(401, {
            error_description: 'AADSTS7000215: mock requires a secret or an assertion'
          });
        }
        return ctx.send(200, {
          token_type: 'Bearer',
          expires_in: 3600,
          access_token: `mock-entra-token-${randomUUID()}`
        });
      }
    }
  ]
};
