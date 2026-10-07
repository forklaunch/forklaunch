import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

/**
 * The idempotency ledger for provider webhooks (the other half —
 * confirmPayment/failPayment finally have callers, this is what keeps
 * those callers safe against at-least-once delivery). One row per
 * (provider, provider event id) pair.
 *
 * `providerEventId` is the provider's own, unprefixed event id (Stripe's
 * `evt_...`, PayPal's own id) — both call sites in webhook.controller.ts
 * pass it through as-is. Uniqueness is enforced as a genuine composite
 * constraint on `(provider, providerEventId)` below, rather than assuming
 * Stripe and PayPal event id formats never collide.
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
    providerEventId: fp.string().compliance('none'),
    eventType: fp.string().compliance('none'),
    processed: fp.boolean().compliance('none')
  },
  uniques: [{ properties: ['provider', 'providerEventId'] }]
});
