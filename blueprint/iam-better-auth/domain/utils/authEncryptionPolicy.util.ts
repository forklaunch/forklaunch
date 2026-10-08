import {
  getCurrentTenantId,
  withEncryptionContext
} from '@forklaunch/core/persistence';

/** Fresh scaffolds only. Existing ciphertext requires an explicit, reviewed migration. */
export const AUTH_ENCRYPTION_POLICY = 'iam-service-v1' as const;
/** Accounts are global identities across organizations; signing keys also belong to this IAM service. */
export const AUTH_ENCRYPTION_CONTEXT = 'iam:identity-service:v1';
export function withAuthEncryptionPolicy<T>(fn: () => T): T {
  return withEncryptionContext(AUTH_ENCRYPTION_CONTEXT, fn);
}
type AnyEm = Record<string, (...args: any[]) => any>;
/** Mark new auth ciphertext and reject legacy rows. Business-data tenant binding is unchanged. */
export function createAuthEncryptionOrm<
  T extends { em: object; getMetadata: (...args: any[]) => any }
>(orm: T): T {
  const em = orm.em as AnyEm;
  const protectedModel = (entity: unknown) =>
    ['account', 'jwks'].includes(
      String(orm.getMetadata().find(entity)?.className ?? '').toLowerCase()
    );
  const assertScope = () => {
    if (getCurrentTenantId() !== AUTH_ENCRYPTION_CONTEXT)
      throw new Error(
        'IAM service encryption policy scope is missing or incompatible.'
      );
  };
  const assertRows = async (
    entity: unknown,
    criteria: unknown,
    options?: Record<string, unknown>
  ) => {
    assertScope();
    // Inspect only plaintext metadata before any secret is hydrated. No legacy key guessing.
    const rows = await em
      .fork()
      .find(entity, criteria ?? {}, {
        ...options,
        fields: ['id', 'encryptionPolicy'],
        limit: 1001
      });
    if (rows.length > 1000)
      throw new Error('Auth encryption policy check exceeds its row limit.');
    for (const row of rows)
      if (row.encryptionPolicy !== AUTH_ENCRYPTION_POLICY)
        throw new Error(
          'Legacy auth encryption policy requires an explicit ciphertext migration.'
        );
  };
  const reads = new Set([
    'findOne',
    'findOneOrFail',
    'find',
    'findAll',
    'findAndCount',
    'nativeUpdate'
  ]);
  const proxy = new Proxy(em, {
    get(target, property) {
      const key = String(property),
        method = target[key];
      if (key === 'create')
        return (entity: unknown, data: unknown, ...rest: unknown[]) => {
          if (!protectedModel(entity))
            return method.call(target, entity, data, ...rest);
          assertScope();
          return method.call(
            target,
            entity,
            { ...(data as object), encryptionPolicy: AUTH_ENCRYPTION_POLICY },
            ...rest
          );
        };
      if (reads.has(key))
        return async (
          entity: unknown,
          criteria: unknown,
          ...rest: unknown[]
        ) => {
          if (protectedModel(entity)) {
            if (
              key === 'nativeUpdate' &&
              rest[0] &&
              typeof rest[0] === 'object' &&
              'encryptionPolicy' in rest[0]
            )
              throw new Error(
                'Changing an auth encryption policy requires migration.'
              );
            await assertRows(entity, key === 'findAll' ? {} : criteria);
          }
          return method.call(target, entity, criteria, ...rest);
        };
      if (key === 'assign')
        return (entity: object, data: unknown, ...rest: unknown[]) => {
          if (
            protectedModel(entity.constructor) &&
            data &&
            typeof data === 'object' &&
            'encryptionPolicy' in data
          )
            throw new Error(
              'Changing an auth encryption policy requires migration.'
            );
          return method.call(target, entity, data, ...rest);
        };
      return typeof method === 'function' ? method.bind(target) : method;
    }
  });
  return new Proxy(orm, {
    get(target, property, receiver) {
      if (property === 'em') return proxy;
      const value = Reflect.get(target, property, receiver);
      return typeof value === 'function' ? value.bind(target) : value;
    }
  });
}
