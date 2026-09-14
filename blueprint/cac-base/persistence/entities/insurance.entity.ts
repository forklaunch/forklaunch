import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { Patient } from './patient.entity';

export const Insurance = defineComplianceEntity({
  name: 'Insurance',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.uuid().compliance('none'),
    patient: () => fp.manyToOne(Patient),
    payerName: fp.string().compliance('none'),
    memberId: fp.string().compliance('phi'),
    groupNumber: fp.string().nullable().compliance('none')
  },
  userIdField: 'patient'
});
