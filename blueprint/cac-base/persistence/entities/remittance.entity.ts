import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { Claim } from './claim.entity';

// One row per 835/ERA response applied to a claim — see §8.
export const Remittance = defineComplianceEntity({
  name: 'Remittance',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.uuid().compliance('none'),
    claim: () => fp.manyToOne(Claim),
    paidAmount: fp.double().compliance('none'),
    // CARC/RARC codes present on this remittance line, if any — see §6's
    // CARC reference table.
    carcCodes: fp.string().array().nullable().compliance('none'),
    rarcCodes: fp.string().array().nullable().compliance('none'),
    receivedAt: fp.datetime().compliance('none')
  }
});
