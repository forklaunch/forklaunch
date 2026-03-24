import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import {
  BillingProviderEnum,
  CurrencyEnum,
  PlanCadenceEnum
} from '@forklaunch/implementation-billing-stripe/enum';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import Stripe from 'stripe';

export const Plan = defineComplianceEntity({
  name: 'Plan',
  properties: {
    ...sqlBaseProperties,
    active: fp.boolean().compliance('none'),
    name: fp.string().compliance('none'),
    description: fp.string().nullable().compliance('none'),
    price: fp.double().compliance('none'),
    currency: fp.enum(() => CurrencyEnum).compliance('none'),
    cadence: fp.enum(() => PlanCadenceEnum).compliance('none'),
    // tie to permissions (slugs)
    features: fp.string().array().nullable().compliance('none'),
    providerFields: fp.json<Stripe.Product>().compliance('none'),
    externalId: fp.string().unique().compliance('none'),
    billingProvider: fp.enum(() => BillingProviderEnum).compliance('none')
  }
});
