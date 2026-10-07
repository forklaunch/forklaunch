import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

export const StripeWebhookEvent = defineComplianceEntity({
  name: 'StripeWebhookEvent',
  properties: {
    ...sqlBaseProperties,
    stripeId: fp.string().compliance('none'),
    idempotencyKey: fp.string().nullable().compliance('none'),
    eventType: fp.string().compliance('none'),
    eventData: fp.json<unknown>().compliance('none')
  }
});
