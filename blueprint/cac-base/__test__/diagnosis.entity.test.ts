/**
 * Compliance entities require a registered field encryptor before any
 * entity module is imported — same requirement as
 * codeSetProviderResolver.test.ts.
 */
import { FieldEncryptor, registerEncryptor } from '@forklaunch/core/persistence';
registerEncryptor(new FieldEncryptor('0'.repeat(64)));

import { Diagnosis } from '../persistence/entities/diagnosis.entity';

// Regression test: MikroORM's default naming strategy only inserts an
// underscore before an uppercase letter that follows a *lowercase* letter,
// so "icd10Code" (a digit precedes the "C") silently mapped to column
// "icd10code" at runtime, while the hand-written migration created
// "icd10_code" to match this schema's snake_case convention everywhere
// else. That mismatch meant every read/write of a diagnosis code silently
// missed the real column — scrub() was reading zero diagnosis codes back
// off the DB for every claim, regardless of what was actually diagnosed.
// icd10Code now pins .fieldName('icd10_code') explicitly; this test fails
// loudly if that pin is ever removed.
describe('Diagnosis entity schema', () => {
  it('maps icd10Code to the icd10_code column, not icd10code', () => {
    // `fieldName` (singular) is populated on the pre-discovery EntitySchema
    // metadata this test reads without a live DB connection; MikroORM's
    // EntityProperty type only declares the post-discovery `fieldNames`
    // (plural, array) shape, hence the cast.
    const icd10Code = Diagnosis.meta.properties.icd10Code as unknown as {
      fieldName: string;
    };
    expect(icd10Code.fieldName).toBe('icd10_code');
  });
});
