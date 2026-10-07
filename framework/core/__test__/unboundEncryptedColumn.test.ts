import { Type } from '@mikro-orm/core';
import { describe, expect, it } from 'vitest';
import {
  EncryptedType,
  FieldEncryptor,
  UnboundTenantError,
  registerEncryptor,
  withEncryptionContext
} from '../src/persistence';

/**
 * AN ENCRYPTED COLUMN TOUCHED WITH NO TENANT BOUND MUST FAIL WHERE IT HAPPENS.
 *
 * It used to fall back to `getCurrentTenantId()`, which answers `''` when
 * nothing is bound — so:
 *
 *   write   the row was encrypted under the empty key and looked written. It
 *           failed later, for the tenant that owned it, as "Failed to decrypt
 *           encrypted column value" — blaming the reader for the writer's bug.
 *           This is exactly how IAM's auth hooks flushed user PII: they took
 *           `wrap(entity).__em`, the RAW manager, which escapes the tenant
 *           proxy, and every row went under `''`.
 *
 *   query   a WHERE value on an encrypted column is encrypted before it is
 *           compared. Under the empty key it simply matched nothing: not an
 *           error, an empty result set. Silent, and wrong.
 *
 * Both now raise `UnboundTenantError` naming the operation, at the call that
 * forgot to bind.
 */

const platform = '_internal';
const encrypted = new EncryptedType('string');
const fakePlatform = {} as never;

registerEncryptor(new FieldEncryptor('unbound-column-test-master-key'));

describe('an encrypted column with no tenant bound', () => {
  it('refuses to encrypt a value for writing', () => {
    expect(() =>
      encrypted.convertToDatabaseValue('ada@example.com', fakePlatform, {
        fromQuery: false
      } as never)
    ).toThrow(UnboundTenantError);
  });

  it('refuses to encrypt a WHERE value rather than matching nothing', () => {
    // The failure mode this replaces returned zero rows and no error.
    expect(() =>
      encrypted.convertToDatabaseValue('ada@example.com', fakePlatform, {
        fromQuery: true
      } as never)
    ).toThrow(/Cannot write an encrypted column with no tenant bound/);
  });

  it('refuses to decrypt a stored value', () => {
    const ciphertext = withEncryptionContext(platform, () =>
      encrypted.convertToDatabaseValue('ada@example.com', fakePlatform, {
        fromQuery: false
      } as never)
    ) as string;
    expect(ciphertext.startsWith('v')).toBe(true);

    expect(() => encrypted.convertToJSValue(ciphertext, fakePlatform)).toThrow(
      UnboundTenantError
    );
  });

  it('names the operation it could not do, and how to bind one', () => {
    expect(() =>
      encrypted.convertToDatabaseValue('x', fakePlatform, {} as never)
    ).toThrow(/withEncryptionContext[\s\S]*wrapEmWithTenantContext/);
  });

  it('round-trips under a bound tenant', () => {
    const value = 'ada@example.com';
    const roundTripped = withEncryptionContext(platform, () => {
      const stored = encrypted.convertToDatabaseValue(value, fakePlatform, {
        fromQuery: false
      } as never) as string;
      return encrypted.convertToJSValue(stored, fakePlatform);
    });
    expect(roundTripped).toBe(value);
  });

  it('does not decrypt under a different tenant', () => {
    const stored = withEncryptionContext(platform, () =>
      encrypted.convertToDatabaseValue('ada@example.com', fakePlatform, {
        fromQuery: false
      } as never)
    ) as string;
    expect(() =>
      withEncryptionContext('302fbb63-1710-4738-aa60-d1fcabd2988f', () =>
        encrypted.convertToJSValue(stored, fakePlatform)
      )
    ).toThrow(/Failed to decrypt encrypted column value/);
  });

  it('leaves a plaintext (unencrypted) stored value alone', () => {
    // A column that has not been encrypted yet must still read back without a
    // tenant, or a migration could never walk the table to encrypt it.
    expect(encrypted.convertToJSValue('not-encrypted', fakePlatform)).toBe(
      'not-encrypted'
    );
    expect(encrypted.convertToJSValue(null, fakePlatform)).toBeNull();
  });

  it('is a Type MikroORM can use', () => {
    expect(encrypted).toBeInstanceOf(Type);
  });
});
