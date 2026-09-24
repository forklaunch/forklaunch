import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

// Global reference/lookup table, bulk-loaded by the ICD-10-CM ETL loader
// (§7) — not org-scoped, since ICD-10-CM is free, public CDC/NCHS data
// shared identically across every tenant. Populated by
// scripts/refresh-code-sets.ts, never hand-seeded.
// MikroORM's default naming strategy only inserts an underscore before an
// uppercase letter that follows a *lowercase* letter — "Icd10Code" (a digit
// precedes the "C") silently maps to table "icd10code", while the
// hand-written migration created "icd10_code" to match this schema's
// snake_case convention everywhere else (confirmed directly against
// UnderscoreNamingStrategy.classToTableName — HcpcsCode/CptCode are
// unaffected, since a lowercase letter precedes their "Code"). Without this
// pin, every real query against this entity throws
// TableNotFoundException — CodeValidationService.validateIcd10() and the
// real scripts/refresh-code-sets.ts ETL loader have never worked. Same bug
// class, same fix, as Diagnosis.icd10Code's .fieldName('icd10_code') pin —
// this is the entity-name-level version of it.
export const Icd10Code = defineComplianceEntity({
  name: 'Icd10Code',
  tableName: 'icd10_code',
  properties: {
    ...sqlBaseProperties,
    code: fp.string().unique().compliance('none'),
    description: fp.string().compliance('none'),
    // The CDC/NCHS release this row came from — effective October 1 each
    // year (§7).
    effectiveDate: fp.datetime().nullable().compliance('none')
  }
});
