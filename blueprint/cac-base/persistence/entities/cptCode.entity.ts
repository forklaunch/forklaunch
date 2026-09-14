import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

// Real-CPT reference table — the concrete target of the CptCodeProvider
// extension point (§5). Unlike Icd10Code/HcpcsCode this IS org-scoped: real
// CPT content is licensed per adopting organization, and different
// organizations may be on different editions/vintages of it (§5 — "which
// CPT edition a customer's real feed uses is entirely their decision").
// ForkLaunch never populates this table with real content itself — it stays
// empty until an organization points scripts/refresh-code-sets.ts at their
// own licensed feed. Uniqueness is (organizationId, code) — see the
// composite unique constraint in migrations/ — not a single-column
// constraint on `code` alone, since code repeats across organizations.
export const CptCode = defineComplianceEntity({
  name: 'CptCode',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.uuid().compliance('none'),
    code: fp.string().compliance('none'),
    description: fp.string().compliance('none'),
    effectiveDate: fp.datetime().nullable().compliance('none')
  }
});
