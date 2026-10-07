import { describe, expect, expectTypeOf, it } from 'vitest';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';
import {
  asEncryptionSafe,
  type ContextFreeKeysOfSchema,
  type EncryptedKeysOfSchema,
  type EncryptionAwareReadOptions,
  type SchemaRequiresEncryptionContext
} from '../src/persistence/encryptionSafeEm';

/**
 * The production failure these exist to prevent: a lookup reads a row to
 * discover which organisation it belongs to, while that row carries columns
 * encrypted under that same organisation's key. Hydration cannot succeed —
 * "ciphertext is corrupted or the wrong key was used" — and the endpoint 500s
 * with no way to have known at review time.
 *
 * These assert the type now knows the difference.
 */

const MembershipEntity = defineComplianceEntity({
  name: 'Membership',
  properties: {
    organization: fp.string().compliance('none'),
    role: fp.string().compliance('none'),
    memberEmail: fp.string().nullable().compliance('pii'),
    memberName: fp.string().nullable().compliance('pii')
  }
});

const AuditEntity = defineComplianceEntity({
  name: 'Audit',
  properties: {
    action: fp.string().compliance('none'),
    at: fp.datetime().compliance('none')
  }
});

describe('schema-level encryption introspection', () => {
  it('reads the encrypted keys back off a defined entity', () => {
    expectTypeOf<
      EncryptedKeysOfSchema<typeof MembershipEntity>
    >().toEqualTypeOf<'memberEmail' | 'memberName'>();
  });

  it('reports nothing encrypted for an entity that classifies everything none', () => {
    expectTypeOf<
      EncryptedKeysOfSchema<typeof AuditEntity>
    >().toEqualTypeOf<never>();
  });

  it('knows which schemas need a context for a full read', () => {
    expectTypeOf<
      SchemaRequiresEncryptionContext<typeof MembershipEntity>
    >().toEqualTypeOf<true>();
    expectTypeOf<
      SchemaRequiresEncryptionContext<typeof AuditEntity>
    >().toEqualTypeOf<false>();
  });

  it('names the columns that are safe to select without a context', () => {
    expectTypeOf<
      ContextFreeKeysOfSchema<typeof MembershipEntity>
    >().toEqualTypeOf<'organization' | 'role'>();
  });
});

describe('EncryptionAwareReadOptions', () => {
  it('accepts the partial select that fixed the outage', () => {
    const options = { fields: ['organization', 'role'] } as const;
    expectTypeOf(options).toMatchTypeOf<
      EncryptionAwareReadOptions<typeof MembershipEntity, object>
    >();
  });

  it('accepts an explicit bound-context acknowledgement', () => {
    const options = { encryptionContextIsBound: true } as const;
    expectTypeOf(options).toMatchTypeOf<
      EncryptionAwareReadOptions<typeof MembershipEntity, object>
    >();
  });

  it('REJECTS a selection that pulls in an encrypted column', () => {
    const options = { fields: ['organization', 'memberEmail'] } as const;
    expectTypeOf(options).not.toMatchTypeOf<
      EncryptionAwareReadOptions<typeof MembershipEntity, object>
    >();
  });

  it('REJECTS a read that neither selects around nor declares a context', () => {
    // The shape that caused the outage: a bare full select.
    const options = {} as const;
    expectTypeOf(options).not.toMatchTypeOf<
      EncryptionAwareReadOptions<typeof MembershipEntity, object>
    >();
  });

  it('leaves unencrypted entities entirely unconstrained', () => {
    // A plain CRUD entity must not be made harder to read.
    expectTypeOf({}).toMatchTypeOf<
      EncryptionAwareReadOptions<typeof AuditEntity, object>
    >();
    expectTypeOf({ fields: ['action'] } as const).toMatchTypeOf<
      EncryptionAwareReadOptions<typeof AuditEntity, object>
    >();
  });
});

describe('asEncryptionSafe', () => {
  it('returns the same object, narrowing only the type', () => {
    const em = { findOne: async () => null, flush: async () => undefined };
    expect(asEncryptionSafe(em)).toBe(em);
  });

  it('keeps non-read members reachable', () => {
    const em = { findOne: async () => null, flush: async () => undefined };
    expectTypeOf(asEncryptionSafe(em).flush).toBeFunction();
  });
});
