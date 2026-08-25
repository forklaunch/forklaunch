import { randomUUID } from 'node:crypto';
import { EntityManager } from '@mikro-orm/core';
import {
  cleanupTestDatabase,
  setupTestDatabase,
  TestSetupResult
} from './test-utils';

/**
 * Regression test for checkout's payment-side idempotency.
 *
 * The bug this pins: checkout reused a still-PENDING order on a retry but
 * then called createPayment unconditionally, so every retry opened another
 * live PaymentIntent against the same order. Both intents stayed
 * confirmable. Order state and inventory were never wrong — the state
 * machine rejects the second PAID transition and worker.ts decrements
 * stock once — so the only symptom was the customer being charged twice,
 * which nothing in the system surfaced. Observed live: one $53.50 order
 * with two succeeded PaymentIntents against it, $107.00 collected.
 *
 * The lookup is what checkout consults before creating a payment, so its
 * filter is the whole fix. Run against a real database rather than a fake
 * EntityManager precisely because the filter is the thing under test:
 * `providerRef: { $ne: null }` against a hand-rolled findOne would assert
 * the query shape this test was written from, not what Postgres does with
 * it.
 *
 * Follows webhookEvent.test.ts: the service is constructed directly on the
 * harness's connected ORM (the same seam the DI factory uses) rather than
 * resolved through the container.
 */
describe('PaymentOrderLookupService (checkout payment idempotency)', () => {
  let setup: TestSetupResult;

  // 180s: matches webhookEvent/purchaseLoop — the Postgres+Redis
  // testcontainers can take well over a minute to pull and start.
  beforeAll(async () => {
    setup = await setupTestDatabase();
  }, 180000);

  afterAll(async () => {
    await cleanupTestDatabase();
  }, 30000);

  async function makeService() {
    const { PaymentOrderLookupService } = await import(
      '../domain/services/paymentOrderLookup.service'
    );
    const em = setup.orm!.em.fork() as unknown as EntityManager;
    return { service: new PaymentOrderLookupService(em), em };
  }

  /** Persists a payment row for `orderId` and returns its generated id. */
  async function givenPayment(
    em: EntityManager,
    orderId: string,
    status: string,
    providerRef: string | null
  ): Promise<string> {
    const { Payment } = await import('../persistence/entities');
    const payment = em.create(Payment, {
      orderId,
      amountCents: 5350,
      currency: 'usd',
      status,
      providerRef
    } as never);
    // create()+persist(), matching webhookEvent.service.ts — em.insert() is
    // a raw fast-path that skips sqlBaseProperties' onCreate hooks, so the
    // row would reach Postgres with a null id.
    em.persist(payment);
    await em.flush();
    return (payment as unknown as { id: string }).id;
  }

  it('finds the pending payment a retry has to reuse', async () => {
    const { service, em } = await makeService();
    const orderId = randomUUID();
    const paymentId = await givenPayment(
      em,
      orderId,
      'pending',
      `pi_${randomUUID()}`
    );

    const found = await service.findPendingPaymentByOrderId(orderId);

    expect(found?.id).toBe(paymentId);
  });

  it('returns the providerRef so the caller can route to the right provider', async () => {
    const { service, em } = await makeService();
    const orderId = randomUUID();
    const providerRef = `pi_${randomUUID()}`;
    await givenPayment(em, orderId, 'pending', providerRef);

    const found = await service.findPendingPaymentByOrderId(orderId);

    expect(found?.providerRef).toBe(providerRef);
  });

  it('finds nothing for an order that has never been paid for', async () => {
    const { service } = await makeService();

    const found = await service.findPendingPaymentByOrderId(randomUUID());

    expect(found).toBeNull();
  });

  it('ignores a succeeded payment, so a paid order is never handed a live credential', async () => {
    const { service, em } = await makeService();
    const orderId = randomUUID();
    await givenPayment(em, orderId, 'succeeded', `pi_${randomUUID()}`);

    const found = await service.findPendingPaymentByOrderId(orderId);

    expect(found).toBeNull();
  });

  it('ignores a failed payment, which the provider will not confirm again', async () => {
    const { service, em } = await makeService();
    const orderId = randomUUID();
    await givenPayment(em, orderId, 'failed', `pi_${randomUUID()}`);

    const found = await service.findPendingPaymentByOrderId(orderId);

    expect(found).toBeNull();
  });

  it('ignores a pending payment that never reached the provider', async () => {
    // A null providerRef has no intent behind it, so there is nothing to
    // resume and no double-charge risk. Returning it would be worse than
    // useless: findOne would keep handing back the same dead row and
    // shadow the real pending payment on every retry.
    const { service, em } = await makeService();
    const orderId = randomUUID();
    await givenPayment(em, orderId, 'pending', null);

    const found = await service.findPendingPaymentByOrderId(orderId);

    expect(found).toBeNull();
  });

  it('picks the live payment over a dead one on the same order', async () => {
    const { service, em } = await makeService();
    const orderId = randomUUID();
    await givenPayment(em, orderId, 'pending', null);
    const liveId = await givenPayment(
      em,
      orderId,
      'pending',
      `pi_${randomUUID()}`
    );

    const found = await service.findPendingPaymentByOrderId(orderId);

    expect(found?.id).toBe(liveId);
  });

  it('scopes to its own order', async () => {
    const { service, em } = await makeService();
    const orderId = randomUUID();
    await givenPayment(em, randomUUID(), 'pending', `pi_${randomUUID()}`);

    const found = await service.findPendingPaymentByOrderId(orderId);

    expect(found).toBeNull();
  });
});
