import { describe, expect, it } from 'vitest';
import {
  DecryptionError,
  FieldEncryptor,
  MissingEncryptionKeyError,
  encryptionKeyId,
  isEncryptedCiphertext,
  parseEncryptionKeyList,
  stampedKeyId,
  stampedPrefix
} from '../src/persistence/fieldEncryptor';

const CURRENT = 'current-master-key-2026-09';
const PREVIOUS = 'previous-master-key-2026-03';
const OLDEST = 'oldest-master-key-2025-11';
const org = '302fbb63-1710-4738-aa60-d1fcabd2988f';

const ring = new FieldEncryptor(CURRENT, { previousKeys: [PREVIOUS, OLDEST] });
const current = new FieldEncryptor(CURRENT);
const previous = new FieldEncryptor(PREVIOUS);
const oldest = new FieldEncryptor(OLDEST);

describe('FieldEncryptor key ring', () => {
  it('writes with the current key only', () => {
    expect(ring.encrypt('x', org)).toBe(current.encrypt('x', org));
  });

  it('reads values written under any key in the ring', () => {
    expect(ring.decrypt(current.encrypt('a', org), org)).toBe('a');
    expect(ring.decrypt(previous.encrypt('b', org), org)).toBe('b');
    expect(ring.decrypt(oldest.encrypt('c', org), org)).toBe('c');
  });

  it('still refuses a key that is not in the ring', () => {
    const stranger = new FieldEncryptor('a-key-nobody-kept');
    expect(() => ring.decrypt(stranger.encrypt('d', org), org)).toThrow(
      DecryptionError
    );
  });

  it('still refuses the wrong tenant for every key', () => {
    expect(() => ring.decrypt(previous.encrypt('b', org), '')).toThrow(
      DecryptionError
    );
  });

  it('reports which key opened a value, by fingerprint', () => {
    const opened = ring.open(previous.encrypt('b', org)!, org);
    expect(opened).toEqual({
      plaintext: 'b',
      keyId: encryptionKeyId(PREVIOUS),
      current: false,
      version: 'v2',
      stale: true
    });
    expect(ring.open(current.encrypt('a', org)!, org).current).toBe(true);
    expect(ring.keyIds).toEqual({
      current: encryptionKeyId(CURRENT),
      previous: [encryptionKeyId(PREVIOUS), encryptionKeyId(OLDEST)]
    });
  });

  it('fingerprints reveal nothing and are stable', () => {
    expect(encryptionKeyId(CURRENT)).toHaveLength(12);
    expect(encryptionKeyId(CURRENT)).toBe(encryptionKeyId(CURRENT));
    expect(encryptionKeyId(CURRENT)).not.toContain(CURRENT.slice(0, 6));
  });

  it('rotates a previous-key value to the current key, same tenant', () => {
    const old = previous.encrypt('rotate-me', org)!;
    expect(ring.needsRotation(old, org)).toBe(true);
    const rotated = ring.rotate(old, org)!;
    expect(rotated).toBe(current.encrypt('rotate-me', org));
    expect(ring.needsRotation(rotated, org)).toBe(false);
    expect(ring.rotate(rotated, org)).toBe(rotated);
    expect(ring.rotate(null, org)).toBeNull();
  });

  it('drops blanks, duplicates and the current key from previous keys', () => {
    const e = new FieldEncryptor(CURRENT, {
      previousKeys: ['', CURRENT, PREVIOUS, PREVIOUS]
    });
    expect(e.keyIds.previous).toEqual([encryptionKeyId(PREVIOUS)]);
  });

  it('withPreviousKeys returns a ring with the same current key', () => {
    const e = current.withPreviousKeys([OLDEST]);
    expect(e.keyIds.current).toBe(current.keyIds.current);
    expect(e.decrypt(oldest.encrypt('c', ''), '')).toBe('c');
  });

  it('behaves exactly as before with a single key', () => {
    expect(current.keyIds.previous).toEqual([]);
    expect(() => current.decrypt(previous.encrypt('b', org), org)).toThrow(
      DecryptionError
    );
  });
});

describe('FieldEncryptor.fromEnv', () => {
  it('builds the ring from ENCRYPTION_KEY and LEGACY_ENCRYPTION_KEYS', () => {
    const e = FieldEncryptor.fromEnv({
      ENCRYPTION_KEY: CURRENT,
      LEGACY_ENCRYPTION_KEYS: `${PREVIOUS}, ${OLDEST}`,
      LEGACY_ENCRYPTION_KEY: PREVIOUS
    });
    expect(e.keyIds).toEqual(ring.keyIds);
  });

  it('requires ENCRYPTION_KEY', () => {
    expect(() => FieldEncryptor.fromEnv({})).toThrow(MissingEncryptionKeyError);
  });

  it('parses key lists in any common separator', () => {
    expect(parseEncryptionKeyList('a, b;c\nd', 'b', undefined, '"e"')).toEqual([
      'a',
      'b',
      'c',
      'd',
      'e'
    ]);
    expect(parseEncryptionKeyList(undefined, '')).toEqual([]);
  });
});

describe('v3 stamped envelope', () => {
  const v3 = new FieldEncryptor(CURRENT, {
    previousKeys: [PREVIOUS],
    format: 'v3'
  });

  it('stamps new values with the current key fingerprint', () => {
    const value = v3.encrypt('x', org)!;
    expect(value.startsWith(stampedPrefix(encryptionKeyId(CURRENT)))).toBe(
      true
    );
    expect(stampedKeyId(value)).toBe(encryptionKeyId(CURRENT));
    expect(stampedKeyId(current.encrypt('x', org)!)).toBeNull();
    expect(isEncryptedCiphertext(value)).toBe(true);
    expect(value.split(':')).toHaveLength(5);
  });

  it('is deterministic and byte-different from v2 of the same plaintext', () => {
    expect(v3.encrypt('x', org)).toBe(v3.encrypt('x', org));
    expect(v3.encrypt('x', org)).not.toBe(current.encrypt('x', org));
  });

  it('reads v3 by the stamped key and v1/v2 by trial, in either format', () => {
    const stamped = v3.encrypt('a', org)!;
    expect(v3.decrypt(stamped, org)).toBe('a');
    expect(ring.decrypt(stamped, org)).toBe('a'); // a v2 writer still reads v3
    expect(v3.decrypt(previous.encrypt('b', org), org)).toBe('b');
    expect(v3.open(stamped, org)).toMatchObject({
      keyId: encryptionKeyId(CURRENT),
      current: true,
      version: 'v3',
      stale: false
    });
  });

  it('names the missing key when a stamped value is not in the ring', () => {
    const other = new FieldEncryptor(OLDEST, { format: 'v3' });
    const value = other.encrypt('c', org)!;
    expect(() => v3.decrypt(value, org)).toThrow(encryptionKeyId(OLDEST));
    expect(() => v3.decrypt(value, org)).toThrow(DecryptionError);
    expect(v3.withPreviousKeys([PREVIOUS, OLDEST]).decrypt(value, org)).toBe(
      'c'
    );
  });

  it('still rejects the wrong tenant for a stamped value', () => {
    expect(() => v3.decrypt(v3.encrypt('a', org)!, '')).toThrow(
      DecryptionError
    );
  });

  it('treats unstamped current-key values as stale only when writing v3', () => {
    const unstamped = current.encrypt('u', org)!;
    expect(ring.needsRotation(unstamped, org)).toBe(false);
    expect(ring.rotate(unstamped, org)).toBe(unstamped);
    expect(v3.needsRotation(unstamped, org)).toBe(true);
    const rotated = v3.rotate(unstamped, org)!;
    expect(rotated).toBe(v3.encrypt('u', org));
    expect(v3.needsRotation(rotated, org)).toBe(false);
  });

  it('a v2 writer leaves a current-key v3 value alone', () => {
    const stamped = v3.encrypt('a', org)!;
    expect(ring.needsRotation(stamped, org)).toBe(false);
    expect(ring.rotate(stamped, org)).toBe(stamped);
  });

  it('fromEnv reads ENCRYPTION_FORMAT and rejects other values', () => {
    expect(
      FieldEncryptor.fromEnv({
        ENCRYPTION_KEY: CURRENT,
        ENCRYPTION_FORMAT: 'v3'
      }).format
    ).toBe('v3');
    expect(FieldEncryptor.fromEnv({ ENCRYPTION_KEY: CURRENT }).format).toBe(
      'v2'
    );
    expect(() =>
      FieldEncryptor.fromEnv({
        ENCRYPTION_KEY: CURRENT,
        ENCRYPTION_FORMAT: 'v9'
      })
    ).toThrow('ENCRYPTION_FORMAT');
    expect(current.withFormat('v3').format).toBe('v3');
  });
});
