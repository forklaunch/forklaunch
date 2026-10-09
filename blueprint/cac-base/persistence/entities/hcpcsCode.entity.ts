import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

// Global reference/lookup table, bulk-loaded by the HCPCS Level II ETL
// loader (§7) — not org-scoped, since HCPCS is free, public CMS data shared
// identically across every tenant. Populated by scripts/refresh-code-sets.ts,
// never hand-seeded.
export const HcpcsCode = defineComplianceEntity({
  name: 'HcpcsCode',
  properties: {
    ...sqlBaseProperties,
    code: fp.string().unique().compliance('none'),
    description: fp.string().compliance('none'),
    // CMS releases HCPCS quarterly (Jan/Apr/Jul/Oct) — §7.
    effectiveDate: fp.datetime().nullable().compliance('none')
  }
});
