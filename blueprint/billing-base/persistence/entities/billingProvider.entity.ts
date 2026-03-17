import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { BillingProviderEnum } from '../../domain/enum/billingProvider.enum';

// This is to represent connection information for a billing provider
export const billingProvider = defineEntity({
  name: 'BillingProvider',
  properties: {
    ...sqlBaseProperties,
    externalId: p.string().unique().nullable(),
    providerFields: p.json<unknown>().nullable(),
    billingProvider: p.enum(() => BillingProviderEnum).nullable()
  }
});

export type BillingProvider = InferEntity<typeof billingProvider>;
