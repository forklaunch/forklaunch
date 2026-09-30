import Stripe from 'stripe';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import {
  createPaymentsClient,
  createStripeClient,
  PaymentsRequestError
} from '../../src/http/paymentsClient';
import { type Harness, startHarness } from './harness';

/**
 * Payments end to end: the real Stripe SDK, built by createStripeClient,
 * talking to the gateway mock over HTTP with signed form-encoded requests;
 * onboarding through createPaymentsClient; and Stripe's events delivered to
 * the app as signed platform events it verifies.
 */
let h: Harness;
beforeAll(async () => {
  h = await startHarness({ MOCK_PAYMENTS_FEE_PERCENT: '5' });
});
afterAll(async () => {
  await h?.stop();
});

async function until<T>(read: () => T | undefined, ms = 3000): Promise<T> {
  const end = Date.now() + ms;
  for (;;) {
    const value = read();
    if (value !== undefined) return value;
    if (Date.now() > end) throw new Error('timed out');
    await new Promise((r) => setTimeout(r, 25));
  }
}

const stripe = () => createStripeClient({ ...h.clientOptions, Stripe });

describe('payments (e2e)', () => {
  it('refuses Stripe calls before onboarding, then onboards', async () => {
    const payments = createPaymentsClient(h.clientOptions);
    expect(await payments.status()).toEqual({
      chargesEnabled: false,
      payoutsEnabled: false,
      requirementsDue: ['onboarding']
    });
    await expect(
      stripe().customers.create({ email: 'a@example.com' })
    ).rejects.toMatchObject({ statusCode: 409, code: 'payments_not_onboarded' });

    const { url } = await payments.onboardingLink({
      returnUrl: 'https://clinic.example.com/settings/payments?done=1',
      refreshUrl: 'https://clinic.example.com/settings/payments'
    });
    expect(url).toMatch(/^https:\/\/connect\.stripe\.com\/setup\/e\/acct_mock_/);
    expect(await payments.status()).toEqual({
      chargesEnabled: true,
      payoutsEnabled: true,
      requirementsDue: []
    });
    const updated = await until(() =>
      h.received('payments').find((e) => e.type === 'account.updated')
    );
    expect(updated.data).toMatchObject({ charges_enabled: true });
    expect(() =>
      payments.onboardingLink({ returnUrl: 'nope', refreshUrl: 'nope' })
    ).toThrow('returnUrl');
  });

  it('creates a checkout session with the real Stripe SDK, pinned to the instance account', async () => {
    const client = stripe();
    const product = await client.products.create({ name: 'Initial consult' });
    const price = await client.prices.create({
      product: product.id,
      unit_amount: 12500,
      currency: 'usd'
    });
    expect(price).toMatchObject({ object: 'price', unit_amount: 12500, type: 'one_time' });
    const customer = await client.customers.create({ email: 'pat@example.com' });
    expect(customer.id).toMatch(/^cus_/);

    const session = await client.checkout.sessions.create(
      {
        mode: 'payment',
        customer: customer.id,
        line_items: [{ price: price.id, quantity: 2 }],
        success_url: 'https://clinic.example.com/paid',
        metadata: { appointmentId: 'apt_123' }
      },
      // The app cannot redirect the charge to another account.
      { stripeAccount: 'acct_someone_else' }
    );
    expect(session).toMatchObject({
      object: 'checkout.session',
      status: 'open',
      amount_total: 25000,
      metadata: { appointmentId: 'apt_123' }
    });
    expect(session.url).toMatch(/^https:\/\/checkout\.stripe\.com\//);
    expect((await client.checkout.sessions.retrieve(session.id)).id).toBe(session.id);

    const calls = await h.requests('payments');
    const create = calls.find(
      (c) => c.method === 'POST' && c.path === '/stripe/v1/checkout/sessions'
    ) as unknown as {
      body: string;
      forwarded: { stripeAccount: string; sentStripeAccount: string; body: string };
    };
    expect(create.forwarded.sentStripeAccount).toBe('acct_someone_else');
    expect(create.forwarded.stripeAccount).toMatch(/^acct_mock_/);
    // Signed and received as form data; nothing carries a Stripe key.
    expect(create.body).toContain('line_items[0][price]=price_');
    // The product's 5% fee applies only where Stripe takes a percent
    // (subscriptions); a payment-mode session with no flat fee is unchanged.
    expect(create.forwarded.body).not.toContain('application_fee');

    const intent = await client.paymentIntents.create({ amount: 10000, currency: 'usd' });
    expect(intent.status).toBe('requires_payment_method');
    const piCall = (await h.requests('payments')).find(
      (c) => c.path === '/stripe/v1/payment_intents'
    ) as unknown as { forwarded: { body: string } };
    expect(piCall.forwarded.body).toContain('application_fee_amount=500');

    const prices = await client.prices.list({ product: product.id, active: true });
    expect(prices.data.map((p) => p.id)).toEqual([price.id]);
  });

  it('delivers checkout.session.completed, which the app verifies', async () => {
    const client = stripe();
    const price = await client.prices.create({
      currency: 'usd',
      unit_amount: 4000,
      product_data: { name: 'Follow-up' }
    });
    const session = await client.checkout.sessions.create({
      mode: 'payment',
      line_items: [{ price: price.id, quantity: 1 }],
      success_url: 'https://clinic.example.com/paid'
    });
    const response = await fetch(
      `${h.gatewayUrl}/__mock/payments/complete/${session.id}`,
      { method: 'POST' }
    );
    expect(response.status).toBe(200);
    const event = await until(() =>
      h.received('payments').find((e) => e.type === 'checkout.session.completed')
    );
    expect(event.data).toMatchObject({
      id: session.id,
      status: 'complete',
      payment_status: 'paid'
    });
    expect(h.rejected()).toEqual([]);
    const paid = await client.checkout.sessions.retrieve(session.id);
    expect(paid.payment_intent).toMatch(/^pi_/);
  });

  it('forwards only the allowlist, and refuses protected-data metadata', async () => {
    const client = stripe();
    await expect(
      client.payouts.create({ amount: 100, currency: 'usd' })
    ).rejects.toBeInstanceOf(Stripe.errors.StripePermissionError);
    await expect(
      client.accounts.update('acct_mock_0001', { email: 'x@example.com' })
    ).rejects.toMatchObject({ statusCode: 403, code: 'forklaunch_gateway_forbidden' });
    await expect(
      client.transfers.create({ amount: 1, currency: 'usd', destination: 'acct_x' })
    ).rejects.toMatchObject({ statusCode: 403 });

    await expect(
      client.customers.create({ metadata: { diagnosis: 'J45.909' } })
    ).rejects.toBeInstanceOf(Stripe.errors.StripeInvalidRequestError);
    await expect(
      client.customers.create({ metadata: { patientDob: '1980-01-01' } })
    ).rejects.toMatchObject({ statusCode: 400, code: 'forklaunch_protected_data' });
    await expect(
      client.customers.create({ description: 'SSN 123-45-6789' })
    ).rejects.toMatchObject({ statusCode: 400 });
    // An opaque reference is fine.
    const ok = await client.customers.create({ metadata: { patientRef: 'p_8812' } });
    expect(ok.metadata).toEqual({ patientRef: 'p_8812' });
  });

  it('refuses a wrong instance key', async () => {
    const wrong = createStripeClient({ ...h.clientOptions, hmacKey: 'nope', Stripe });
    await expect(wrong.customers.create({})).rejects.toBeInstanceOf(
      Stripe.errors.StripeAuthenticationError
    );
    const payments = createPaymentsClient({ ...h.clientOptions, hmacKey: 'nope' });
    await expect(payments.status()).rejects.toBeInstanceOf(PaymentsRequestError);
    await expect(payments.status()).rejects.toMatchObject({ status: 401 });
  });

  it('builds a plain Stripe client outside managed mode, and refuses without a key', () => {
    const saved = {
      url: process.env.PLATFORM_GATEWAY_URL,
      id: process.env.INSTANCE_ID,
      key: process.env.INSTANCE_HMAC_KEY
    };
    delete process.env.PLATFORM_GATEWAY_URL;
    delete process.env.INSTANCE_ID;
    delete process.env.INSTANCE_HMAC_KEY;
    try {
      const plain = createStripeClient({ apiKey: 'sk_test_local', Stripe });
      expect(plain).toBeInstanceOf(Stripe);
      expect(() => createStripeClient({ Stripe })).toThrow('not a managed instance');
      // Without an explicit class the optional peer is loaded lazily.
      expect(createStripeClient({ apiKey: 'sk_test_local' })).toBeTruthy();
    } finally {
      if (saved.url) process.env.PLATFORM_GATEWAY_URL = saved.url;
      if (saved.id) process.env.INSTANCE_ID = saved.id;
      if (saved.key) process.env.INSTANCE_HMAC_KEY = saved.key;
    }
  });
});

/**
 * The same proxy in front of the official stripe/stripe-mock (docker run -p
 * 12111:12111 stripe/stripe-mock), which validates each request against
 * Stripe's OpenAPI spec. Skipped when it is not running.
 */
const STRIPE_MOCK = process.env.STRIPE_MOCK_URL ?? 'http://localhost:12111';
const stripeMockUp = await fetch(`${STRIPE_MOCK}/v1/customers`, {
  headers: { authorization: 'Bearer sk_test_123' }
})
  .then((r) => r.ok)
  .catch(() => false);

describe.skipIf(!stripeMockUp)('payments against stripe/stripe-mock (e2e)', () => {
  let up: Harness;
  beforeAll(async () => {
    up = await startHarness({ MOCK_STRIPE_UPSTREAM: STRIPE_MOCK });
  });
  afterAll(async () => {
    await up?.stop();
  });

  it('forwards allowlisted calls upstream with the account pinned', async () => {
    await createPaymentsClient(up.clientOptions).onboardingLink({
      returnUrl: 'https://clinic.example.com/done',
      refreshUrl: 'https://clinic.example.com/retry'
    });
    const client = createStripeClient({ ...up.clientOptions, Stripe });
    const session = await client.checkout.sessions.create({
      mode: 'payment',
      line_items: [{ price: 'price_123', quantity: 1 }],
      success_url: 'https://clinic.example.com/paid'
    });
    expect(session.object).toBe('checkout.session');
    const portal = await client.billingPortal.sessions.create({
      customer: 'cus_123',
      return_url: 'https://clinic.example.com'
    });
    expect(portal.object).toBe('billing_portal.session');
    const sub = await client.subscriptions.create({
      customer: 'cus_123',
      items: [{ price: 'price_123' }]
    });
    expect(sub.object).toBe('subscription');
    const calls = (await up.requests('payments')) as unknown as {
      path: string;
      forwarded?: { stripeAccount: string };
    }[];
    expect(
      calls.filter((c) => c.path.startsWith('/stripe/')).every((c) =>
        c.forwarded?.stripeAccount?.startsWith('acct_mock_')
      )
    ).toBe(true);
    await expect(client.payouts.list()).rejects.toMatchObject({ statusCode: 403 });
  });
});
