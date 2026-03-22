import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineEntity, p } from '@mikro-orm/core';

export const StripeWebhookEvent = defineEntity({
  name: 'StripeWebhookEvent',
  properties: {
    ...sqlBaseProperties,
    stripeId: p.string(),
    idempotencyKey: p.string().nullable(),
    eventType: p.string(),
    eventData: p.json<unknown>()
  }
});
