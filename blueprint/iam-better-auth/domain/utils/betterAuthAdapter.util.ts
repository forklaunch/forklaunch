import { mikroOrmAdapter } from '@forklaunch/better-auth-mikro-orm-fork';
import { isCompliantField } from '@forklaunch/core/persistence';

/** Better Auth's trusted adapter consumes plaintext hashes, provider tokens and
 * private JWKs. Keep fields wrapped everywhere else; reveal only adapter results
 * inside the request's encryption context so each read remains audited. */
function reveal(value: unknown): unknown {
  if (isCompliantField(value)) return value.deanon;
  if (Array.isArray(value)) return value.map(reveal);
  if (value === null || typeof value !== 'object') return value;
  // Dates and other typed values retain their runtime identity. Adapter records
  // can originate in another realm, so instanceof Object is insufficient.
  const prototype = Object.getPrototypeOf(value);
  if (prototype !== null && Object.getPrototypeOf(prototype) !== null)
    return value;
  return Object.fromEntries(
    Object.entries(value).map(([key, entry]) => [key, reveal(entry)])
  );
}

function revealAdapter<T extends object>(adapter: T): T {
  return new Proxy(adapter, {
    get(target, property, receiver) {
      const method: unknown = Reflect.get(target, property, receiver);
      if (typeof method !== 'function') return method;
      if (property === 'transaction') {
        return (callback: (transaction: object) => Promise<unknown>) =>
          Reflect.apply(method, target, [
            (transaction: object) => callback(revealAdapter(transaction))
          ]);
      }
      return async (...args: unknown[]) =>
        reveal(await Reflect.apply(method, target, args));
    }
  });
}

export const betterAuthMikroOrmAdapter: typeof mikroOrmAdapter = (...args) => {
  const factory = mikroOrmAdapter(...args);
  return (options) => revealAdapter(factory(options));
};
