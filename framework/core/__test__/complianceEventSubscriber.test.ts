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
      const jsonType = new EncryptedType('json');
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
