import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { BillingProviderEnum } from '../../domain/enum/billingProvider.enum';
import { CurrencyEnum } from '../../domain/enum/currency.enum';
import { PlanCadenceEnum } from '../../domain/enum/planCadence.enum';

export const plan = defineEntity({
  name: 'Plan',
  properties: {
    ...sqlBaseProperties,
    active: p.boolean(),
    name: p.string(),
    description: p.string().nullable(),
    price: p.number(),
    currency: p.enum(() => CurrencyEnum),
    cadence: p.enum(() => PlanCadenceEnum),
    // tie to permissions (slugs)
    features: p.array(p.string()).nullable(),
    providerFields: p.json<unknown>().nullable(),
    externalId: p.string().unique(),
    billingProvider: p.enum(() => BillingProviderEnum).nullable()
  }
});

export type Plan = InferEntity<typeof plan>;
