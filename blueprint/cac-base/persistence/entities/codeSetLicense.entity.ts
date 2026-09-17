import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { CodeSetType } from '../../domain/enum/codeSetType.enum';
import { LicenseStatus } from '../../domain/enum/licenseStatus.enum';

// Tracks whether a given organization has its own real, licensed code-set
// connector wired up — drives the per-organization feature gate in §5
// (CodeSetProviderResolver looks this up directly). This entity never
// holds any real AMA CPT content; it only tracks status. Unique on
// (organizationId, codeSetType) — see the composite constraint in
// migrations/ — so the resolver's findOne() can't land on an arbitrary
// row if duplicates were ever inserted.
export const CodeSetLicense = defineComplianceEntity({
  name: 'CodeSetLicense',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.uuid().compliance('none'),
    codeSetType: fp.enum(() => CodeSetType).compliance('none'),
    status: fp
      .enum(() => LicenseStatus)
      .default(LicenseStatus.NONE)
      .compliance('none'),
    signedAt: fp.datetime().nullable().compliance('none')
  }
});
