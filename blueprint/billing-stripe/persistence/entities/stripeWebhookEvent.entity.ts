import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineEntity, p, type InferEntity } from '@mikro-orm/core';

export const stripeWebhookEvent = defineEntity({
  name: 'StripeWebhookEvent',
  properties: {
    ...sqlBaseProperties,
    stripeId: p.string(),
    idempotencyKey: p.string().nullable(),
    eventType: p.string(),
    eventData: p.json<unknown>()
  }
});

export type StripeWebhookEvent = InferEntity<typeof stripeWebhookEvent>;
