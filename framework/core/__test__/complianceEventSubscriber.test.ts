import { describe, expect, it, beforeEach } from 'vitest';
import { Platform } from '@mikro-orm/core';
import { FieldEncryptor } from '../src/persistence/fieldEncryptor';
import { wrapEmWithNativeQueryBlocking } from '../src/persistence/complianceEventSubscriber';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';
import {
  EncryptedType,
  registerEncryptor
} from '../src/persistence/encryptedType';

const MASTER_KEY = 'test-master-key-for-unit-tests-32ch';

// Register test entities
defineComplianceEntity({
  name: 'Patient',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    name: fp.string().compliance('pii'),
    ssn: fp.string().compliance('phi'),
    cardNumber: fp.string().compliance('pci'),
    status: fp.string().compliance('none')
  }
});

defineComplianceEntity({
  name: 'PublicEntity',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    label: fp.string().compliance('none')
  }
});

describe('EncryptedType', () => {
  let encryptedType: EncryptedType;
  const platform = {} as Platform;

  beforeEach(() => {
    const encryptor = new FieldEncryptor(MASTER_KEY);
    registerEncryptor(encryptor);
    encryptedType = new EncryptedType('string');
  });

  describe('convertToDatabaseValue', () => {
    it('encrypts plaintext strings', () => {
      const result = encryptedType.convertToDatabaseValue('hello', platform);
      expect(result).toMatch(/^v[12]:/);
    });

    it('returns null for null', () => {
      expect(encryptedType.convertToDatabaseValue(null, platform)).toBeNull();
    });

    it('does not double-encrypt', () => {
      const encrypted = encryptedType.convertToDatabaseValue('hello', platform);
      const result = encryptedType.convertToDatabaseValue(encrypted, platform);
      expect(result).toBe(encrypted);
    });

    it('returns empty string as-is', () => {
      expect(encryptedType.convertToDatabaseValue('', platform)).toBe('');
    });
  });

  describe('convertToJSValue', () => {
    it('decrypts encrypted strings', () => {
      const encrypted = encryptedType.convertToDatabaseValue(
        'hello',
        platform
      ) as string;
      const result = encryptedType.convertToJSValue(encrypted, platform);
      expect(result).toBe('hello');
    });

    it('returns null for null', () => {
      expect(encryptedType.convertToJSValue(null, platform)).toBeNull();
    });

    it('passes through plaintext (pre-migration)', () => {
      const result = encryptedType.convertToJSValue('plain value', platform);
      expect(result).toBe('plain value');
    });
  });

  describe('roundtrip', () => {
    it('roundtrips string values', () => {
      const original = 'sensitive data';
      const encrypted = encryptedType.convertToDatabaseValue(
        original,
        platform
      );
      const decrypted = encryptedType.convertToJSValue(encrypted, platform);
      expect(decrypted).toBe(original);
    });

    it('roundtrips JSON values', () => {
      const jsonType = new EncryptedType('any');
      const original = { key: 'value', nested: { a: 1 } };
      const encrypted = jsonType.convertToDatabaseValue(original, platform);
      expect(typeof encrypted).toBe('string');
      expect(encrypted).toMatch(/^v[12]:/);
      const decrypted = jsonType.convertToJSValue(encrypted, platform);
      expect(decrypted).toEqual(original);
    });

    it('roundtrips number values', () => {
      const numberType = new EncryptedType('number');
      const encrypted = numberType.convertToDatabaseValue(42, platform);
      expect(encrypted).toMatch(/^v[12]:/);
      const decrypted = numberType.convertToJSValue(encrypted, platform);
      expect(decrypted).toBe(42);
    });

    it('roundtrips boolean values', () => {
      const boolType = new EncryptedType('boolean');
      const encrypted = boolType.convertToDatabaseValue(true, platform);
      expect(encrypted).toMatch(/^v[12]:/);
      const decrypted = boolType.convertToJSValue(encrypted, platform);
      expect(decrypted).toBe(true);
    });

    it('roundtrips Date values', () => {
      const dateType = new EncryptedType('Date');
      const original = new Date('2024-06-15T10:30:00.000Z');
      const encrypted = dateType.convertToDatabaseValue(original, platform);
      expect(encrypted).toMatch(/^v[12]:/);
      const decrypted = dateType.convertToJSValue(encrypted, platform);
      expect(decrypted).toBeInstanceOf(Date);
      expect((decrypted as Date).toISOString()).toBe(original.toISOString());
    });

    it('roundtrips bigint values', () => {
      const bigintType = new EncryptedType('bigint');
      const original = BigInt('9007199254740993');
      const encrypted = bigintType.convertToDatabaseValue(original, platform);
      expect(encrypted).toMatch(/^v[12]:/);
      const decrypted = bigintType.convertToJSValue(encrypted, platform);
      expect(decrypted).toBe(original);
    });

    it('roundtrips Buffer values', () => {
      const bufferType = new EncryptedType('Buffer');
      const original = Buffer.from('binary data');
      const encrypted = bufferType.convertToDatabaseValue(original, platform);
      expect(encrypted).toMatch(/^v[12]:/);
      const decrypted = bufferType.convertToJSValue(encrypted, platform);
      expect(Buffer.isBuffer(decrypted)).toBe(true);
      expect((decrypted as Buffer).toString()).toBe('binary data');
    });

    it('roundtrips float/double as number', () => {
      const numberType = new EncryptedType('number');
      const original = 3.14159;
      const encrypted = numberType.convertToDatabaseValue(original, platform);
      const decrypted = numberType.convertToJSValue(encrypted, platform);
      expect(decrypted).toBeCloseTo(original);
    });
  });

  describe('array container roundtrip', () => {
    it('roundtrips array of strings', () => {
      const type = new EncryptedType('string', true);
      const original = ['alice', 'bob', 'charlie'];
      const encrypted = type.convertToDatabaseValue(original, platform);
      expect(encrypted).toMatch(/^v[12]:/);
      const decrypted = type.convertToJSValue(encrypted, platform);
      expect(decrypted).toEqual(original);
    });

    it('roundtrips array of numbers', () => {
      const type = new EncryptedType('number', true);
      const original = [1, 2.5, 42, 0, -7];
      const encrypted = type.convertToDatabaseValue(original, platform);
      const decrypted = type.convertToJSValue(encrypted, platform);
      expect(decrypted).toEqual(original);
    });

    it('roundtrips array of booleans', () => {
      const type = new EncryptedType('boolean', true);
      const original = [true, false, true];
      const encrypted = type.convertToDatabaseValue(original, platform);
      const decrypted = type.convertToJSValue(encrypted, platform);
      expect(decrypted).toEqual(original);
    });

    it('roundtrips array of Dates', () => {
      const type = new EncryptedType('Date', true);
      const original = [
        new Date('2024-01-01T00:00:00.000Z'),
        new Date('2024-06-15T10:30:00.000Z')
      ];
      const encrypted = type.convertToDatabaseValue(original, platform);
      const decrypted = type.convertToJSValue(encrypted, platform) as Date[];
      expect(decrypted).toHaveLength(2);
      expect(decrypted[0]).toBeInstanceOf(Date);
      expect(decrypted[0].toISOString()).toBe('2024-01-01T00:00:00.000Z');
      expect(decrypted[1].toISOString()).toBe('2024-06-15T10:30:00.000Z');
    });

    it('roundtrips empty array', () => {
      const type = new EncryptedType('number', true);
      const encrypted = type.convertToDatabaseValue([], platform);
      const decrypted = type.convertToJSValue(encrypted, platform);
      expect(decrypted).toEqual([]);
    });

    it('roundtrips array of JSON objects', () => {
      const type = new EncryptedType('any', true);
      const original = [{ a: 1 }, { b: 'two' }];
      const encrypted = type.convertToDatabaseValue(original, platform);
      const decrypted = type.convertToJSValue(encrypted, platform);
      expect(decrypted).toEqual(original);
    });
  });
});

describe('wrapEmWithNativeQueryBlocking', () => {
  function makeMockEm() {
    return {
      nativeInsert: (..._args: unknown[]) => Promise.resolve(),
      nativeUpdate: (..._args: unknown[]) => Promise.resolve(0),
      nativeDelete: (..._args: unknown[]) => Promise.resolve(0),
      find: (..._args: unknown[]) => Promise.resolve([])
    };
  }

  it('blocks nativeInsert on entities with pii/phi/pci fields', () => {
    const mockEm = makeMockEm();
    const wrapped = wrapEmWithNativeQueryBlocking(mockEm);

    expect(() => wrapped.nativeInsert('Patient')).toThrow(/blocked/);
  });

  it('allows nativeInsert on entities without compliance fields', () => {
    const mockEm = makeMockEm();
    const wrapped = wrapEmWithNativeQueryBlocking(mockEm);

    expect(() => wrapped.nativeInsert('PublicEntity')).not.toThrow();
  });

  it('allows non-blocked methods', async () => {
    const mockEm = makeMockEm();
    const wrapped = wrapEmWithNativeQueryBlocking(mockEm);

    await expect(wrapped.find()).resolves.toEqual([]);
  });
});
