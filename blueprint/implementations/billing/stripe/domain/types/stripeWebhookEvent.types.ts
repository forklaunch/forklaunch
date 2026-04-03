import { defineEntity, p, type InferEntity } from '@mikro-orm/core';

export const StripeWebhookEventEntity = defineEntity({
  name: 'StripeWebhookEvent',
  properties: {
    id: p.uuid().primary(),
    createdAt: p.datetime(),
    updatedAt: p.datetime(),
    stripeId: p.string(),
    idempotencyKey: p.string().nullable(),
    eventType: p.string(),
    eventData: p.json<unknown>()
  }
});

export type StripeWebhookEvent = InferEntity<typeof StripeWebhookEventEntity>;
