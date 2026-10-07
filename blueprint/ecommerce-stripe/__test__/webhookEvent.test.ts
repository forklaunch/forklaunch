import { randomUUID } from 'node:crypto';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { EntityManager } from '@mikro-orm/core';
import {
  cleanupTestDatabase,
  setupTestDatabase,
  TestSetupResult
} from './test-utils';

/**
 * Regression test for the webhook idempotency gate.
 *
 * The bug this pins: beginProcessing originally claimed events with
 * `em.insert(WebhookEvent, {...})`. em.insert() is a raw fast-path that
 * bypasses the unit of work, so sqlBaseProperties' onCreate hooks
 * (id/createdAt/updatedAt) never ran and the INSERT reached Postgres with a
 * null id -> NotNullConstraintViolation -> every provider webhook returned
 * 500 and no order could ever transition to PAID via a real payment. The
 * purchase-loop E2E never caught it because it drives order transitions
 * directly and never walks the webhook path.
 *
 * The service is constructed directly on the harness's connected ORM (the
 * same seam the DI factory uses) rather than resolved through the container:
 * resolving the EntityManager token inside the test harness hits the known
 * pre-existing Orm.em harness quirk that live servers don't have, and this
 * test exists to pin insert-vs-create semantics, not container wiring.
 */
describe('WebhookEventService idempotency gate', () => {
  let setup: TestSetupResult;

  // 180s: the Postgres+Redis testcontainers can take well over a minute to
  // pull/start on a loaded machine; purchaseLoop pays the same cost.
  beforeAll(async () => {
    setup = await setupTestDatabase();
  }, 180000);

  afterAll(async () => {
    await cleanupTestDatabase();
  }, 30000);

  async function makeService() {
    const { WebhookEventService } = await import(
      '../domain/services/webhookEvent.service'
    );
    const em = setup.orm!.em.fork() as unknown as EntityManager;
    return {
      service: new WebhookEventService(
        em,
        new OpenTelemetryCollector('ecommerce-webhook-test', 'info')
      ),
      em
    };
  }

  it('claims a new event with a generated id (regression: em.insert sent null id)', async () => {
    const { WebhookEvent } = await import('../persistence/entities');
    const { service, em } = await makeService();
    const providerEventId = `evt_${randomUUID()}`;

    // With the em.insert() bug this line threw NotNullConstraintViolation
    // (null id) instead of returning 'new'.
    const outcome = await service.beginProcessing({
      provider: 'stripe',
      providerEventId,
      eventType: 'payment_intent.succeeded'
    });
    expect(outcome).toBe('new');

    const row = await em.fork().findOneOrFail(WebhookEvent, {
      provider: 'stripe',
      providerEventId
    });
    // The heart of the regression: the row must carry a generated id.
    expect(row.id).toBeTruthy();
    expect(row.processed).toBe(false);
  });

  it('keeps redelivery semantics: retry before markProcessed, already-processed after', async () => {
    const { service } = await makeService();
    const providerEventId = `evt_${randomUUID()}`;
    const claim = () =>
      service.beginProcessing({
        provider: 'stripe',
        providerEventId,
        eventType: 'charge.succeeded'
      });

    expect(await claim()).toBe('new');
    // Same event again before processing finished (crash/redelivery window):
    // safe — and necessary — to process again.
    expect(await claim()).toBe('retry');

    await service.markProcessed({ provider: 'stripe', providerEventId });
    // Post-completion redelivery is a no-op.
    expect(await claim()).toBe('already-processed');
  });

  it('scopes idempotency per provider: same event id from another provider is new', async () => {
    const { service } = await makeService();
    const providerEventId = `evt_${randomUUID()}`;

    expect(
      await service.beginProcessing({
        provider: 'stripe',
        providerEventId,
        eventType: 'payment_intent.succeeded'
      })
    ).toBe('new');
    // providerEventId alone is only unique per provider (composite
    // constraint) — PayPal reusing Stripe's id string must not collide.
    expect(
      await service.beginProcessing({
        provider: 'paypal',
        providerEventId,
        eventType: 'PAYMENT.CAPTURE.COMPLETED'
      })
    ).toBe('new');
  });
});
