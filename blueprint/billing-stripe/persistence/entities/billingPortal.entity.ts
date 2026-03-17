import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import Stripe from 'stripe';

// This is to represent connection information for a billing provider
export const billingPortal = defineEntity({
  name: 'BillingPortal',
  properties: {
    ...sqlBaseProperties,
    customerId: p.string(),
    uri: p.string().nullable(),
    expiresAt: p.datetime(),
    providerFields: p.json<Stripe.BillingPortal.Session>()
  }
});

export type BillingPortal = InferEntity<typeof billingPortal>;
