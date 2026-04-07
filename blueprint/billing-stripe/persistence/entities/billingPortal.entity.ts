import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { StripeBillingPortalSession } from '@forklaunch/implementation-billing-stripe/types';

// This is to represent connection information for a billing provider
export const BillingPortal = defineComplianceEntity({
  name: 'BillingPortal',
  properties: {
    ...sqlBaseProperties,
    customerId: fp.string().compliance('none'),
    uri: fp.string().nullable().compliance('none'),
    expiresAt: fp.datetime().compliance('none'),
    providerFields: fp.json<StripeBillingPortalSession>().compliance('none')
  }
});
