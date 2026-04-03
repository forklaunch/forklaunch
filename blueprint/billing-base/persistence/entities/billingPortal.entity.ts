import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

// This is to represent connection information for a billing provider
export const BillingPortal = defineComplianceEntity({
  name: 'BillingPortal',
  properties: {
    ...sqlBaseProperties,
    customerId: fp.string().compliance('none'),
    uri: fp.string().nullable().compliance('none'),
    expiresAt: fp.datetime().compliance('none'),
    providerFields: fp.json<unknown>().nullable().compliance('none')
  }
});
