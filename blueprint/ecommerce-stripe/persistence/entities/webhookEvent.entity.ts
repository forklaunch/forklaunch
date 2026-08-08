import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

/**
 * The idempotency ledger for provider webhooks (ECOM-10's other half —
 * confirmPayment/failPayment finally have callers, this is what keeps
 * those callers safe against at-least-once delivery). One row per
 * (provider, provider event id) pair.
 *
 * `providerEventId` is namespaced with the provider (`stripe:evt_...` /
 * `paypal:<id>`) rather than relying on a composite unique constraint —
 * Stripe and PayPal event id formats don't collide in practice, but
 * namespacing makes that an explicit guarantee instead of an assumption.
 *
 * `processed` mirrors the exact pattern OrderEventRecord already uses for
 * the same reason: a webhook that we saw and started handling but that
 * then threw partway through must NOT look "seen" to a legitimate retry —
 * only a fully successful handling run flips this to true. Business logic
 * downstream (confirmPayment/failPayment/transitionOrder) is independently
 * idempotent too, so a reprocessed-but-not-yet-marked-processed event is
 * always safe to run again.
 */
export const WebhookEvent = defineComplianceEntity({
  name: 'WebhookEvent',
  properties: {
    ...sqlBaseProperties,
    provider: fp.string().compliance('none'),
    providerEventId: fp.string().unique().compliance('none'),
    eventType: fp.string().compliance('none'),
    processed: fp.boolean().compliance('none')
  }
});
