import type { EncryptedKeysOf } from './complianceTypes';

/**
 * Makes "this read would decrypt, and nothing has bound a context" a compile
 * error instead of a production 500.
 *
 * The failure it prevents, twice seen in production: an entity carries columns
 * encrypted under an ORGANISATION's key, and a lookup reads that row to
 * discover which organisation it belongs to. The context needed to decrypt is
 * exactly what the read is trying to find, so hydration cannot succeed and
 * fails with "ciphertext is corrupted or the wrong key was used". The remedy is
 * a partial select omitting those columns — easy to apply, easy to forget, and
 * invisible until a tenant id is in play.
 *
 * Two things had to be true for a type to catch it, and both now are: the
 * classification level is recorded in the type (see `EncryptedKeysOf`), and an
 * entity schema exposes its properties as `readonly properties: TProperties`.
 *
 * Adoption is per call site by design. Wrapping an EntityManager here narrows
 * only that reference, so a service can move over one file at a time rather
 * than the whole repository failing to compile at once.
 */

/** The property map behind an entity schema from `defineComplianceEntity`. */
export type PropertiesOfSchema<TSchema> = TSchema extends {
  readonly properties: infer TProperties;
}
  ? TProperties
  : never;

/** The encrypted-at-rest property names of an entity schema. */
export type EncryptedKeysOfSchema<TSchema> = EncryptedKeysOf<
  PropertiesOfSchema<TSchema>
>;

/** True when reading this schema in full would decrypt something. */
export type SchemaRequiresEncryptionContext<TSchema> = [
  EncryptedKeysOfSchema<TSchema>
] extends [never]
  ? false
  : true;

/** The property names of a schema that are safe to select without a context. */
export type ContextFreeKeysOfSchema<TSchema> = Exclude<
  keyof PropertiesOfSchema<TSchema>,
  EncryptedKeysOfSchema<TSchema>
>;

/**
 * Read options that provably avoid every encrypted column.
 *
 * `fields` is the same option MikroORM already takes; the constraint is only on
 * which names may appear in it.
 */
export type ContextFreeReadOptions<TSchema> = {
  fields: readonly ContextFreeKeysOfSchema<TSchema>[];
};

/**
 * Carried by a caller that has already bound an encryption context and so may
 * decrypt. It is a type-level acknowledgement, not a runtime flag: nothing can
 * observe an AsyncLocalStorage binding from a signature, so the honest design
 * is to make the claim explicit and greppable rather than to pretend to check
 * it.
 */
export type DecryptingReadOptions = {
  /**
   * Set only inside `withEncryptionContext(...)`, or where the EntityManager
   * came from `wrapEmWithTenantContext(em, tenantId)` with a real tenant.
   */
  encryptionContextIsBound: true;
};

/**
 * The options a read may pass, given what the schema carries.
 *
 * - nothing encrypted: options are unconstrained, exactly as before
 * - something encrypted: either select around it, or declare a bound context
 */
export type EncryptionAwareReadOptions<TSchema, TOptions> = [
  EncryptedKeysOfSchema<TSchema>
] extends [never]
  ? TOptions
  : TOptions & (ContextFreeReadOptions<TSchema> | DecryptingReadOptions);

/** Read methods that hydrate entities, and therefore decrypt. */
export type HydratingReadMethod =
  'findOne' | 'findOneOrFail' | 'find' | 'findAll' | 'findAndCount';

/**
 * An EntityManager whose hydrating reads carry the constraint above. Every
 * other member is passed through untouched, so this stays a drop-in narrowing
 * of an existing reference.
 */
export type EncryptionSafeEntityManager<TEntityManager> = Omit<
  TEntityManager,
  HydratingReadMethod
> & {
  [
    TMethod in HydratingReadMethod & keyof TEntityManager
  ]: TEntityManager[TMethod] extends (
    entity: infer TSchema,
    where: infer TWhere,
    options?: infer TOptions
  ) => infer TResult
    ? <TActualSchema extends TSchema>(
        entity: TActualSchema,
        where: TWhere,
        options: EncryptionAwareReadOptions<TActualSchema, TOptions>
      ) => TResult
    : TEntityManager[TMethod];
};

/**
 * Narrows an EntityManager so its hydrating reads must account for encrypted
 * columns. Purely a type-level change — the same object is returned, so there
 * is no proxy, no allocation and no behavioural difference at runtime.
 */
export function asEncryptionSafe<TEntityManager extends object>(
  em: TEntityManager
): EncryptionSafeEntityManager<TEntityManager> {
  return em as EncryptionSafeEntityManager<TEntityManager>;
}
