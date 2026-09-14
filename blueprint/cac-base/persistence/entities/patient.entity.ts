import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { Claim } from './claim.entity';
import { Encounter } from './encounter.entity';
import { Insurance } from './insurance.entity';

// Every hospital/clinic client is one Organization (from the existing IAM
// module, a separate service) — organizationId is the tenant column the
// framework's tenant-isolation filter matches on. See plan/cac/ §3.
export const Patient = defineComplianceEntity({
  name: 'Patient',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.uuid().compliance('none'),
    // Internal surrogate identifier — used as the reference everywhere in
    // the domain model instead of SSN, per HIPAA "minimum necessary" (§4).
    // Uniqueness is (organizationId, mrn) — see the composite unique
    // constraint in migrations/, same reasoning as cptCode.entity.ts — not
    // a single-column constraint here, since an MRN is only unique within
    // the hospital/clinic that issued it.
    mrn: fp.string().compliance('none'),
    firstName: fp.string().compliance('phi'),
    lastName: fp.string().compliance('phi'),
    dateOfBirth: fp.datetime().compliance('phi'),
    addressLine1: fp.string().nullable().compliance('phi'),
    city: fp.string().nullable().compliance('phi'),
    state: fp.string().nullable().compliance('phi'),
    postalCode: fp.string().nullable().compliance('phi'),
    phoneNumber: fp.string().nullable().compliance('phi'),
    email: fp.string().nullable().compliance('phi'),
    // Only populated when a specific payer integration requires it — never
    // used as a primary key. See §4's "On SSN specifically" note.
    ssn: fp.string().nullable().compliance('phi'),
    encounters: () => fp.oneToMany(Encounter).mappedBy('patient'),
    insurances: () => fp.oneToMany(Insurance).mappedBy('patient'),
    claims: () => fp.oneToMany(Claim).mappedBy('patient')
  },
  // Compliance data service resolves erase/export requests by treating the
  // Patient record itself as the "user" — see registrations.ts.
  userIdField: 'id'
});
