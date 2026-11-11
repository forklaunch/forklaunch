/**
 * Compliance entities require a registered field encryptor before any
 * entity module is imported — same requirement as
 * codeSetProviderResolver.test.ts.
 */
import { FieldEncryptor, registerEncryptor } from '@forklaunch/core/persistence';
registerEncryptor(new FieldEncryptor('0'.repeat(64)));

import { Patient } from '../persistence/entities/patient.entity';

// Regression test: dateOfBirth is compliance('phi'), and PHI compliance
// forces EncryptedType regardless of the property's logical type —
// ciphertext is always a string. The hand-written migration created this
// column as timestamptz (a reasonable guess for a "datetime" field that
// didn't account for encryption), so every patient insert failed outright
// with a Postgres DateTimeParseError. This test locks the entity's own
// columnType to 'text' so a future migration change can be checked
// against it instead of only being caught by a real DB round-trip.
describe('Patient entity schema', () => {
  it('stores dateOfBirth as text (PHI encryption always produces ciphertext)', () => {
    // `columnType` (singular) is populated on the pre-discovery EntitySchema
    // metadata this test reads without a live DB connection; MikroORM's
    // EntityProperty type only declares the post-discovery `columnTypes`
    // (plural, array) shape, hence the cast — same as
    // diagnosis.entity.test.ts's `fieldName` cast.
    const dateOfBirth = Patient.meta.properties.dateOfBirth as unknown as {
      columnType: string;
    };
    expect(dateOfBirth.columnType).toBe('text');
  });
});
