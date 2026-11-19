import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { Encounter } from './encounter.entity';

export const Diagnosis = defineComplianceEntity({
  name: 'Diagnosis',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.uuid().compliance('none'),
    encounter: () => fp.manyToOne(Encounter).deleteRule('cascade').updateRule('cascade'),
    // The code itself is public data (ICD-10-CM) — 'none', not 'phi'. See §4.
    //
    // .fieldName() is explicit, not decorative: MikroORM's default naming
    // strategy only inserts an underscore before an uppercase letter that
    // follows a *lowercase* letter, so "icd10Code" (digit before the "C")
    // silently maps to column "icd10code" at runtime — while the migration,
    // written by hand against the intended "icd10_code" convention, created
    // the column with the underscore. That mismatch meant every scrub()
    // call was silently reading zero diagnosis codes back off the DB (a
    // real bug — every claim was failing the LCD/NCD medical-necessity
    // check regardless of its actual diagnosis, since the read side of any
    // WHERE/hydration touching this column found nothing). Pin it explicitly
    // so property and column agree with the migration and with every other
    // snake_case column in this schema.
    icd10Code: fp.string().fieldName('icd10_code').compliance('none'),
    // The diagnosis's position as the coder entered it (1, 2, 3...). Findings
    // number diagnoses in this order, so "Diagnosis 2" is the second one entered.
    // Optional: rows without one fall back to createdAt, then id, which is
    // stable but cannot tell apart rows saved in the same millisecond.
    lineNumber: fp.integer().nullable().compliance('none')
  }
});
