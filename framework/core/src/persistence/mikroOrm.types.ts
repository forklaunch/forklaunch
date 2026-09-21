import type {
  Collection,
  IndexHints,
  MikroORM,
  Reference
} from '@mikro-orm/core';

/**
 * `MikroORM` with relaxed type parameters.
 *
 * MikroORM 7.1+ types the third generic as
 * `Entities extends readonly (...)[]` with a *mutable* array default, so
 * `MikroORM.init()` / `new MikroORM(defineConfig(...))` produce instances
 * whose readonly entities tuple is not assignable to a bare `MikroORM`
 * annotation. Use this alias for ORM-valued parameters, fields, and
 * variables that must accept any configured instance.
 *
 * Derived from `init`'s own return type rather than written as
 * `MikroORM<any, any, any>`. The `any`s said "accepts anything", which is
 * broader than the truth and switches off checking on every ORM value in the
 * codebase; deriving keeps the real instance type and follows MikroORM's
 * generics automatically when they change again.
 *
 * @example
 * ```typescript
 * import { AnyMikroORM } from '@forklaunch/core/persistence';
 *
 * function withOrm(orm: AnyMikroORM) {
 *   return orm.em.fork();
 * }
 * ```
 */
export type AnyMikroORM = Awaited<ReturnType<typeof MikroORM.init>>;

/**
 * The resolved, structural view of an inferred entity: its plain data fields
 * only, without mikro-orm's symbol-keyed metadata slots (`PrimaryKeyProp`,
 * `IndexHints`, ...). Those slots embed the raw property-builder record,
 * which is invariant — two identically-shaped entities defined in different
 * packages (or with different builder options such as `.unique()` or
 * `.index()`) will never unify on it. Cross-package entity constraints
 * should compare `ResolvedEntity<(typeof X)['~entity']>` so compatibility is
 * judged on the actual field types, which is what the consuming code reads
 * and writes.
 *
 * Relations are resolved the same way. A `Collection<Permission>`, a
 * `Reference<Organization>` or a bare `Organization` on the entity carries the
 * related entity's builder record too, and since mikro-orm 7.2 the builders
 * are declared `in out` (invariant), so a concrete `Role` whose `permissions`
 * point at the app's `Permission` would never satisfy a constraint written
 * against a module's minimal `Permission` unless the relation target is
 * stripped of its metadata slots as well.
 */
export type ResolvedEntity<T> = {
  [K in keyof T as K extends string ? K : never]: ResolvedRelation<T[K]>;
};

/**
 * `ResolvedEntity` applied through a relation wrapper: `Collection<E>` becomes
 * `Collection<ResolvedEntity<E>>`, references likewise, and a bare entity
 * object (a `manyToOne` without `.ref()`) resolves in place. Scalars, dates,
 * enums, `null`, `undefined` and `any` pass through untouched: only a value
 * that carries the `IndexHints` slot is an entity. (`any` is checked first
 * because `keyof any` contains every symbol, and a shape entity that declares
 * `fp.enum()` without naming the enum infers its value as `any`.)
 */
export type ResolvedRelation<V> = 0 extends 1 & V
  ? V
  : V extends Collection<infer E extends object, infer O extends object>
    ? Collection<ResolvedEntity<E>, O>
    : V extends Reference<infer E extends object>
      ? Reference<ResolvedEntity<E>>
      : typeof IndexHints extends keyof V
        ? ResolvedEntity<V>
        : V;
