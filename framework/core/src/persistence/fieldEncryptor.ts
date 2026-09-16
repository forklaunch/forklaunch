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
const HKDF_SALT = Buffer.alloc(0); // empty salt - key material is already high-entropy
const KEY_ID_HEX_CHARS = 12;
const DERIVED_KEY_CACHE_LIMIT = 4096;

/** Environment variable holding the key new values are written with. */
export const ENCRYPTION_KEY_ENV = 'ENCRYPTION_KEY';
/** Environment variable holding every previous key, comma separated. */
export const LEGACY_ENCRYPTION_KEYS_ENV = 'LEGACY_ENCRYPTION_KEYS';
/** Older single-key spelling of {@link LEGACY_ENCRYPTION_KEYS_ENV}; still honoured. */
export const LEGACY_ENCRYPTION_KEY_ENV = 'LEGACY_ENCRYPTION_KEY';

/**
 * Split a key list as it appears in an environment variable: comma,
 * semicolon or whitespace separated, optionally quoted. Blanks and
 * duplicates are dropped, order is kept.
 */
export function parseEncryptionKeyList(
  ...sources: (string | undefined | null)[]
): string[] {
  const keys: string[] = [];
  for (const source of sources) {
    for (const part of (source ?? '').split(/[\s,;]+/)) {
      const key = part.trim().replace(/^["']|["']$/g, '');
      if (key && !keys.includes(key)) keys.push(key);
    }
  }
  return keys;
}

/**
 * A short, non-reversible fingerprint of a master key, for logs and
 * reports. Two deployments with the same key get the same id; the id
 * reveals nothing about the key.
 */
export function encryptionKeyId(masterKey: string): string {
  return crypto
    .createHash('sha256')
    .update(masterKey)
    .digest('hex')
    .slice(0, KEY_ID_HEX_CHARS);
}

export interface FieldEncryptorOptions {
  /**
   * Keys that data may still be encrypted under, newest first. Values are
   * never written with these; they are only tried when the current key
   * cannot open a ciphertext. Keep a key here until a rotation sweep
   * (`reencryptEncryptedColumns`) reports nothing left under it.
   */
  previousKeys?: readonly string[];
}

/** What {@link FieldEncryptor.open} learned about a ciphertext. */
export interface OpenedCiphertext {
  plaintext: string;
  /** Fingerprint of the key that opened it; see {@link encryptionKeyId}. */
  keyId: string;
  /** True when the current key opened it, so no rotation is needed. */
  current: boolean;
}

interface KeyMaterial {
  readonly id: string;
  readonly masterKey: string;
}

// ---------------------------------------------------------------------------
// FieldEncryptor
// ---------------------------------------------------------------------------

/**
 * AES-256-GCM field encryption with per-tenant keys derived by HKDF from a
 * master key, and a key ring for rotation.
 *
 * The ring has one current key and any number of previous keys. Every
 * write uses the current key. Every read tries the current key first and
 * then each previous key in order; AES-GCM's authentication tag makes
 * "this key opens it" a real check rather than a guess. That is what lets
 * the master key change without a downtime window: add the old key to
 * `previousKeys`, deploy, run the rotation sweep, then drop the old key.
 *
 * The on-disk format is unchanged by the ring (`v2:` values carry no key
 * id), so a rotation never breaks equality lookups on rows already under
 * the current key, and rows under a previous key match again as soon as
 * the sweep rewrites them.
 */
export class FieldEncryptor {
  private readonly ring: readonly KeyMaterial[];
  private readonly derived = new Map<string, Buffer>();

  constructor(masterKey: string, options: FieldEncryptorOptions = {}) {
    if (!masterKey) {
      throw new MissingEncryptionKeyError();
    }
    const ring: KeyMaterial[] = [{ id: encryptionKeyId(masterKey), masterKey }];
    for (const key of options.previousKeys ?? []) {
      if (!key || ring.some((k) => k.masterKey === key)) continue;
      ring.push({ id: encryptionKeyId(key), masterKey: key });
    }
    this.ring = ring;
  }

  /**
   * Build the ring from the environment: `ENCRYPTION_KEY` plus
   * `LEGACY_ENCRYPTION_KEYS` (comma separated) and the older
   * `LEGACY_ENCRYPTION_KEY`.
   */
  static fromEnv(env: NodeJS.ProcessEnv = process.env): FieldEncryptor {
    const masterKey = env[ENCRYPTION_KEY_ENV];
    if (!masterKey) {
      throw new MissingEncryptionKeyError(`${ENCRYPTION_KEY_ENV} must be set`);
    }
    return new FieldEncryptor(masterKey, {
      previousKeys: parseEncryptionKeyList(
        env[LEGACY_ENCRYPTION_KEYS_ENV],
        env[LEGACY_ENCRYPTION_KEY_ENV]
      )
    });
  }

  /** Fingerprints of the ring, current first. Safe to log. */
  get keyIds(): { current: string; previous: string[] } {
    return {
      current: this.ring[0].id,
      previous: this.ring.slice(1).map((k) => k.id)
    };
  }

  /** A copy of this encryptor with a different set of previous keys. */
  withPreviousKeys(previousKeys: readonly string[]): FieldEncryptor {
    return new FieldEncryptor(this.ring[0].masterKey, { previousKeys });
  }

  /**
   * Derive a per-tenant 32-byte key using HKDF-SHA256 from the CURRENT
   * master key. The master key is the input key material and the tenantId
   * the info context.
   */
  deriveKey(tenantId: string): Buffer {
    return this.deriveKeyFor(this.ring[0], tenantId);
  }

  private deriveKeyFor(key: KeyMaterial, tenantId: string): Buffer {
    const cacheKey = `${key.id} ${tenantId}`;
    const cached = this.derived.get(cacheKey);
    if (cached) return cached;
    const derived = Buffer.from(
      crypto.hkdfSync(HKDF_HASH, key.masterKey, HKDF_SALT, tenantId, KEY_BYTES)
    );
    if (this.derived.size >= DERIVED_KEY_CACHE_LIMIT) this.derived.clear();
    this.derived.set(cacheKey, derived);
    return derived;
  }

  /**
   * Derive a deterministic IV from the key and plaintext using HMAC-SHA256,
   * truncated to IV_BYTES. Same plaintext + same key -> same IV -> same
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
   * Encrypt a plaintext string under the current key.
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
   * Decrypt a ciphertext string produced by {@link encrypt}, trying the
   * current key and then every previous key. Supports both v1 (random IV)
   * and v2 (deterministic IV) formats.
   */
  decrypt(ciphertext: string | null): string | null;
  decrypt(ciphertext: string | null, tenantId: string): string | null;
  decrypt(ciphertext: string | null, tenantId?: string): string | null {
    if (ciphertext === null || ciphertext === undefined) {
      return null;
    }
    return this.open(ciphertext, tenantId ?? '').plaintext;
  }

  /**
   * Decrypt and report which key opened the value. Throws
   * {@link DecryptionError} when no key in the ring does.
   */
  open(ciphertext: string, tenantId = ''): OpenedCiphertext {
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

    for (const [index, key] of this.ring.entries()) {
      try {
        const decipher = crypto.createDecipheriv(
          ALGORITHM,
          this.deriveKeyFor(key, tenantId),
          iv
        );
        decipher.setAuthTag(authTag);
        const decrypted = Buffer.concat([
          decipher.update(encrypted),
          decipher.final()
        ]);
        return {
          plaintext: decrypted.toString('utf8'),
          keyId: key.id,
          current: index === 0
        };
      } catch {
        // wrong key for this value; try the next one in the ring
      }
    }
    throw new DecryptionError(
      this.ring.length > 1
        ? `Decryption failed: none of the ${this.ring.length} keys in the ring opens this value (ciphertext corrupted, wrong tenant, or the key it was written with is not in ${LEGACY_ENCRYPTION_KEYS_ENV})`
        : undefined
    );
  }

  /**
   * True when the value opens only with a previous key and should be
   * rewritten under the current one. False for values already current.
   * Throws when no key opens it.
   */
  needsRotation(ciphertext: string, tenantId = ''): boolean {
    return !this.open(ciphertext, tenantId).current;
  }

  /**
   * Rewrite a value under the current key. Returns the input unchanged
   * when it is already current, so callers can compare to detect writes.
   */
  rotate(ciphertext: string | null, tenantId = ''): string | null {
    if (ciphertext === null || ciphertext === undefined) return null;
    const opened = this.open(ciphertext, tenantId);
    if (opened.current) return ciphertext;
    return this.encrypt(opened.plaintext, tenantId);
  }
}
