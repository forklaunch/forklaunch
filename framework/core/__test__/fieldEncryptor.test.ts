import { describe, expect, it } from 'vitest';
import {
  DecryptionError,
  FieldEncryptor,
  MissingEncryptionKeyError
} from '../src/encryption/fieldEncryptor';

const MASTER_KEY = 'test-master-key-that-is-long-enough-for-hkdf';

describe('FieldEncryptor', () => {
  it('should throw MissingEncryptionKeyError when master key is empty', () => {
    expect(() => new FieldEncryptor('')).toThrow(MissingEncryptionKeyError);
  });

  it('should encrypt/decrypt roundtrip and produce the original value', () => {
    const enc = new FieldEncryptor(MASTER_KEY);
    const plaintext = 'sensitive-data-123';
    const ciphertext = enc.encrypt(plaintext, 'tenant-a');
    const decrypted = enc.decrypt(ciphertext!, 'tenant-a');
    expect(decrypted).toBe(plaintext);
  });

  it('should produce different ciphertext for different tenants with the same plaintext', () => {
    const enc = new FieldEncryptor(MASTER_KEY);
    const plaintext = 'same-data';
    const ct1 = enc.encrypt(plaintext, 'tenant-a');
    const ct2 = enc.encrypt(plaintext, 'tenant-b');
    expect(ct1).not.toBe(ct2);
  });

  it('should produce different ciphertext for the same tenant and plaintext (random IV)', () => {
    const enc = new FieldEncryptor(MASTER_KEY);
    const plaintext = 'same-data';
    const ct1 = enc.encrypt(plaintext, 'tenant-a');
    const ct2 = enc.encrypt(plaintext, 'tenant-a');
    expect(ct1).not.toBe(ct2);
  });

  it('should throw DecryptionError when ciphertext is tampered with', () => {
    const enc = new FieldEncryptor(MASTER_KEY);
    const ciphertext = enc.encrypt('hello', 'tenant-a')!;
    const parts = ciphertext.split(':');
    // Corrupt the ciphertext portion
    parts[3] = Buffer.from('corrupted-data').toString('base64');
    const tampered = parts.join(':');
    expect(() => enc.decrypt(tampered, 'tenant-a')).toThrow(DecryptionError);
  });

  it('should throw DecryptionError when decrypting with the wrong tenant ID', () => {
    const enc = new FieldEncryptor(MASTER_KEY);
    const ciphertext = enc.encrypt('hello', 'tenant-a')!;
    expect(() => enc.decrypt(ciphertext, 'tenant-b')).toThrow(DecryptionError);
  });

  it('should encrypt/decrypt empty string roundtrip', () => {
    const enc = new FieldEncryptor(MASTER_KEY);
    const ciphertext = enc.encrypt('', 'tenant-a');
    const decrypted = enc.decrypt(ciphertext!, 'tenant-a');
    expect(decrypted).toBe('');
  });

  it('should return null when encrypting null', () => {
    const enc = new FieldEncryptor(MASTER_KEY);
    expect(enc.encrypt(null, 'tenant-a')).toBeNull();
  });

  it('should return null when decrypting null', () => {
    const enc = new FieldEncryptor(MASTER_KEY);
    expect(enc.decrypt(null, 'tenant-a')).toBeNull();
  });

  it('should produce ciphertext that starts with "v1:" prefix', () => {
    const enc = new FieldEncryptor(MASTER_KEY);
    const ciphertext = enc.encrypt('data', 'tenant-a')!;
    expect(ciphertext.startsWith('v1:')).toBe(true);
  });

  it('should throw DecryptionError for unknown version prefix', () => {
    const enc = new FieldEncryptor(MASTER_KEY);
    expect(() => enc.decrypt('v2:abc:def:ghi', 'tenant-a')).toThrow(
      DecryptionError
    );
  });

  it('should throw DecryptionError for malformed ciphertext', () => {
    const enc = new FieldEncryptor(MASTER_KEY);
    expect(() => enc.decrypt('not-valid-format', 'tenant-a')).toThrow(
      DecryptionError
    );
  });
});
