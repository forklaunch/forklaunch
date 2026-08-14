import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { Metrics } from '@forklaunch/blueprint-monitoring';
import { EntityManager, UniqueConstraintViolationException } from '@mikro-orm/core';
import { WebhookEvent } from '../../persistence/entities/webhookEvent.entity';

export type WebhookEventOutcome =
  /** First time this event has been seen — go ahead and process it. */
  | 'new'
  /** Seen before, fully processed — a redelivery. Do nothing. */
  | 'already-processed'
  /** Seen before, but processing never completed last time (crash, thrown
   *  error, etc). Safe — and necessary — to process again. */
  | 'retry';

/**
 * The idempotency gate for provider webhooks (ECOM-10) — Stripe and PayPal
 * both deliver at-least-once, so the same event id can arrive more than
 * once. `beginProcessing` is the insert-first half: it claims an event
 * (inserting a WebhookEvent row) *before* any business logic runs, so two
 * concurrent deliveries of the same event race on a DB unique constraint,
 * not on application logic. `markProcessed` is called only after the
 * handler's business logic has fully succeeded.
 */
export class WebhookEventService {
  constructor(
    private readonly em: EntityManager,
    private readonly openTelemetryCollector: OpenTelemetryCollector<Metrics>
  ) {}

  async beginProcessing(params: {
    provider: string;
    providerEventId: string;
    eventType: string;
  }): Promise<WebhookEventOutcome> {
    try {
      await this.em.transactional(async (innerEm) => {
        // create()+persist, NOT insert(): em.insert() is a raw fast-path that
        // bypasses the unit of work, so sqlBaseProperties' onCreate hooks
        // (id/createdAt/updatedAt) never run and the INSERT sends null id ->
        // NotNullConstraintViolation. The unique-constraint race is still
        // enforced at flush, so idempotency is unchanged.
        const event = innerEm.create(WebhookEvent, {
          provider: params.provider,
          providerEventId: params.providerEventId,
          eventType: params.eventType,
          processed: false
        });
        innerEm.persist(event);
        await innerEm.flush();
      });
      return 'new';
    } catch (error) {
      if (!(error instanceof UniqueConstraintViolationException)) {
        throw error;
      }
      // Someone else's insert (this request or an earlier delivery) won
      // the race — find out whether it actually finished. Matched on the
      // full (provider, providerEventId) composite, same as the unique
      // constraint itself — providerEventId alone is only unique per
      // provider (see WebhookEvent's entity comment).
      const existing = await this.em.findOneOrFail(WebhookEvent, {
        provider: params.provider,
        providerEventId: params.providerEventId
      });
      if (existing.processed) {
        this.openTelemetryCollector.info(
          'Webhook event already processed — skipping',
          params
        );
        return 'already-processed';
      }
      this.openTelemetryCollector.warn(
        'Webhook event was seen before but never finished processing — retrying',
        params
      );
      return 'retry';
    }
  }

  async markProcessed(params: {
    provider: string;
    providerEventId: string;
  }): Promise<void> {
    const event = await this.em.findOneOrFail(WebhookEvent, {
      provider: params.provider,
      providerEventId: params.providerEventId
    });
    this.em.assign(event, { processed: true });
    await this.em.transactional(async (innerEm) => {
      await innerEm.persist(event);
    });
  }
}
