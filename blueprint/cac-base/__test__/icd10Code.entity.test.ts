/**
 * Compliance entities require a registered field encryptor before any
 * entity module is imported — same requirement as
 * diagnosis.entity.test.ts/patient.entity.test.ts.
 */
import { FieldEncryptor, registerEncryptor } from '@forklaunch/core/persistence';
registerEncryptor(new FieldEncryptor('0'.repeat(64)));

import { Icd10Code } from '../persistence/entities/icd10Code.entity';

// Regression test: same bug class as Diagnosis.icd10Code, one level up —
// MikroORM's default naming strategy only inserts an underscore before an
// uppercase letter that follows a *lowercase* letter, so the entity name
// "Icd10Code" (a digit precedes the "C") silently mapped to table
// "icd10code" at runtime, while the hand-written migration created
// "icd10_code". Every real query against this entity threw
// TableNotFoundException as a result — CodeValidationService.validateIcd10()
// and the real scripts/refresh-code-sets.ts ETL loader have never worked.
// tableName now pins 'icd10_code' explicitly; this test fails loudly if
// that pin is ever removed. Confirmed via
// UnderscoreNamingStrategy.classToTableName directly: HcpcsCode/CptCode are
// unaffected (a lowercase letter precedes their "Code"), only Icd10Code's
// digit-before-uppercase pattern triggers this.
describe('Icd10Code entity schema', () => {
  it('maps to the icd10_code table, not icd10code', () => {
    expect(Icd10Code.meta.tableName).toBe('icd10_code');
  });
});
