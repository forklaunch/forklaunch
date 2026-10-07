import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { BillingProviderEnum } from '../../domain/enum/billingProvider.enum';

// This is to represent connection information for a billing provider
export const BillingProvider = defineComplianceEntity({
  name: 'BillingProvider',
  properties: {
    ...sqlBaseProperties,
    externalId: fp.string().unique().nullable().compliance('none'),
    providerFields: fp.json<unknown>().nullable().compliance('none'),
    billingProvider: fp
      .enum(() => BillingProviderEnum)
      .default(BillingProviderEnum.NONE)
      .compliance('none')
  }
});
