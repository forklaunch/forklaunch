import type { EntityManager } from '@mikro-orm/core';
import { setEncryptionTenantId, withEncryptionContext } from './encryptedType';

/**
 * Wrap a tenant-scoped MikroORM `EntityManager` so that every operation on
 * it executes inside `withEncryptionContext(tenantId, …)`.
 *
 * # Why this exists
 *
 * `setEncryptionTenantId` (and other places that need to record the
 * current tenant for `EncryptedType` hydration) uses
 * `AsyncLocalStorage.enterWith`. `enterWith` mutates the *current* async
 * resource's store. That works for plain Promise chains, but it does NOT
 * propagate through `pg` connection pool callback async resources.
 *
 * Pooled connections are long-lived async resources created at pool init.
 * When MikroORM hydration runs in a connection's resolution callback,
 * `getCurrentTenantId()` reads whatever store was bound to that pool
 * resource at creation time (typically empty), not the request's value.
 * The result is intermittent decrypt failures on encrypted columns even
 * though the EM is "tenant-scoped" everywhere in app code, with no obvious
 * culprit at any single call site.
 *
 * `withEncryptionContext(tenantId, fn)` uses `als.run(...)` instead, which
 * creates a fresh async resource bound to the store. Node's promise hooks
 * then propagate the store forward through the pool callback boundary, so
 * `getCurrentTenantId()` returns the right value at hydration time
 * regardless of which pooled connection serviced the query.
 *
 * By proxying the EM, every `find / findOne / populate / flush / persist`
 * etc executes inside its own bound `als.run` callback. Single point of
 * truth for tenant scoping; call sites stay clean.
 *
 * # Usage
 *
 * Use this in your DI `EntityManager` factory:
 *
 * ```ts
 * EntityManager: {
 *   lifetime: Lifetime.Scoped,
 *   type: EntityManager,
 *   factory: (
 *     { Orm },
 *     context?: { entityManagerOptions?: ForkOptions; tenantId?: string }
 *   ) =>
 *     wrapEmWithTenantContext(
 *       Orm.em.fork(context?.entityManagerOptions),
 *       context?.tenantId
 *     )
 * }
 * ```
 *
 * If `tenantId` is `undefined` or empty, the original EM is returned
 * unwrapped — useful for super-admin / lookup paths that need an
 * unscoped query before they know the tenant.
 *
 * @param em        a freshly forked `EntityManager` from `orm.em.fork(...)`
 * @param tenantId  the org/tenant id to bind for filter params + ALS;
 *                  pass `undefined` to skip wrapping entirely
 */
export function wrapEmWithTenantContext(
  em: EntityManager,
  tenantId: string | undefined
): EntityManager {
  if (!tenantId) {
    return em;
  }

  // Apply MikroORM tenant filter (used for row-level org isolation in
  // queries) and seed AsyncLocalStorage as a best-effort for any
  // synchronous code paths that read the tenant before invoking an EM
  // method. The Proxy below is the load-bearing part for async / pg pool
  // paths.
  em.setFilterParams('tenant', { tenantId });
  setEncryptionTenantId(tenantId);

  return new Proxy(em, {
    get(target, prop, receiver) {
      const value = Reflect.get(target, prop, receiver);
      if (typeof value !== 'function') {
        return value;
      }
      return function tenantBoundMethod(...args: unknown[]) {
        return withEncryptionContext(tenantId, () =>
          (value as (...a: unknown[]) => unknown).apply(target, args)
        );
      };
    }
  }) as EntityManager;
}
