// Official SDK serialization oracle only: all responses are owned loopback fixtures.
// This proves wire compatibility, not live Stripe acceptance or payment success.
import assert from 'node:assert/strict';
import http from 'node:http';
import { createRequire } from 'node:module';

const Stripe = createRequire(import.meta.url)(process.env.STRIPE_SDK_PATH ?? 'stripe');
assert.equal(Stripe.PACKAGE_VERSION, '22.6.2');
const version = '2026-08-26.dahlia';
const key = 'opaque-subscription-fixture-key';
const fields = {
  customer: 'cus_owned',
  items: [{ plan: 'custom-plan', quantity: 2 }],
  collection_method: 'charge_automatically',
  payment_behavior: 'default_incomplete',
  description: 'Pro plan / two seats ✓'
};
const plan = {
  id: 'custom-plan', object: 'plan', livemode: false, active: true,
  product: 'custom-product', amount: 1200, currency: 'usd', interval: 'month',
  interval_count: 1, usage_type: 'licensed', billing_scheme: 'per_unit'
};
const price = { id: 'custom-plan', object: 'price', livemode: false, active: true, type: 'recurring', billing_scheme: 'per_unit', unit_amount: 1200, currency: 'usd', product: 'custom-product', recurring: { interval: 'month', interval_count: 1, usage_type: 'licensed' } };
const subscription = status => ({
  id: 'sub_owned', object: 'subscription', customer: 'cus_owned', livemode: false,
  collection_method: 'charge_automatically', start_date: 1791936000, status, description: fields.description,
  items: {
    object: 'list', has_more: false,
    data: [{ id: 'si_owned', object: 'subscription_item', subscription: 'sub_owned', quantity: 2, plan, price }]
  },
  latest_invoice: 'in_owned'
});
const expected = {
  customer: fields.customer,
  'items[0][plan]': 'custom-plan',
  'items[0][quantity]': '2',
  collection_method: fields.collection_method,
  payment_behavior: fields.payment_behavior,
  description: fields.description
};
const calls = [];
let status = 'incomplete';
let failPost = false;
const server = http.createServer(async (req, res) => {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  assert.equal(req.headers.authorization, `Bearer ${key}`);
  assert.equal(req.headers['stripe-version'], version);
  assert.equal(req.headers['stripe-account'], undefined);
  const body = Buffer.concat(chunks).toString();
  calls.push([req.method, req.url]);
  let result;
  if (req.method === 'POST') {
    assert.equal(req.url, '/v1/subscriptions');
    assert.equal(req.headers['idempotency-key'], 'fl-subscription-00000000-0000-4000-8000-000000000001');
    assert.deepEqual(Object.fromEntries(new URLSearchParams(body)), expected);
    if (failPost) {
      res.writeHead(500, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ error: { type: 'api_error', message: 'Owned ambiguous fixture response' } }));
      return;
    }
    result = subscription(status);
  } else {
    assert.equal(req.method, 'GET');
    assert.equal(body, '');
    assert.equal(req.headers['idempotency-key'], undefined);
    switch (req.url) {
      case '/v1/account': result = { id: 'acct_owned', object: 'account' }; break;
      case '/v1/balance': result = { object: 'balance', livemode: false }; break;
      case '/v1/customers/cus_owned': result = { id: 'cus_owned', object: 'customer', livemode: false }; break;
      case '/v1/plans/custom-plan?expand[0]=product': result = { ...plan, product: { id: 'custom-product', object: 'product', livemode: false } }; break;
      case '/v1/subscriptions/sub_owned': result = subscription(status); break;
      default: throw new Error(`Unexpected loopback path ${req.url}`);
    }
  }
  res.writeHead(200, { 'content-type': 'application/json' });
  res.end(JSON.stringify(result));
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
try {
  const client = new Stripe(key, {
    host: '127.0.0.1', port: server.address().port, protocol: 'http',
    apiVersion: version, maxNetworkRetries: 0
  });
  for (status of ['incomplete', 'active']) {
    calls.length = 0;
    await client.accounts.retrieve();
    await client.balance.retrieve();
    await client.customers.retrieve('cus_owned');
    await client.plans.retrieve('custom-plan', { expand: ['product'] });
    const created = await client.subscriptions.create(fields, {
      idempotencyKey: 'fl-subscription-00000000-0000-4000-8000-000000000001'
    });
    const readback = await client.subscriptions.retrieve(created.id);
    assert.equal(readback.status, status);
    assert.equal(readback.items.data[0].plan.id, 'custom-plan');
    assert.equal(readback.start_date, 1791936000);
    assert.deepEqual(calls, [
      ['GET', '/v1/account'], ['GET', '/v1/balance'], ['GET', '/v1/customers/cus_owned'],
      ['GET', '/v1/plans/custom-plan?expand[0]=product'], ['POST', '/v1/subscriptions'],
      ['GET', '/v1/subscriptions/sub_owned']
    ]);
  }
  calls.length = 0;
  failPost = true;
  await assert.rejects(client.subscriptions.create(fields, {
    idempotencyKey: 'fl-subscription-00000000-0000-4000-8000-000000000001'
  }));
  assert.deepEqual(calls, [['POST', '/v1/subscriptions']], 'SDK retries must be explicitly disabled');
  console.log('PASS Stripe22.6.2 Subscription finite form, custom Plan ID, exact GET readback, pinned account/version/idempotency, active/incomplete fixtures, and one POST on 500; loopback wire only');
} finally {
  await new Promise(resolve => server.close(resolve));
}
