/**
 * Payments: Stripe Connect through the instance gateway (createPaymentsClient
 * and createStripeClient from @forklaunch/core/http).
 *
 * Gateway routes, HMAC-verified like the platform's:
 *   POST /payments/onboarding-link  { returnUrl, refreshUrl } -> { url }
 *                                   creates the instance's connected account
 *                                   on first use (acct_mock_<n>)
 *   GET  /payments/status           { chargesEnabled, payoutsEnabled, requirementsDue }
 *   *    /stripe/v1/...             the Stripe API, as the real Stripe SDK
 *                                   sends it (form-encoded, signed verbatim)
 *
 * The proxy does what the platform does, in the same order: refuses a call
 * before onboarding (409), forwards only the allowlist below (403 otherwise:
 * never payouts, transfers, external accounts or account changes), refuses
 * metadata keys named like protected data and SSN-shaped text (400), applies
 * the rate limit (429), pins `Stripe-Account` to the instance's account
 * (overwriting any the app sent), adds the application fee, and answers with
 * Stripe-shaped objects kept in memory: enough for customers, products and
 * prices, checkout and billing-portal sessions, payment links, subscriptions,
 * payment intents, refunds and invoices. Refusals use Stripe's error body, so
 * the SDK raises StripePermissionError / StripeInvalidRequestError / …
 * Every call's /__mock/requests entry gets `forwarded` = { method, path,
 * stripeAccount, sentStripeAccount, body } — what went upstream.
 *
 * Events (to MOCK_EVENTS_URL_PAYMENTS / MOCK_EVENTS_URL, as platform events):
 *   account.updated              when onboarding completes
 *   checkout.session.completed   POST /__mock/payments/complete/<session id>
 *                                (pays the session: payment_status=paid, and
 *                                a subscription for mode=subscription)
 *   or anything via POST /__mock/events/payments { type, data }.
 * Other vendor routes: POST /__mock/payments/enable (finish onboarding by hand).
 *
 *   MOCK_PAYMENTS_AUTO_ENABLE   onboarding completes as soon as a link is made
 *                               (default 1; 0 = wait for /__mock/payments/enable)
 *   MOCK_PAYMENTS_FEE_PERCENT   the product's application fee, percent (default none)
 *   MOCK_PAYMENTS_FEE_AMOUNT    or a flat fee in minor units (default none)
 *   MOCK_PAYMENTS_RPM           Stripe calls per instance per minute (default 300)
 *   MOCK_STRIPE_UPSTREAM        forward allowlisted calls to a Stripe-compatible
 *                               server instead of answering in memory, e.g.
 *                               http://localhost:12111 (docker stripe/stripe-mock)
 *   MOCK_STRIPE_UPSTREAM_KEY    the key sent upstream (default sk_test_123)
 */
import { randomBytes } from 'node:crypto';

const env = process.env;
const AUTO_ENABLE = !['0', 'false', 'no'].includes(
  String(env.MOCK_PAYMENTS_AUTO_ENABLE ?? '1').toLowerCase()
);
const RPM = Number(env.MOCK_PAYMENTS_RPM ?? 300);
const FEE = {
  percent: env.MOCK_PAYMENTS_FEE_PERCENT ? Number(env.MOCK_PAYMENTS_FEE_PERCENT) : undefined,
  amount: env.MOCK_PAYMENTS_FEE_AMOUNT ? Number(env.MOCK_PAYMENTS_FEE_AMOUNT) : undefined
};
const UPSTREAM = env.MOCK_STRIPE_UPSTREAM?.replace(/\/+$/, '');
const UPSTREAM_KEY = env.MOCK_STRIPE_UPSTREAM_KEY ?? 'sk_test_123';

// ---------------------------------------------------------------- policy
// Mirrors forklaunch-platform managed-apps domain/services/payments-policy.ts.
// Generated from what @forklaunch/implementation-billing-stripe and the
// billing-stripe / ecommerce-stripe blueprints call, plus refunds and invoice
// reads.
const ID = '[A-Za-z0-9_]+';
export const STRIPE_ALLOWLIST = [
  ['POST', '/v1/customers'],
  ['GET', `/v1/customers/${ID}`],
  ['POST', `/v1/customers/${ID}`],
  ['POST', '/v1/products'],
  ['GET', '/v1/products/search'],
  ['GET', `/v1/products/${ID}`],
  ['POST', `/v1/products/${ID}`],
  ['POST', '/v1/prices'],
  ['GET', '/v1/prices'],
  ['GET', `/v1/prices/${ID}`],
  ['POST', `/v1/prices/${ID}`],
  ['POST', '/v1/plans'],
  ['GET', `/v1/plans/${ID}`],
  ['DELETE', `/v1/plans/${ID}`],
  ['POST', '/v1/checkout/sessions'],
  ['GET', `/v1/checkout/sessions/${ID}`],
  ['POST', `/v1/checkout/sessions/${ID}`],
  ['POST', `/v1/checkout/sessions/${ID}/expire`],
  ['POST', '/v1/billing_portal/sessions'],
  ['POST', '/v1/payment_links'],
  ['GET', '/v1/payment_links'],
  ['GET', `/v1/payment_links/${ID}`],
  ['POST', `/v1/payment_links/${ID}`],
  ['POST', '/v1/subscriptions'],
  ['GET', `/v1/subscriptions/${ID}`],
  ['POST', `/v1/subscriptions/${ID}`],
  ['DELETE', `/v1/subscriptions/${ID}`],
  ['POST', `/v1/subscriptions/${ID}/resume`],
  ['POST', '/v1/payment_intents'],
  ['GET', `/v1/payment_intents/${ID}`],
  ['POST', '/v1/payment_methods'],
  ['POST', '/v1/tax/calculations'],
  ['GET', '/v1/invoices'],
  ['GET', `/v1/invoices/${ID}`],
  ['POST', '/v1/refunds'],
  ['GET', `/v1/refunds/${ID}`]
].map(([method, pattern]) => ({ method, re: new RegExp(`^${pattern}$`) }));

export function isAllowedStripeCall(method, path) {
  return STRIPE_ALLOWLIST.some((a) => a.method === method && a.re.test(path));
}

// Metadata keys named like protected data. The gateway cannot see where a
// value came from (`.deanon` is app-side), so it refuses by NAME; the CLI's
// `payments-protected-data` check is the real guard.
const DENY_WORDS = new Set(['ssn', 'dob', 'mrn', 'icd', 'icd10', 'phi', 'pii', 'npi']);
const DENY_FRAGMENTS = [
  'socialsecurity',
  'dateofbirth',
  'birthdate',
  'diagnos',
  'medicalrecord',
  'medication',
  'prescription',
  'allerg',
  'insurance',
  'symptom',
  'treatment',
  'labresult',
  'healthcondition',
  'patientname'
];
const SSN = /\b\d{3}-\d{2}-\d{4}\b/;

function keyWords(key) {
  return key
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
}

export function protectedDataViolation(params) {
  for (const [key, value] of params) {
    const meta = /(?:^metadata|\[metadata\])\[([^\]]+)\]$/.exec(key);
    if (meta) {
      const words = keyWords(meta[1]);
      const joined = words.join('');
      if (
        words.some((w) => DENY_WORDS.has(w)) ||
        DENY_FRAGMENTS.some((f) => joined.includes(f))
      ) {
        return `metadata key '${meta[1]}' looks like protected health or personal data; keep it in the app and reference it by an opaque id`;
      }
    }
    const field = /(?:^|\[)(description|statement_descriptor(?:_suffix)?)\]?$/.exec(key);
    if ((meta || field) && SSN.test(value)) {
      return `${key} contains an SSN-shaped value`;
    }
  }
  return null;
}

/** The product's application fee, set on the calls that create a charge. */
export function applyApplicationFee(method, path, params, fee) {
  if (method !== 'POST' || (fee.percent == null && fee.amount == null)) return;
  const put = (k, v) => params.set(k, String(v));
  if (path === '/v1/checkout/sessions') {
    if (params.get('mode') === 'subscription') {
      if (fee.percent != null) put('subscription_data[application_fee_percent]', fee.percent);
    } else if (fee.amount != null) {
      put('payment_intent_data[application_fee_amount]', fee.amount);
    }
  } else if (path === '/v1/payment_intents') {
    const amount = Number(params.get('amount') ?? 0);
    const value = fee.amount ?? Math.round((amount * fee.percent) / 100);
    put('application_fee_amount', value);
  } else if (path === '/v1/subscriptions' && fee.percent != null) {
    put('application_fee_percent', fee.percent);
  }
}

// ------------------------------------------------------------- form + ids

/** Stripe's bracketed form encoding -> nested objects and arrays. */
export function parseForm(raw) {
  const out = {};
  for (const [key, value] of new URLSearchParams(raw ?? '')) {
    const parts = key.split(/\[|\]\[|\]/).filter((p, i) => i === 0 || p !== '');
    let node = out;
    parts.forEach((part, i) => {
      const last = i === parts.length - 1;
      const nextIsIndex = !last && /^\d+$/.test(parts[i + 1]);
      if (last) {
        node[part] = value;
      } else {
        node[part] ??= nextIsIndex ? [] : {};
        node = node[part];
      }
    });
  }
  return coerce(out);
}

function coerce(value, inMetadata = false) {
  if (Array.isArray(value)) return value.map((v) => coerce(v, inMetadata));
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([k, v]) => [k, coerce(v, inMetadata || k === 'metadata')])
    );
  }
  if (inMetadata || typeof value !== 'string') return value;
  if (/^-?\d+$/.test(value)) return Number(value);
  if (value === 'true' || value === 'false') return value === 'true';
  return value;
}

const rid = (prefix) => `${prefix}_${randomBytes(12).toString('hex').slice(0, 24)}`;
const now = () => Math.floor(Date.now() / 1000);

// --------------------------------------------------------------- objects

const RESOURCES = {
  customers: { prefix: 'cus', object: 'customer' },
  products: { prefix: 'prod', object: 'product', base: { active: true } },
  prices: { prefix: 'price', object: 'price', base: { active: true, currency: 'usd' } },
  plans: { prefix: 'plan', object: 'plan', base: { active: true } },
  'checkout/sessions': { prefix: 'cs_test', object: 'checkout.session' },
  'billing_portal/sessions': { prefix: 'bps', object: 'billing_portal.session' },
  payment_links: { prefix: 'plink', object: 'payment_link', base: { active: true } },
  subscriptions: { prefix: 'sub', object: 'subscription' },
  payment_intents: { prefix: 'pi', object: 'payment_intent' },
  payment_methods: { prefix: 'pm', object: 'payment_method' },
  'tax/calculations': { prefix: 'taxcalc', object: 'tax.calculation' },
  invoices: { prefix: 'in', object: 'invoice' },
  refunds: { prefix: 're', object: 'refund' }
};
const RESOURCE_KEYS = Object.keys(RESOURCES).sort((a, b) => b.length - a.length);

function splitPath(path) {
  const rest = path.replace(/^\/v1\//, '');
  const key = RESOURCE_KEYS.find((k) => rest === k || rest.startsWith(`${k}/`));
  if (!key) return null;
  const [id, action] = rest.slice(key.length + 1).split('/').filter(Boolean);
  return { key, id, action };
}

function lineItemsTotal(store, items = []) {
  return items.reduce((sum, item) => {
    const price = store.objects.get(item.price);
    return sum + Number(price?.unit_amount ?? 0) * Number(item.quantity ?? 1);
  }, 0);
}

function created(store, key, params, account) {
  const spec = RESOURCES[key];
  const id = rid(spec.prefix);
  const obj = {
    id,
    object: spec.object,
    created: now(),
    livemode: false,
    metadata: {},
    ...spec.base,
    ...params
  };
  delete obj.expand;
  switch (key) {
    case 'prices':
      obj.type = params.recurring ? 'recurring' : 'one_time';
      break;
    case 'checkout/sessions':
      Object.assign(obj, {
        status: 'open',
        payment_status: 'unpaid',
        mode: params.mode ?? 'payment',
        amount_total: lineItemsTotal(store, params.line_items),
        currency: 'usd',
        url: `https://checkout.stripe.com/c/pay/${id}`,
        payment_intent: null,
        subscription: null
      });
      delete obj.line_items;
      break;
    case 'billing_portal/sessions':
      obj.url = `https://billing.stripe.com/p/session/test_${id}`;
      break;
    case 'payment_links':
      obj.url = `https://buy.stripe.com/test_${id}`;
      delete obj.line_items;
      break;
    case 'subscriptions':
      Object.assign(obj, {
        status: 'active',
        current_period_start: now(),
        items: {
          object: 'list',
          data: (params.items ?? []).map((item) => ({
            id: rid('si'),
            object: 'subscription_item',
            price: store.objects.get(item.price) ?? { id: item.price },
            quantity: item.quantity ?? 1
          }))
        }
      });
      break;
    case 'payment_intents':
      Object.assign(obj, {
        status: 'requires_payment_method',
        client_secret: `${id}_secret_${randomBytes(8).toString('hex')}`
      });
      break;
    case 'tax/calculations':
      Object.assign(obj, {
        amount_total: (params.line_items ?? []).reduce((s, l) => s + Number(l.amount ?? 0), 0),
        tax_amount_exclusive: 0,
        tax_amount_inclusive: 0
      });
      break;
    case 'refunds':
      obj.status = 'succeeded';
      break;
  }
  obj.__account = account;
  store.objects.set(id, obj);
  return obj;
}

const visible = (obj) => {
  if (!obj) return obj;
  const { __account, ...rest } = obj;
  return rest;
};

function missing(ctx, id) {
  return stripeError(ctx, 404, 'invalid_request_error', `No such object: '${id}'`, 'resource_missing');
}

function stripeError(ctx, status, type, message, code, headers = {}) {
  return ctx.send(
    status,
    { error: { type, message, ...(code ? { code } : {}) } },
    { 'stripe-should-retry': 'false', ...headers }
  );
}

function answerInMemory(ctx, method, path, params, account) {
  const store = ctx.state;
  const where = splitPath(path);
  if (!where) return stripeError(ctx, 404, 'invalid_request_error', `Unrecognized request URL (${method}: ${path})`);
  const { key, id, action } = where;
  const owned = (o) => o && o.__account === account;
  const list = () =>
    [...store.objects.values()].filter(
      (o) => owned(o) && o.object === RESOURCES[key].object
    );

  if (method === 'POST' && !id) return ctx.send(200, visible(created(store, key, params, account)));
  if (method === 'GET' && !id) {
    let data = list();
    if (params.active !== undefined) data = data.filter((o) => o.active === params.active);
    if (params.product) data = data.filter((o) => o.product === params.product);
    const limit = Number(params.limit ?? 10);
    return ctx.send(200, {
      object: 'list',
      url: path,
      has_more: data.length > limit,
      data: data.slice(0, limit).map(visible)
    });
  }
  if (method === 'GET' && id === 'search') {
    const data = list();
    return ctx.send(200, { object: 'search_result', url: path, has_more: false, data: data.map(visible) });
  }
  const obj = store.objects.get(id);
  if (!owned(obj)) return missing(ctx, id);
  if (method === 'GET') return ctx.send(200, visible(obj));
  if (method === 'DELETE') {
    if (key === 'subscriptions') {
      obj.status = 'canceled';
      obj.canceled_at = now();
      return ctx.send(200, visible(obj));
    }
    store.objects.delete(id);
    return ctx.send(200, { id, object: RESOURCES[key].object, deleted: true });
  }
  if (action === 'expire') obj.status = 'expired';
  else if (action === 'resume') obj.status = 'active';
  else {
    const { metadata, expand, ...rest } = params;
    Object.assign(obj, rest);
    if (metadata) obj.metadata = { ...obj.metadata, ...metadata };
  }
  return ctx.send(200, visible(obj));
}

async function answerUpstream(ctx, method, path, query, form, account) {
  const headers = {
    authorization: `Bearer ${UPSTREAM_KEY}`,
    'stripe-account': account
  };
  for (const h of ['stripe-version', 'idempotency-key']) {
    if (ctx.req.headers[h]) headers[h] = ctx.req.headers[h];
  }
  if (form) headers['content-type'] = 'application/x-www-form-urlencoded';
  const response = await fetch(`${UPSTREAM}${path}${query}`, {
    method,
    headers,
    body: form || undefined
  });
  const text = await response.text();
  return ctx.send(response.status, JSON.parse(text || '{}'), {
    'request-id': response.headers.get('request-id') ?? ''
  });
}

// ---------------------------------------------------------------- routes

function accountFor(ctx) {
  ctx.state.accounts ??= {};
  ctx.state.objects ??= new Map();
  return ctx.state.accounts[ctx.instanceId];
}

function enable(ctx, account) {
  if (account.chargesEnabled) return;
  Object.assign(account, {
    chargesEnabled: true,
    payoutsEnabled: true,
    detailsSubmitted: true,
    requirementsDue: []
  });
  return ctx.emit('account.updated', {
    id: account.id,
    object: 'account',
    charges_enabled: true,
    payouts_enabled: true,
    details_submitted: true,
    requirements: { currently_due: [] }
  });
}

async function onboardingLink(ctx) {
  const { returnUrl, refreshUrl } = ctx.body ?? {};
  for (const [name, value] of [['returnUrl', returnUrl], ['refreshUrl', refreshUrl]]) {
    if (typeof value !== 'string' || !/^https?:\/\//.test(value)) {
      return ctx.send(400, `${name} must be an absolute http(s) URL`);
    }
  }
  let account = accountFor(ctx);
  if (!account) {
    ctx.state.accountCount = (ctx.state.accountCount ?? 0) + 1;
    account = ctx.state.accounts[ctx.instanceId] = {
      id: `acct_mock_${String(ctx.state.accountCount).padStart(4, '0')}`,
      chargesEnabled: false,
      payoutsEnabled: false,
      detailsSubmitted: false,
      requirementsDue: ['business_profile.url', 'external_account', 'tos_acceptance.date']
    };
  }
  const url = `https://connect.stripe.com/setup/e/${account.id}/mock_${randomBytes(6).toString('hex')}`;
  ctx.send(200, { url });
  if (AUTO_ENABLE) await enable(ctx, account);
}

function status(ctx) {
  const account = accountFor(ctx);
  return ctx.send(200, {
    chargesEnabled: Boolean(account?.chargesEnabled),
    payoutsEnabled: Boolean(account?.payoutsEnabled),
    requirementsDue: account ? account.requirementsDue : ['onboarding']
  });
}

async function stripeProxy(ctx) {
  const method = ctx.req.method;
  const path = ctx.path.replace(/^\/instance-gateway\/stripe/, '');
  const query = new URL(ctx.req.url ?? '/', 'http://mock').search;
  const account = accountFor(ctx);
  if (!account) {
    return stripeError(ctx, 409, 'invalid_request_error', 'Payments are not set up for this instance: create an onboarding link first', 'payments_not_onboarded');
  }
  if (!isAllowedStripeCall(method, path)) {
    return stripeError(ctx, 403, 'invalid_request_error', `${method} ${path} is not available to managed instances`, 'forklaunch_gateway_forbidden');
  }
  const params = new URLSearchParams(method === 'GET' || method === 'DELETE' ? query.slice(1) : ctx.raw);
  const violation = protectedDataViolation(params);
  if (violation) {
    return stripeError(ctx, 400, 'invalid_request_error', violation, 'forklaunch_protected_data');
  }
  const minute = Math.floor(Date.now() / 60_000);
  ctx.state.rate ??= {};
  const rateKey = `${ctx.instanceId}:${minute}`;
  ctx.state.rate[rateKey] = (ctx.state.rate[rateKey] ?? 0) + 1;
  if (ctx.state.rate[rateKey] > RPM) {
    return stripeError(ctx, 429, 'rate_limit_error', `Rate limited: ${RPM} Stripe calls per minute`, 'rate_limit', { 'retry-after': '60' });
  }
  applyApplicationFee(method, path, params, FEE);
  const form = method === 'GET' || method === 'DELETE' ? '' : params.toString();
  ctx.record.forwarded = {
    method,
    path: `${path}${query}`,
    stripeAccount: account.id,
    sentStripeAccount: ctx.req.headers['stripe-account'] ?? null,
    body: form || undefined
  };
  if (UPSTREAM) return answerUpstream(ctx, method, path, query, form, account.id);
  return answerInMemory(ctx, method, path, parseForm(form || query.slice(1)), account.id);
}

async function complete(ctx) {
  ctx.state.objects ??= new Map();
  const session = ctx.state.objects.get(ctx.params.id);
  if (!session || session.object !== 'checkout.session') return ctx.send(404, `No such checkout session: ${ctx.params.id}`);
  if (session.status !== 'open') return ctx.send(409, `Checkout session is ${session.status}`);
  Object.assign(session, { status: 'complete', payment_status: 'paid' });
  if (session.mode === 'subscription') {
    session.subscription = created(ctx.state, 'subscriptions', { customer: session.customer, metadata: session.metadata }, session.__account).id;
  } else {
    const intent = created(ctx.state, 'payment_intents', { amount: session.amount_total, currency: session.currency, customer: session.customer }, session.__account);
    intent.status = 'succeeded';
    session.payment_intent = intent.id;
  }
  const delivery = await ctx.emit('checkout.session.completed', visible(session));
  return ctx.send(200, { session: visible(session), delivery });
}

async function enableByHand(ctx) {
  const accounts = Object.values(ctx.state.accounts ?? {});
  for (const account of accounts) await enable(ctx, account);
  return ctx.send(200, { enabled: accounts.map((a) => a.id) });
}

const PROXY = /^\/stripe\/v1\/.+$/;

export default {
  feature: 'payments',
  routes: [
    { method: 'POST', path: '/payments/onboarding-link', handler: onboardingLink },
    { method: 'GET', path: '/payments/status', handler: status },
    ...['GET', 'POST', 'DELETE'].map((method) => ({ method, path: PROXY, handler: stripeProxy }))
  ],
  vendorRoutes: [
    { method: 'POST', path: /^\/__mock\/payments\/complete\/(?<id>[A-Za-z0-9_]+)$/, handler: complete },
    { method: 'POST', path: '/__mock/payments/enable', handler: enableByHand }
  ]
};
