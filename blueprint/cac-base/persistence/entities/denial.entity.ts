import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import {
  defineComplianceEntity,
  fp,
  RetentionDuration
} from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { DenialReasonCategory } from '../../domain/enum/denialReasonCategory.enum';
import { WorklistStatus } from '../../domain/enum/worklistStatus.enum';
import { Claim } from './claim.entity';

// Denial worklist entry — one row per CARC/RARC-coded rejection caught by
// the scrubbing engine (§6). This *is* the report: cac-base never submits
// claims or receives real remittances itself (§8), so every Denial row
// comes from ScrubbingService, not from a payer response.
export const Denial = defineComplianceEntity({
  name: 'Denial',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.uuid().compliance('none'),
    claim: () => fp.manyToOne(Claim),
    carcCode: fp.string().compliance('none'),
    category: fp.enum(() => DenialReasonCategory).compliance('none'),
    worklistStatus: fp
      .enum(() => WorklistStatus)
      .default(WorklistStatus.OPEN)
      .compliance('none'),
    resolvedAt: fp.datetime().nullable().compliance('none')
  },
  // HIPAA §164.530(j) — denial worklist records purged after 7 years. This
  // entity carries no PII/PHI fields of its own (patient identity lives on
  // Claim/Patient, each with their own retention posture), so 'delete' is
  // the correct action here rather than 'anonymize', which would be a no-op
  // with nothing to null out. See plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §4.
  retention: {
    duration: RetentionDuration.years(7),
    action: 'delete'
  }
});

export type Denial = InferEntity<typeof Denial>;
