import type { EntityManager } from '@mikro-orm/core';
import { assertBindableTenantId, withEncryptionContext } from './encryptedType';

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
 * # The empty tenant is not a tenant
 *
 * `''` is rejected: it throws `EmptyTenantError`. It used to mean "bind the
 * no-tenant key", which is how global rows (a billing plan, a trial, a
 * template) were encrypted — and it made a deliberate global row and a tenant
 * nobody resolved the same key. A path that forgot to bind wrote perfectly
 * readable rows under `''`, and the mistake surfaced later, somewhere else, as
 * the owning tenant's "Failed to decrypt encrypted column value".
 *
 * Global rows need an id you can name and search for. Pick a constant no
 * organization can collide with — the platform uses `'_internal'` — and bind
 * it like any other tenant.
 *
 * `undefined` still means "do not wrap": the EM is returned as is and inherits
 * whatever context is already bound. That is right only for a lookup that has
 * not resolved a tenant yet and reads no encrypted column; anything that
 * touches one now fails with `UnboundTenantError` rather than quietly using
 * the empty key.
 *
 * @param em        a freshly forked `EntityManager` from `orm.em.fork(...)`
 * @param tenantId  the org/tenant id to bind; `''` throws;
 *                  `undefined` skips wrapping entirely
 * @throws EmptyTenantError when `tenantId` is `''`
 */
export function wrapEmWithTenantContext(
  em: EntityManager,
  tenantId: string | undefined
): EntityManager {
  if (tenantId === undefined) {
    return em;
  }

  assertBindableTenantId(tenantId, 'wrapEmWithTenantContext');

  em.setFilterParams('tenant', { tenantId });

  // No `setEncryptionTenantId` here on purpose: `enterWith` mutates the
  // caller's async resource and leaks the tenant into everything that runs
  // on it afterwards. The Proxy binds the tenant for every EM call, which is
  // the only place hydration and flush read it.
  const proxy: EntityManager = new Proxy(em, {
    get(target, prop, receiver) {
      const value = Reflect.get(target, prop, receiver);
      if (typeof value !== 'function') {
        return value;
      }
      return function tenantBoundMethod(...args: unknown[]) {
        const result = withEncryptionContext(tenantId, () =>
          (value as (...a: unknown[]) => unknown).apply(target, args)
        );
        return rebind(result, target, proxy);
      };
    }
  }) as EntityManager;
  return proxy;
}

/**
 * Keep a chained call inside the tenant. `persist()`, `remove()` and the
 * other fluent methods return the entity manager itself, and they return
 * the *raw* one, so `em.persist(row).flush()` would run `flush` outside the
 * context and encrypt under whatever tenant the caller happened to be in.
 * The manager comes back as the proxy instead.
 *
 * A `fork()` is deliberately left alone: a fork is a new manager, and the
 * established idiom for stepping out of the tenant is exactly
 * `getSuperAdminContext(em).fork()` inside an explicit
 * `withEncryptionContext(other, …)`. Binding the fork would make that
 * explicit context lose to the tenant the parent was created with.
 */
function rebind(
  result: unknown,
  target: EntityManager,
  proxy: EntityManager
): unknown {
  return result === target ? proxy : result;
}
