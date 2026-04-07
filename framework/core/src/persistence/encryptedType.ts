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
// Type resolution helpers
// ---------------------------------------------------------------------------

/**
 * Resolve a MikroORM type value (string, constructor, or instance) to a
 * Type instance. Returns undefined if unresolvable.
 */
export function resolveTypeInstance(
  type: unknown
): Type<unknown, unknown> | undefined {
  if (type instanceof Type) return type;
  if (typeof type === 'function') {
    try {
      const inst = new (type as new () => unknown)();
      return inst instanceof Type ? inst : undefined;
    } catch {
      return undefined;
    }
  }
  return undefined;
}

/**
 * Hydrate a raw value (after JSON.parse or string) to the JS type
 * indicated by `runtimeType`. This covers the cases where MikroORM's
 * own convertToJSValue is a NOOP (relies on the DB driver).
 */
function hydrateValue(value: unknown, runtimeType: string): unknown {
  if (value === null || value === undefined) return value;
  switch (runtimeType) {
    case 'string':
      return typeof value === 'string' ? value : String(value);
    case 'number':
      return typeof value === 'number' ? value : Number(value);
    case 'boolean':
      return typeof value === 'boolean'
        ? value
        : typeof value === 'string'
          ? value === 'true'
          : Boolean(value);
    case 'Date':
      return value instanceof Date ? value : new Date(value as string | number);
    case 'bigint':
      return typeof value === 'bigint' ? value : BigInt(value as string);
    case 'Buffer':
      return Buffer.isBuffer(value)
        ? value
        : typeof value === 'string'
          ? Buffer.from(value, 'base64')
          : Buffer.from(value as Uint8Array);
    case 'any': // json — return as-is after JSON.parse
    default:
      return value;
  }
}

// ---------------------------------------------------------------------------
// EncryptedType
// ---------------------------------------------------------------------------

/**
 * MikroORM custom Type that transparently encrypts/decrypts values.
 *
 * Works with any JS type. Non-string values are serialized before
 * encryption and hydrated after decryption using the original Type's
 * runtimeType for correct JS type reconstruction.
 *
 * DB column is always `text` — the encrypted ciphertext is a string
 * regardless of the original JS type.
 *
 * For array container types, the element type is tracked separately
 * so each element is hydrated individually after decryption.
 */
export class EncryptedType extends Type<unknown, string | null> {
  // NOTE: Using regular properties instead of #private fields to avoid
  // "Cannot read private member" errors when multiple copies of this class
  // are loaded (dual-package hazard with CJS/ESM or duplicate node_modules).
  readonly _elementRuntimeType: string;
  readonly _isArray: boolean;
  readonly _enumValues: unknown[] | undefined;

  /**
   * @param elementRuntimeType - The runtimeType of the (element) type,
   *   e.g. 'string', 'number', 'Date', 'bigint', 'Buffer', 'any'.
   * @param isArray - Whether this is an array container type.
   * @param enumValues - Optional list of allowed enum values for app-level
   *   validation (used when an enum field has encrypted compliance).
   */
  constructor(
    elementRuntimeType: string = 'string',
    isArray: boolean = false,
    enumValues?: unknown[]
  ) {
    super();
    this._elementRuntimeType = elementRuntimeType;
    this._isArray = isArray;
    this._enumValues = enumValues;
  }

  override convertToDatabaseValue(
    value: unknown,
    _platform: Platform,
    _context?: TransformContext
  ): string | null {
    if (value === null || value === undefined) return null;
    if (typeof value === 'string' && value.length === 0) return '';

    // Validate enum values at app level before encryption replaces them
    // with ciphertext that would never pass a DB check constraint.
    if (this._enumValues) {
      const valuesToCheck =
        this._isArray && Array.isArray(value) ? value : [value];
      for (const v of valuesToCheck) {
        if (!this._enumValues.includes(v)) {
          throw new Error(
            `Invalid enum value: ${String(v)}. Allowed values: ${this._enumValues.join(', ')}`
          );
        }
      }
    }

    if (!_encryptor) {
      return this.serialize(value);
    }

    if (typeof value === 'string' && isEncrypted(value)) {
      return value;
    }

    const serialized = this.serialize(value);
    return _encryptor.encrypt(serialized, getCurrentTenantId()) ?? serialized;
  }

  override convertToJSValue(
    value: string | null,
    _platform: Platform
  ): unknown {
    if (value === null || value === undefined) return null;
    if (typeof value !== 'string') return value;

    if (!isEncrypted(value)) {
      return this.deserialize(value);
    }

    if (!_encryptor) return value;

    // Decrypt failures must throw. Returning the ciphertext silently lets a
    // bad row hydrate as a malformed value (e.g. `new Date(ciphertext)` →
    // Invalid Date), which then sits in the identity map and crashes a later
    // unrelated `flush()` deep inside `serialize()` — far from the real cause.
    // Surface the failure at the read site instead.
    let decrypted: string | null;
    try {
      decrypted = _encryptor.decrypt(value, getCurrentTenantId());
    } catch (err) {
      throw new Error(
        `Failed to decrypt encrypted column value: ${err instanceof Error ? err.message : String(err)}`,
        { cause: err }
      );
    }
    if (decrypted === null) return null;
    return this.deserialize(decrypted);
  }

  override getColumnType(): string {
    return 'text';
  }

  override get runtimeType(): string {
    if (this._isArray) return 'object';
    return this._elementRuntimeType;
  }

  override ensureComparable(): boolean {
    return false;
  }

  // ---------------------------------------------------------------------------
  // Serialization helpers
  // ---------------------------------------------------------------------------

  private serialize(value: unknown): string {
    if (typeof value === 'string') return value;
    if (value instanceof Date) return value.toISOString();
    if (typeof value === 'bigint') return value.toString();
    if (Buffer.isBuffer(value) || value instanceof Uint8Array)
      return Buffer.from(value as Uint8Array).toString('base64');
    return JSON.stringify(value);
  }

  private deserialize(value: string): unknown {
    if (this._isArray) {
      try {
        const arr = JSON.parse(value);
        if (!Array.isArray(arr)) return arr;
        return arr.map((el: unknown) =>
          hydrateValue(el, this._elementRuntimeType)
        );
      } catch {
        return value;
      }
    }

    switch (this._elementRuntimeType) {
      case 'string':
        return value;
      case 'any':
        try {
          return JSON.parse(value);
        } catch {
          return value;
        }
      // Types serialized via toString/toISOString/base64 — hydrate from string directly
      case 'bigint':
      case 'Date':
      case 'Buffer':
        return hydrateValue(value, this._elementRuntimeType);
      // Types where JSON.parse recovers the native JS value (number, boolean)
      default:
        return hydrateValue(this.tryJsonParse(value), this._elementRuntimeType);
    }
  }

  private tryJsonParse(value: string): unknown {
    try {
      return JSON.parse(value);
    } catch {
      return value;
    }
  }
}
