import { AsyncLocalStorage } from 'node:async_hooks';
import { Type, type Platform, type TransformContext } from '@mikro-orm/core';
import type { FieldEncryptor } from './fieldEncryptor';

const ENCRYPTED_PREFIXES = ['v1:', 'v2:'] as const;

function isEncrypted(value: string): boolean {
  return ENCRYPTED_PREFIXES.some((p) => value.startsWith(p));
}

// ---------------------------------------------------------------------------
// Module-level state
// ---------------------------------------------------------------------------

let _encryptor: FieldEncryptor | undefined;

/**
 * AsyncLocalStorage for the current tenant ID. This is set automatically
 * by `withEncryptionContext()` — users don't interact with it directly.
 */
const _tenantContext = new AsyncLocalStorage<{ tenantId: string }>();

/**
 * Register the FieldEncryptor instance for use by EncryptedType.
 * Call this once at application bootstrap (e.g., in mikro-orm.config.ts).
 */
export function registerEncryptor(encryptor: FieldEncryptor): void {
  _encryptor = encryptor;
}

/**
 * Run a callback with the given tenant ID available to EncryptedType
 * for per-tenant key derivation.
 */
export function withEncryptionContext<T>(tenantId: string, fn: () => T): T {
  return _tenantContext.run({ tenantId }, fn);
}

/**
 * Set the encryption tenant ID for the current async context.
 * Called automatically by the tenant context middleware — users
 * don't need to call this directly.
 */
export function setEncryptionTenantId(tenantId: string): void {
  _tenantContext.enterWith({ tenantId });
}

/**
 * Get the current tenant ID. Returns empty string when no context is set
 * (startup, seeders, better-auth, background jobs).
 */
export function getCurrentTenantId(): string {
  return _tenantContext.getStore()?.tenantId ?? '';
}

// ---------------------------------------------------------------------------
// EncryptedType
// ---------------------------------------------------------------------------

/**
 * MikroORM custom Type that transparently encrypts/decrypts values.
 *
 * Works with any JS type (string, number, boolean, object/JSON).
 * Non-string values are JSON-serialized before encryption and
 * JSON-parsed after decryption.
 *
 * DB column is always `text` — the encrypted ciphertext is a string
 * regardless of the original JS type.
 *
 * Operates at the data conversion layer — before the identity map,
 * during hydration, and during persistence. Entities always hold
 * their original JS values (plaintext).
 */
export class EncryptedType extends Type<unknown, string | null> {
  private readonly originalType: string;

  /**
   * @param originalType - The original JS type hint ('string' | 'json' | 'number' | 'boolean').
   *                       Used to determine serialization strategy.
   */
  constructor(originalType: string = 'string') {
    super();
    this.originalType = originalType;
  }

  override convertToDatabaseValue(
    value: unknown,
    _platform: Platform,
    _context?: TransformContext
  ): string | null {
    if (value === null || value === undefined) return null;
    if (typeof value === 'string' && value.length === 0) return '';

    if (!_encryptor) {
      return this.serializeValue(value);
    }

    if (typeof value === 'string' && isEncrypted(value)) {
      return value;
    }

    const serialized = this.serializeValue(value);
    return _encryptor.encrypt(serialized, getCurrentTenantId()) ?? serialized;
  }

  override convertToJSValue(
    value: string | null,
    _platform: Platform
  ): unknown {
    if (value === null || value === undefined) return null;
    if (typeof value !== 'string') return value;

    if (!isEncrypted(value)) {
      return this.deserializeValue(value);
    }

    if (!_encryptor) {
      throw new Error(
        'EncryptedType: no encryptor registered but database contains encrypted value. ' +
          'Call registerEncryptor() at application bootstrap.'
      );
    }

    const tenantId = getCurrentTenantId();
    const decrypted = _encryptor.decrypt(value, tenantId);
    if (decrypted === null) return null;
    return this.deserializeValue(decrypted);
  }

  override getColumnType(): string {
    return 'text';
  }

  override get runtimeType(): string {
    return this.originalType === 'json' ? 'object' : this.originalType;
  }

  override ensureComparable(): boolean {
    // Return false so MikroORM does NOT run convertToDatabaseValue on raw
    // DB data during entity hydration. With deterministic encryption, the
    // same plaintext always produces the same ciphertext, so raw DB values
    // can be compared directly for change detection.
    return false;
  }

  // ---------------------------------------------------------------------------
  // Serialization helpers
  // ---------------------------------------------------------------------------

  private serializeValue(value: unknown): string {
    if (typeof value === 'string') return value;
    return JSON.stringify(value);
  }

  private deserializeValue(value: string): unknown {
    switch (this.originalType) {
      case 'string':
        return value;
      case 'number':
        return Number(value);
      case 'boolean':
        return value === 'true';
      case 'json':
      default:
        try {
          return JSON.parse(value);
        } catch {
          return value;
        }
    }
  }
}
