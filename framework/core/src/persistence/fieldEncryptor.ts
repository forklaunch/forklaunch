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

/**
 * Ciphertext envelopes the encryptor reads. Only `v3` names its key.
 *
 *   v1:{iv}:{tag}:{data}            random IV (legacy)
 *   v2:{iv}:{tag}:{data}            deterministic IV
 *   v3:{keyId}:{iv}:{tag}:{data}    deterministic IV, stamped with the key fingerprint
 */
export const ENCRYPTED_PREFIXES = ['v1:', 'v2:', 'v3:'] as const;

/** True when the string carries one of the encryptor's envelopes. */
export function isEncryptedCiphertext(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    ENCRYPTED_PREFIXES.some((p) => value.startsWith(p))
  );
}

/**
 * Which envelope new values are written in.
 *
 *   'v2'  unstamped. Default. Byte-compatible with every existing row, so
 *         equality lookups keep matching rows written before the ring existed.
 *   'v3'  stamped with the writing key's fingerprint. Reads resolve the key
 *         directly, a missing key fails by name, and the rows still under a
 *         key can be counted with a LIKE. Switch to it only after a rotation
 *         sweep has rewritten every row: a v3 write does not byte-match a v2
 *         row of the same plaintext, so lookups miss until the sweep is done.
 */
export type CiphertextFormat = 'v2' | 'v3';

/** Environment variable holding the key new values are written with. */
export const ENCRYPTION_KEY_ENV = 'ENCRYPTION_KEY';
/** Environment variable holding every previous key, comma separated. */
export const LEGACY_ENCRYPTION_KEYS_ENV = 'LEGACY_ENCRYPTION_KEYS';
/** Older single-key spelling of {@link LEGACY_ENCRYPTION_KEYS_ENV}; still honoured. */
export const LEGACY_ENCRYPTION_KEY_ENV = 'LEGACY_ENCRYPTION_KEY';
/** Environment variable selecting the write format, `v2` (default) or `v3`. */
export const ENCRYPTION_FORMAT_ENV = 'ENCRYPTION_FORMAT';

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
 * A short, non-reversible fingerprint of a master key, for logs, reports
 * and the `v3` envelope. Two deployments with the same key get the same
 * id; the id reveals nothing about the key.
 */
export function encryptionKeyId(masterKey: string): string {
  return crypto
    .createHash('sha256')
    .update(masterKey)
    .digest('hex')
    .slice(0, KEY_ID_HEX_CHARS);
}

/**
 * The key fingerprint a stored value is stamped with, or null for `v1`/`v2`
 * values, which name no key.
 */
export function stampedKeyId(ciphertext: string): string | null {
  if (!ciphertext.startsWith('v3:')) return null;
  const parts = ciphertext.split(':');
  return parts.length === 5 ? parts[1] : null;
}

/** SQL LIKE pattern matching every value stamped with `keyId`. */
export function stampedPrefix(keyId: string): string {
  return `v3:${keyId}:`;
}

export interface FieldEncryptorOptions {
  /**
   * Keys that data may still be encrypted under, newest first. Values are
   * never written with these; they are only tried when the current key
   * cannot open a ciphertext. Keep a key here until a rotation sweep
   * (`reencryptEncryptedColumns`) reports nothing left under it.
   */
  previousKeys?: readonly string[];
  /** Envelope for new values. See {@link CiphertextFormat}. Default `v2`. */
  format?: CiphertextFormat;
}

/** What {@link FieldEncryptor.open} learned about a ciphertext. */
export interface OpenedCiphertext {
  plaintext: string;
  /** Fingerprint of the key that opened it; see {@link encryptionKeyId}. */
  keyId: string;
  /** True when the current key opened it. */
  current: boolean;
  /** Envelope the value was stored in. */
  version: 'v1' | 'v2' | 'v3';
  /**
   * True when a rewrite would change the stored bytes: the value is under
   * a previous key, or it is unstamped while the encryptor writes `v3`.
   */
  stale: boolean;
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
 * write uses the current key. A `v3` value names its key, so a read looks
 * it up directly and a missing key fails by name. A `v1`/`v2` value names
 * nothing, so a read tries the current key and then each previous key in
 * order; AES-GCM's authentication tag makes "this key opens it" a real
 * check rather than a guess. Either way the master key can change without
 * a downtime window: add the old key to `previousKeys`, deploy, run the
 * rotation sweep, then drop the old key.
 */
export class FieldEncryptor {
  private readonly ring: readonly KeyMaterial[];
  private readonly byId: ReadonlyMap<string, KeyMaterial>;
  private readonly derived = new Map<string, Buffer>();
  readonly format: CiphertextFormat;

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
    this.byId = new Map(ring.map((k) => [k.id, k]));
    this.format = options.format ?? 'v2';
  }

  /**
   * Build the ring from the environment: `ENCRYPTION_KEY` plus
   * `LEGACY_ENCRYPTION_KEYS` (comma separated) and the older
   * `LEGACY_ENCRYPTION_KEY`; `ENCRYPTION_FORMAT` selects the envelope.
   */
  static fromEnv(env: NodeJS.ProcessEnv = process.env): FieldEncryptor {
    const masterKey = env[ENCRYPTION_KEY_ENV];
    if (!masterKey) {
      throw new MissingEncryptionKeyError(`${ENCRYPTION_KEY_ENV} must be set`);
    }
    const format = env[ENCRYPTION_FORMAT_ENV]?.trim().toLowerCase();
    if (format && format !== 'v2' && format !== 'v3') {
      throw new Error(
        `${ENCRYPTION_FORMAT_ENV} must be "v2" or "v3", got "${format}"`
      );
    }
    return new FieldEncryptor(masterKey, {
      previousKeys: parseEncryptionKeyList(
        env[LEGACY_ENCRYPTION_KEYS_ENV],
        env[LEGACY_ENCRYPTION_KEY_ENV]
      ),
      format: (format as CiphertextFormat | undefined) ?? 'v2'
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
    return new FieldEncryptor(this.ring[0].masterKey, {
      previousKeys,
      format: this.format
    });
  }

  /** A copy of this encryptor writing a different envelope. */
  withFormat(format: CiphertextFormat): FieldEncryptor {
    return new FieldEncryptor(this.ring[0].masterKey, {
      previousKeys: this.ring.slice(1).map((k) => k.masterKey),
      format
    });
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
   * @returns `v2:{iv}:{tag}:{data}`, or `v3:{keyId}:{iv}:{tag}:{data}` when
   *   the encryptor's format is `v3`. All parts base64.
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

    const body = [
      iv.toString('base64'),
      authTag.toString('base64'),
      encrypted.toString('base64')
    ];
    return this.format === 'v3'
      ? ['v3', this.ring[0].id, ...body].join(':')
      : ['v2', ...body].join(':');
  }

  /**
   * Decrypt a ciphertext string produced by {@link encrypt}, in any
   * envelope: `v3` by its stamped key, `v1`/`v2` by trying every key in
   * the ring.
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
   * Decrypt and report which key opened the value and whether a rewrite
   * would change it. Throws {@link DecryptionError} when no key in the
   * ring does.
   */
  open(ciphertext: string, tenantId = ''): OpenedCiphertext {
    const parts = ciphertext.split(':');
    const version = parts[0];
    let candidates: readonly KeyMaterial[];
    let body: string[];

    if (version === 'v3' && parts.length === 5) {
      const key = this.byId.get(parts[1]);
      if (!key) {
        throw new DecryptionError(
          `Decryption failed: value is stamped with key ${parts[1]}, which is not in the ring (current ${this.ring[0].id}; previous ${
            this.ring
              .slice(1)
              .map((k) => k.id)
              .join(', ') || 'none'
          }). Add it to ${LEGACY_ENCRYPTION_KEYS_ENV}.`
        );
      }
      candidates = [key];
      body = parts.slice(2);
    } else if ((version === 'v1' || version === 'v2') && parts.length === 4) {
      candidates = this.ring;
      body = parts.slice(1);
    } else {
      throw new DecryptionError(
        `Unknown ciphertext version or malformed format`
      );
    }

    const [ivB64, authTagB64, encryptedB64] = body;
    const iv = Buffer.from(ivB64, 'base64');
    const authTag = Buffer.from(authTagB64, 'base64');
    const encrypted = Buffer.from(encryptedB64, 'base64');

    for (const key of candidates) {
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
        const current = key === this.ring[0];
        return {
          plaintext: decrypted.toString('utf8'),
          keyId: key.id,
          current,
          version: version as 'v1' | 'v2' | 'v3',
          stale: !current || (this.format === 'v3' && version !== 'v3')
        };
      } catch {
        // wrong key (or wrong tenant) for this value; try the next candidate
      }
    }
    throw new DecryptionError(
      candidates.length > 1
        ? `Decryption failed: none of the ${candidates.length} keys in the ring opens this value (ciphertext corrupted, wrong tenant, or the key it was written with is not in ${LEGACY_ENCRYPTION_KEYS_ENV})`
        : `Decryption failed: key ${candidates[0].id} does not open this value (ciphertext corrupted or wrong tenant)`
    );
  }

  /**
   * True when a rewrite would change the stored bytes: the value is under
   * a previous key, or unstamped while this encryptor writes `v3`. Throws
   * when no key opens it.
   */
  needsRotation(ciphertext: string, tenantId = ''): boolean {
    return this.open(ciphertext, tenantId).stale;
  }

  /**
   * Rewrite a value under the current key in the current format. Returns
   * the input unchanged when nothing would change, so callers can compare
   * to detect writes.
   */
  rotate(ciphertext: string | null, tenantId = ''): string | null {
    if (ciphertext === null || ciphertext === undefined) return null;
    const opened = this.open(ciphertext, tenantId);
    if (!opened.stale) return ciphertext;
    return this.encrypt(opened.plaintext, tenantId);
  }
}
