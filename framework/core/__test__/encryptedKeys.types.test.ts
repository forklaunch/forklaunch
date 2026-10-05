import { describe, expectTypeOf, it } from 'vitest';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import type {
  EncryptedKeysOf,
  RequiresEncryptionContext,
  SelectionAvoidsEncryptedColumns
} from '../src/persistence/complianceTypes';

/**
 * These exist so the class of production bug that took IAM sign-up down can be
 * detected by the compiler instead of by an outage.
 *
 * The failure shape: `OrganizationUser` carries member PII encrypted under the
 * ORGANISATION's key, and a lookup reads that row to find out which
 * organisation it belongs to. The context needed to decrypt is what the read is
 * trying to discover, so a full select can never succeed — it fails at runtime
 * with "ciphertext is corrupted or the wrong key was used". The remedy is a
 * partial select omitting the encrypted columns.
 *
 * Until now nothing in the type system distinguished an encrypted column from
 * an unencrypted one: `.compliance()` recorded only THAT a property was
 * classified, not which level. Carrying the literal level makes the distinction
 * expressible, which is what these assert.
 */

const membershipProperties = {
  organizationId: fp.string().compliance('none'),
  role: fp.string().compliance('none'),
  memberEmail: fp.string().nullable().compliance('pii'),
  memberName: fp.string().nullable().compliance('pii')
};

const unclassifiedProperties = {
  slug: fp.string().compliance('none'),
  label: fp.string().compliance('none')
};

const paymentProperties = {
  reference: fp.string().compliance('none'),
  cardToken: fp.string().compliance('pci'),
  diagnosis: fp.string().compliance('phi')
};

describe('EncryptedKeysOf', () => {
  it('names exactly the properties that are encrypted at rest', () => {
    expectTypeOf<EncryptedKeysOf<typeof membershipProperties>>().toEqualTypeOf<
      'memberEmail' | 'memberName'
    >();
  });

  it('treats compliance("none") as unencrypted, not merely unclassified', () => {
    // The distinction the old `'~c': true` marker could not make.
    expectTypeOf<
      EncryptedKeysOf<typeof unclassifiedProperties>
    >().toEqualTypeOf<never>();
  });

  it('covers every encrypting level, not just pii', () => {
    expectTypeOf<EncryptedKeysOf<typeof paymentProperties>>().toEqualTypeOf<
      'cardToken' | 'diagnosis'
    >();
  });
});

describe('RequiresEncryptionContext', () => {
  it('is true when a full read would decrypt something', () => {
    expectTypeOf<
      RequiresEncryptionContext<typeof membershipProperties>
    >().toEqualTypeOf<true>();
  });

  it('is false when nothing is encrypted', () => {
    expectTypeOf<
      RequiresEncryptionContext<typeof unclassifiedProperties>
    >().toEqualTypeOf<false>();
  });
});

describe('SelectionAvoidsEncryptedColumns', () => {
  it('accepts the partial select that fixed the outage', () => {
    // `fields: ['id', 'organization']` — the shape auth.ts now uses.
    expectTypeOf<
      SelectionAvoidsEncryptedColumns<
        typeof membershipProperties,
        readonly ['organizationId', 'role']
      >
    >().toEqualTypeOf<true>();
  });

  it('rejects a selection that pulls in an encrypted column', () => {
    expectTypeOf<
      SelectionAvoidsEncryptedColumns<
        typeof membershipProperties,
        readonly ['organizationId', 'memberEmail']
      >
    >().toEqualTypeOf<false>();
  });

  it('rejects a selection that is encrypted-only', () => {
    expectTypeOf<
      SelectionAvoidsEncryptedColumns<
        typeof membershipProperties,
        readonly ['memberName']
      >
    >().toEqualTypeOf<false>();
  });

  it('accepts any selection when the entity has nothing encrypted', () => {
    expectTypeOf<
      SelectionAvoidsEncryptedColumns<
        typeof unclassifiedProperties,
        readonly ['slug', 'label']
      >
    >().toEqualTypeOf<true>();
  });
});
