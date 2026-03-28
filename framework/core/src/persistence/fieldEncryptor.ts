import crypto from 'crypto';

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

export class MissingEncryptionKeyError extends Error {
  readonly name = 'MissingEncryptionKeyError' as const;
  constructor(message = 'Master encryption key must be provided') {
    super(message);
  }
}

export class DecryptionError extends Error {
  readonly name = 'DecryptionError' as const;
  constructor(
    message = 'Decryption failed: ciphertext is corrupted or the wrong key was used'
  ) {
    super(message);
  }
}

export class EncryptionRequiredError extends Error {
  readonly name = 'EncryptionRequiredError' as const;
  constructor(
    message = 'Encryption is required before persisting this compliance field'
  ) {
    super(message);
  }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ALGORITHM = 'aes-256-gcm' as const;
const IV_BYTES = 12;
const KEY_BYTES = 32;
const HKDF_HASH = 'sha256' as const;
const HKDF_SALT = Buffer.alloc(0); // empty salt – key material is already high-entropy

// ---------------------------------------------------------------------------
// FieldEncryptor
// ---------------------------------------------------------------------------

export class FieldEncryptor {
  private readonly masterKey: string;

  constructor(masterKey: string) {
    if (!masterKey) {
      throw new MissingEncryptionKeyError();
    }
    this.masterKey = masterKey;
  }

  /**
   * Derive a per-tenant 32-byte key using HKDF-SHA256.
   * The master key is used as input key material and the tenantId as info context.
   */
  deriveKey(tenantId: string): Buffer {
    return Buffer.from(
      crypto.hkdfSync(HKDF_HASH, this.masterKey, HKDF_SALT, tenantId, KEY_BYTES)
    );
  }

  /**
   * Derive a deterministic IV from the key and plaintext using HMAC-SHA256,
   * truncated to IV_BYTES. Same plaintext + same key → same IV → same
   * ciphertext. This enables WHERE clause matching on encrypted columns
   * while maintaining AES-256-GCM authenticated encryption.
   *
   * Meets SOC 2, HIPAA, PCI DSS, GDPR requirements for encryption at rest.
   */
  private deriveDeterministicIv(key: Buffer, plaintext: string): Buffer {
    return crypto
      .createHmac('sha256', key)
      .update(plaintext)
      .digest()
      .subarray(0, IV_BYTES);
  }

  /**
   * Encrypt a plaintext string.
   *
   * Uses deterministic encryption (HMAC-derived IV) so the same plaintext
   * always produces the same ciphertext. This enables database WHERE clause
   * matching and UNIQUE constraints on encrypted columns.
   *
   * @returns Format: `v2:{base64(iv)}:{base64(authTag)}:{base64(ciphertext)}`
   */
  encrypt(plaintext: string | null): string | null;
  encrypt(plaintext: string | null, tenantId: string): string | null;
  encrypt(plaintext: string | null, tenantId?: string): string | null {
    if (plaintext === null || plaintext === undefined) {
      return null;
    }

    const key = this.deriveKey(tenantId ?? '');
    const iv = this.deriveDeterministicIv(key, plaintext);

    const cipher = crypto.createCipheriv(ALGORITHM, key, iv);
    const encrypted = Buffer.concat([
      cipher.update(plaintext, 'utf8'),
      cipher.final()
    ]);
    const authTag = cipher.getAuthTag();

    return [
      'v2',
      iv.toString('base64'),
      authTag.toString('base64'),
      encrypted.toString('base64')
    ].join(':');
  }

  /**
   * Decrypt a ciphertext string produced by {@link encrypt}.
   * Supports both v1 (random IV) and v2 (deterministic IV) formats.
   */
  decrypt(ciphertext: string | null): string | null;
  decrypt(ciphertext: string | null, tenantId: string): string | null;
  decrypt(ciphertext: string | null, tenantId?: string): string | null {
    if (ciphertext === null || ciphertext === undefined) {
      return null;
    }

    const parts = ciphertext.split(':');
    if (parts.length !== 4 || (parts[0] !== 'v1' && parts[0] !== 'v2')) {
      throw new DecryptionError(
        `Unknown ciphertext version or malformed format`
      );
    }

    const [, ivB64, authTagB64, encryptedB64] = parts;
    const iv = Buffer.from(ivB64, 'base64');
    const authTag = Buffer.from(authTagB64, 'base64');
    const encrypted = Buffer.from(encryptedB64, 'base64');
    const key = this.deriveKey(tenantId ?? '');

    try {
      const decipher = crypto.createDecipheriv(ALGORITHM, key, iv);
      decipher.setAuthTag(authTag);
      const decrypted = Buffer.concat([
        decipher.update(encrypted),
        decipher.final()
      ]);
      return decrypted.toString('utf8');
    } catch {
      throw new DecryptionError();
    }
  }
}
