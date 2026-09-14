/**
 * Compliance entities require a registered field encryptor before any
 * entity module is imported — same requirement as
 * diagnosis.entity.test.ts/patient.entity.test.ts.
 */
import { FieldEncryptor, registerEncryptor } from '@forklaunch/core/persistence';
registerEncryptor(new FieldEncryptor('0'.repeat(64)));

import { DateTimeType, DoubleType, StringType } from '@mikro-orm/core';
import { Remittance } from '../persistence/entities/remittance.entity';

// Remittance has no service or controller writing to it yet — see
// plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §12 item 13, still an open
// question on whether it should be trimmed, repurposed, or kept as an
// unused convenience for downstream builders. This is a bounded
// entity/schema regression test only, same shape as
// diagnosis.entity.test.ts/patient.entity.test.ts's icd10Code/dateOfBirth
// checks — it guards the entity itself against the same class of silent
// entity/migration drift, not a claim about what should build on top of it.
//
// Unlike icd10Code (an explicit .fieldName() pin) or dateOfBirth (PHI
// compliance forcing columnType), none of Remittance's properties have an
// explicit fieldName/columnType pin, so those aren't inspectable without a
// live DB connection (MikroORM only computes them during discovery). What
// *is* set synchronously by the schema builder, and so checkable here, is
// each property's declared Type class plus its array/nullable flags —
// exactly the values a compliance-tag or builder-method change would alter.
describe('Remittance entity schema', () => {
  it('keeps paidAmount a real double, not string/encrypted', () => {
    const paidAmount = Remittance.meta.properties.paidAmount as unknown as {
      type: unknown;
    };
    expect(paidAmount.type).toBe(DoubleType);
  });

  it('keeps receivedAt a real datetime, not string/encrypted — compliance is none, not phi', () => {
    // The exact failure class dateOfBirth hit: PHI compliance forces
    // EncryptedType/'text' regardless of logical type. receivedAt is
    // compliance('none'), so it must stay a genuine DateTimeType matching
    // the migration's "timestamptz not null" — if this field's compliance
    // level is ever changed to 'phi'/'pii'/'pci' without updating the
    // migration's column type, this assertion is what catches it.
    const receivedAt = Remittance.meta.properties.receivedAt as unknown as {
      type: unknown;
    };
    expect(receivedAt.type).toBe(DateTimeType);
  });

  it('keeps carcCodes and rarcCodes as nullable string arrays', () => {
    const carcCodes = Remittance.meta.properties.carcCodes as unknown as {
      type: unknown;
      array?: boolean;
      nullable?: boolean;
    };
    const rarcCodes = Remittance.meta.properties.rarcCodes as unknown as {
      type: unknown;
      array?: boolean;
      nullable?: boolean;
    };

    for (const prop of [carcCodes, rarcCodes]) {
      expect(prop.type).toBe(StringType);
      expect(prop.array).toBe(true);
      expect(prop.nullable).toBe(true);
    }
  });
});
