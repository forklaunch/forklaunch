import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const StripeWebhookEvent = defineEntity({
  name: 'StripeWebhookEvent',
  properties: {
    ...sqlBaseProperties,
    stripeId: p.string(),
    idempotencyKey: p.string(),
    eventType: p.string(),
    eventData: p.json<unknown>()
  }
});

export type IStripeWebhookEvent = InferEntity<typeof StripeWebhookEvent>;
