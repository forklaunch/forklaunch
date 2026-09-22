import { Type, type Platform, type TransformContext } from '@mikro-orm/core';
import { AsyncLocalStorage } from 'node:async_hooks';
import type { FieldEncryptor } from './fieldEncryptor';

const ENCRYPTED_PREFIXES = ['v1:', 'v2:', 'v3:'] as const;

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
 * The empty string was offered as a tenant id.
 *
 * `''` used to mean "the global tenant", which made it impossible to tell a
 * deliberate global row from a tenant that was never resolved — the two are
 * the same key. Rows written by mistake under `''` read back fine from any
 * other unbound path and fail only when a correctly-bound caller finally
 * touches them, arbitrarily far from the code that caused it.
 *
 * Global rows need a tenant id you can name and search for — a constant like
 * `'_internal'` that no organization can collide with. Pick one and pass it.
 */
export class EmptyTenantError extends Error {
  constructor(api: string) {
    super(
      `${api}: '' is not a tenant id. It used to mean "global", which made a ` +
        `deliberate global row indistinguishable from a tenant nobody resolved. ` +
        `Bind an explicit constant (for example '_internal') for rows that ` +
        `genuinely belong to no organization.`
    );
    this.name = 'EmptyTenantError';
  }
}

/**
 * An encrypted column was read or written with no tenant bound.
 *
 * This is the failure the empty tenant used to hide. An unbound write
 * encrypted under `''` and looked like it worked; the row then failed to
 * decrypt for the tenant that owned it, and the stack trace pointed at the
 * innocent reader. Failing here names the actual culprit: the write path
 * that never bound a tenant.
 *
 * Bind one with `wrapEmWithTenantContext(em, tenantId)` in the DI
 * `EntityManager` factory, or `withEncryptionContext(tenantId, fn)` around
 * the work. A raw manager — `orm.em.fork()`, or the `__em` off a wrapped
 * entity — escapes the proxy and arrives here unbound.
 */
export class UnboundTenantError extends Error {
  constructor(operation: string) {
    super(
      `Cannot ${operation} an encrypted column with no tenant bound. Wrap the ` +
        `work in withEncryptionContext(tenantId, fn), or take the entity ` +
        `manager from a factory that calls wrapEmWithTenantContext. A raw ` +
        `em.fork() (or wrap(entity).__em) escapes the tenant proxy.`
    );
    this.name = 'UnboundTenantError';
  }
}

/**
 * Reject `''` at every door that binds a tenant.
 *
 * Exported so the binding helpers in this package share one rule, and so an
 * application's own factory can apply the same one at its own boundary.
 */
export function assertBindableTenantId(
  tenantId: string,
  api: string
): asserts tenantId is string {
  if (tenantId === '') {
    throw new EmptyTenantError(api);
  }
}

/**
 * Set the encryption tenant ID for the current async context.
 *
 * IMPORTANT: this uses `AsyncLocalStorage.enterWith`, which mutates the
 * current async resource's store. It does NOT reliably propagate through
 * `pg` connection pool callback async resources — pooled connections are
 * long-lived async resources created at pool init, and resumption
 * callbacks run in the pool's resource, not the caller's. As a result,
 * MikroORM hydration of encrypted columns can read an empty / wrong
 * tenant id from `getCurrentTenantId()` even when this was called
 * correctly at the start of the request.
 *
 * Prefer `withEncryptionContext(tenantId, fn)` (or the
 * `wrapEmWithTenantContext` helper) for any code path that hits the
 * database. `setEncryptionTenantId` remains useful as a best-effort seed
 * for purely synchronous code paths.
 */
export function setEncryptionTenantId(tenantId: string): void {
  assertBindableTenantId(tenantId, 'setEncryptionTenantId');
  _tenantContext.enterWith({ tenantId });
}

/**
 * Run `fn` inside a fresh AsyncLocalStorage scope bound to `tenantId`.
 *
 * Unlike `setEncryptionTenantId`, this uses `AsyncLocalStorage.run`, which
 * creates a new async resource bound to the store. Node's promise hooks
 * propagate the store forward through pool callback async resources, so
 * MikroORM hydration of encrypted columns inside `fn` (even after multiple
 * awaits and a pg pool roundtrip) sees the correct tenant id.
 *
 * Most application code should not call this directly — use
 * `wrapEmWithTenantContext(em, tenantId)` in your DI EntityManager
 * factory and every method on the returned EM will be wrapped
 * automatically.
 */
export function withEncryptionContext<T>(tenantId: string, fn: () => T): T {
  assertBindableTenantId(tenantId, 'withEncryptionContext');
  return _tenantContext.run({ tenantId }, fn);
}

/**
 * Get the current tenant ID, or the empty string when none is bound.
 *
 * @deprecated The empty string it returns when nothing is bound is the whole
 * problem: "no tenant" and "the global tenant" became the same value, so an
 * unbound write encrypted under `''` and surfaced as somebody else's decrypt
 * failure. Encryption no longer reads through this — it uses the bound tenant
 * and throws {@link UnboundTenantError} when there is none. Use
 * {@link getBoundTenantId} and handle `undefined` explicitly.
 */
export function getCurrentTenantId(): string {
  return _tenantContext.getStore()?.tenantId ?? '';
}

/**
 * The tenant bound to the current async context, or `undefined` if none is.
 *
 * The distinction {@link getCurrentTenantId} cannot make: `undefined` is
 * "nobody bound a tenant", which is a bug on any path that touches an
 * encrypted column, and never a key.
 */
export function getBoundTenantId(): string | undefined {
  return _tenantContext.getStore()?.tenantId;
}

/** The bound tenant, or {@link UnboundTenantError} naming what was attempted. */
function requireBoundTenantId(operation: string): string {
  const tenantId = _tenantContext.getStore()?.tenantId;
  if (tenantId === undefined || tenantId === '') {
    throw new UnboundTenantError(operation);
  }
  return tenantId;
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
    // Also the WHERE-clause path: a query value is encrypted under the bound
    // tenant before it is compared. Unbound, that used to produce ciphertext
    // under `''` which simply matched nothing — a silent empty result set,
    // not an error.
    return (
      _encryptor.encrypt(serialized, requireBoundTenantId('write')) ??
      serialized
    );
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

    if (!_encryptor) {
      throw new Error(
        'EncryptedType: no encryptor registered but database contains encrypted value. ' +
          'Call registerEncryptor() at application bootstrap.'
      );
    }

    let decrypted: string | null;
    const tenantId = requireBoundTenantId('read');
    try {
      decrypted = _encryptor.decrypt(value, tenantId);
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
