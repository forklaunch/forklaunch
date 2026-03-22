import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineEntity, p } from '@mikro-orm/core';
import { BillingProviderEnum } from '../../domain/enum/billingProvider.enum';
import { CurrencyEnum } from '../../domain/enum/currency.enum';
import { PlanCadenceEnum } from '../../domain/enum/planCadence.enum';

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
    providerFields: p.json<unknown>().nullable(),
    externalId: p.string().unique(),
    billingProvider: p.enum(() => BillingProviderEnum)
  }
});
