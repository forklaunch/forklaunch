import { describe, expect, it } from 'vitest';
import {
  DecryptionError,
  FieldEncryptor,
  MissingEncryptionKeyError,
  encryptionKeyId,
  parseEncryptionKeyList
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
      current: false
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
