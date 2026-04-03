import { EncryptionRequiredError } from './fieldEncryptor';
import {
  type ComplianceLevel,
  getEntityComplianceFields
} from './complianceTypes';

/**
 * Compliance levels that require field-level encryption.
 */
const ENCRYPTED_LEVELS: ReadonlySet<ComplianceLevel> = new Set([
  'pii',
  'phi',
  'pci'
]);

// ---------------------------------------------------------------------------
// Native query blocking
// ---------------------------------------------------------------------------

/**
 * Wraps an EntityManager to block `nativeInsert`, `nativeUpdate`, and
 * `nativeDelete` on entities that have PII, PHI, or PCI compliance fields.
 *
 * This prevents bypassing the EncryptedType's encryption by using raw
 * queries. Call this in the tenant context middleware when creating the
 * request-scoped EM.
 *
 * @returns A Proxy-wrapped EntityManager that throws on native query
 *          operations targeting compliance entities.
 */
export function wrapEmWithNativeQueryBlocking<T extends object>(em: T): T {
  const BLOCKED_METHODS = [
    'nativeInsert',
    'nativeUpdate',
    'nativeDelete'
  ] as const;

  return new Proxy(em, {
    get(target, prop, receiver) {
      if (
        typeof prop === 'string' &&
        BLOCKED_METHODS.includes(prop as (typeof BLOCKED_METHODS)[number])
      ) {
        return (entityNameOrEntity: unknown, ...rest: unknown[]) => {
          const entityName = resolveEntityName(entityNameOrEntity);
          if (entityName) {
            const fields = getEntityComplianceFields(entityName);
            if (fields) {
              for (const [fieldName, level] of fields) {
                if (ENCRYPTED_LEVELS.has(level)) {
                  throw new EncryptionRequiredError(
                    `${prop}() blocked on entity '${entityName}' because field ` +
                      `'${fieldName}' has compliance level '${level}'. ` +
                      `Use em.create() + em.flush() instead to ensure encryption.`
                  );
                }
              }
            }
          }
          const method = Reflect.get(target, prop, receiver);
          return (method as (...args: unknown[]) => unknown).call(
            target,
            entityNameOrEntity,
            ...rest
          );
        };
      }
      return Reflect.get(target, prop, receiver);
    }
  });
}

/**
 * Resolve an entity name from the first argument to nativeInsert/Update/Delete.
 */
function resolveEntityName(entityNameOrEntity: unknown): string | undefined {
  if (typeof entityNameOrEntity === 'string') {
    return entityNameOrEntity;
  }
  if (typeof entityNameOrEntity === 'function') {
    return (entityNameOrEntity as { name?: string }).name;
  }
  if (
    entityNameOrEntity != null &&
    typeof entityNameOrEntity === 'object' &&
    'constructor' in entityNameOrEntity
  ) {
    return (entityNameOrEntity.constructor as { name?: string }).name;
  }
  return undefined;
}
