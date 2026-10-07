import { withEncryptionContext } from '@forklaunch/core/persistence';
import { AsyncLocalStorage } from 'node:async_hooks';

/**
 * Keeps Better Auth's database reads on the same encryption key the rest of the
 * app writes with.
 *
 * Why this is needed. `FieldEncryptor` does not use one key — it derives a key
 * per tenant with HKDF, using the tenant id as info context, and
 * `getCurrentTenantId()` returns `''` when nothing set one. This service wires
 * its own EntityManager through `wrapEmWithTenantContext` (see
 * `registrations.ts`), so application code reads and writes under the request's
 * tenant. Better Auth was handed the raw ORM, so its reads ran under no tenant
 * at all.
 *
 * For a single-tenant deployment both sides are `''` and nothing breaks. The
 * moment a tenant id is supplied, every encrypted column on `account` —
 * `password`, `accessToken`, `refreshToken`, `idToken` — is written under the
 * tenant key and read back under the empty one, which fails with:
 *
 *   Decryption failed: ciphertext is corrupted or the wrong key was used
 *
 * The failure surfaces as a 500 from sign-in or sign-up, and because Better
 * Auth handles its own errors it can do so without logging anything.
 *
 * Usage: wrap the Better Auth handler in `withEncryptionContextLatch`, call
 * `latchEncryptionContext(tenantId)` as soon as the request's tenant is known,
 * and pass `createEncryptionAwareOrm(orm)` to `betterAuthConfig`.
 */
type LatchCell = { context: string | null };

const latchStorage = new AsyncLocalStorage<LatchCell>();

/**
 * Runs `fn` in a fresh scope. Call once per Better Auth request.
 *
 * `AsyncLocalStorage.run` rather than `enterWith`: `enterWith` mutates the
 * current async resource's store and does not reliably survive the pg
 * connection pool, whose pooled connections are long-lived async resources
 * created at pool init. `run` binds a new resource that promise hooks
 * propagate forward.
 */
export function withEncryptionContextLatch<T>(fn: () => T): T {
  return latchStorage.run({ context: null }, fn);
}

/**
 * Records the tenant this request's encrypted columns belong to.
 *
 * First write wins, so a request stays self-consistent. A no-op outside a
 * scope, so callers never have to check.
 */
export function latchEncryptionContext(context: string): void {
  const cell = latchStorage.getStore();
  if (cell && cell.context === null) {
    cell.context = context;
  }
}

/**
 * The latched context, or `undefined` when there is none.
 *
 * `''` and `undefined` are different and must not be collapsed: `''` is the
 * real no-tenant key, while `undefined` means nothing has been decided and the
 * read should be left alone.
 */
export function getLatchedEncryptionContext(): string | undefined {
  return latchStorage.getStore()?.context ?? undefined;
}

/**
 * Reads that hydrate entities, and therefore decrypt. Writes and unit-of-work
 * methods stay on the untouched path.
 */
const HYDRATING_READ_METHODS = new Set([
  'findOne',
  'findOneOrFail',
  'find',
  'findAll',
  'findAndCount'
]);

/**
 * Wraps an ORM so Better Auth's hydrating reads run under the latched context.
 *
 * With nothing latched the behaviour is byte-for-byte what it was before, so
 * single-tenant deployments and background work are unaffected.
 */
export function createEncryptionAwareOrm<T extends { em: object }>(orm: T): T {
  // Constrained to what is actually used rather than to `MikroORM`. The
  // driver-specific ORMs (`PostgreSqlMikroORM` and friends) declare a readonly
  // `~entities`, which is not assignable to the base type — and requiring the
  // base type here would force every caller to widen and lose its driver.
  const emProxy = new Proxy(orm.em, {
    get(target, prop, receiver) {
      if (typeof prop === 'string' && HYDRATING_READ_METHODS.has(prop)) {
        const read = Reflect.get(target, prop, receiver) as (
          ...args: unknown[]
        ) => Promise<unknown>;

        return async (...args: unknown[]) => {
          const invoke = () => read.apply(target, args) as Promise<unknown>;
          const latched = getLatchedEncryptionContext();

          return latched === undefined
            ? invoke()
            : withEncryptionContext(latched, invoke);
        };
      }

      const value = Reflect.get(target, prop, receiver);
      return typeof value === 'function' ? value.bind(target) : value;
    }
  });

  return new Proxy(orm, {
    get(target, prop, receiver) {
      if (prop === 'em') return emProxy;

      const value = Reflect.get(target, prop, receiver);
      return typeof value === 'function' ? value.bind(target) : value;
    }
  }) as T;
}
