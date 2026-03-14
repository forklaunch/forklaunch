import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import {
  BillingProviderEnum,
  CurrencyEnum,
  PlanCadenceEnum
} from '@forklaunch/implementation-billing-stripe/enum';
import Stripe from 'stripe';

export const Plan = defineEntity({
  name: 'Plan',
  properties: {
    ...sqlBaseProperties,
    active: p.boolean(),
    name: p.string(),
    description: p.string().optional(),
    price: p.number(),
    currency: p.enum(() => CurrencyEnum),
    cadence: p.enum(() => PlanCadenceEnum),
    // tie to permissions (slugs)
    features: p.array(p.string()).nullable(),
    providerFields: p.json<Stripe.Product>(),
    externalId: p.string().unique(),
    billingProvider: p.enum(() => BillingProviderEnum).nullable()
  }
});

export type IPlan = InferEntity<typeof Plan>;
