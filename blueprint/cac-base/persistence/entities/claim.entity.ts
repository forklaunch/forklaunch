import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { ClaimStatus } from '../../domain/enum/claimStatus.enum';
import { CodeSetProviderType } from '../../domain/enum/codeSetProviderType.enum';
import { Denial } from './denial.entity';
import { Encounter } from './encounter.entity';
import { Insurance } from './insurance.entity';
import { Patient } from './patient.entity';
import { Remittance } from './remittance.entity';

// A claim is built from one encounter's charges + diagnoses — see
// services/claim.service.ts for the builder/scrubbing orchestration (§6).
export const Claim = defineComplianceEntity({
  name: 'Claim',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.uuid().compliance('none'),
    patient: () => fp.manyToOne(Patient),
    // One claim per encounter — see the claim_encounter_id_unique
    // constraint in migrations/ (same reasoning as Patient.mrn's composite
    // unique: expressed in the migration, not chained here).
    encounter: () => fp.manyToOne(Encounter),
    // The payer being billed — nullable for self-pay claims.
    payer: () => fp.manyToOne(Insurance).nullable(),
    status: fp
      .enum(() => ClaimStatus)
      .default(ClaimStatus.DRAFT)
      .compliance('none'),
    // Which CodeSetProvider this claim was actually built under — resolved
    // once at build time (see ClaimService.buildClaim) and never
    // re-resolved. A later CodeSetLicense flip never changes this claim's
    // own history (§5's "historical claims are never retroactively
    // recoded" rule) — only claims built after the flip get 'cpt'.
    codeSetType: fp
      .enum(() => CodeSetProviderType)
      .default(CodeSetProviderType.MOCK)
      .compliance('none'),
    remittances: () => fp.oneToMany(Remittance).mappedBy('claim'),
    denials: () => fp.oneToMany(Denial).mappedBy('claim')
  },
  userIdField: 'patient'
});

export type Claim = InferEntity<typeof Claim>;
