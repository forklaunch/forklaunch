import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { Charge } from './charge.entity';
import { Diagnosis } from './diagnosis.entity';
import { Patient } from './patient.entity';

export const Encounter = defineComplianceEntity({
  name: 'Encounter',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.uuid().compliance('none'),
    patient: () => fp.manyToOne(Patient),
    // Provider is an IAM User (coder/biller-facing staff record lives in a
    // separate service) — stored as an id pointer, not a local relation.
    providerId: fp.uuid().compliance('none'),
    // Deliberately plaintext, not compliance('phi'): a visit date is a date
    // directly related to an individual (HIPAA Safe Harbor identifier #3),
    // so this is a real trade-off, not an oversight. The scrubbing engine
    // and analytics (§11's date-range summary) both need range queries
    // against this column, which an EncryptedType column can't support.
    // Recorded here per plan/cac/ §4's compliance-classification review.
    visitDate: fp.datetime().compliance('none'),
    diagnoses: () => fp.oneToMany(Diagnosis).mappedBy('encounter'),
    charges: () => fp.oneToMany(Charge).mappedBy('encounter')
  },
  userIdField: 'patient'
});
