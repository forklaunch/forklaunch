/**
 * Points every Stripe client the module builds (the DI container's included)
 * at the running Stripe stub: the mocked constructor adds the stub's host,
 * port and protocol to the client options. With STRIPE_LIVE_TESTS=1 the real
 * SDK is left alone and calls reach the Stripe test API.
 *
 * Kept apart from test-utils.ts: vitest rewrites the imports of a module that
 * calls vi.mock, which would break test-utils' re-exports.
 */
vi.mock('stripe', async (importOriginal) => {
  const actual = await importOriginal<typeof import('stripe')>();
  if (process.env.STRIPE_LIVE_TESTS === '1') return actual;
  const { runningStripeStub } = await import('./stripe-stub');
  const Stripe = actual.default;
  function StubbedStripe(
    key: string,
    config?: ConstructorParameters<typeof Stripe>[1]
  ) {
    const stub = runningStripeStub();
    if (!stub) {
      throw new Error('The Stripe stub is not running; call setupTestDatabase');
    }
    return new Stripe(key, { ...config, ...stub.clientOptions });
  }
  Object.setPrototypeOf(StubbedStripe, Stripe);
  StubbedStripe.prototype = Stripe.prototype;
  return { ...actual, default: StubbedStripe, Stripe: StubbedStripe };
});
