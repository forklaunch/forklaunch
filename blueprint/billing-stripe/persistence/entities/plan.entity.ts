import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import {
  BillingProviderEnum,
  CurrencyEnum,
  PlanCadenceEnum
} from '@forklaunch/implementation-billing-stripe/enum';
import { defineEntity, p } from '@mikro-orm/core';
import Stripe from 'stripe';

export const Plan = defineEntity({
  name: 'Plan',
  properties: {
    ...sqlBaseProperties,
    active: p.boolean(),
    name: p.string(),
    description: p.string().nullable(),
    price: p.double(),
    currency: p.enum(() => CurrencyEnum),
    cadence: p.enum(() => PlanCadenceEnum),
    // tie to permissions (slugs)
    features: p.string().array().nullable(),
    providerFields: p.json<Stripe.Product>(),
    externalId: p.string().unique(),
    billingProvider: p.enum(() => BillingProviderEnum)
  }
});
