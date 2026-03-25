import type {
  EntityManager,
  EventArgs,
  EventSubscriber
} from '@mikro-orm/core';
import {
  DecryptionError,
  EncryptionRequiredError,
  FieldEncryptor
} from './fieldEncryptor';
import {
  type ComplianceLevel,
  getEntityComplianceFields
} from './complianceTypes';

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ENCRYPTED_PREFIX = 'v1:';

/**
 * Compliance levels that require field-level encryption.
 * PII is NOT encrypted (RDS encryption + TLS sufficient).
 */
const ENCRYPTED_LEVELS: ReadonlySet<ComplianceLevel> = new Set(['phi', 'pci']);

// ---------------------------------------------------------------------------
// ComplianceEventSubscriber
// ---------------------------------------------------------------------------

/**
 * MikroORM EventSubscriber that enforces field-level encryption for
 * compliance-classified fields (PHI and PCI).
 *
 * - **onBeforeCreate / onBeforeUpdate**: Encrypts PHI/PCI fields before
 *   database persistence. Throws `EncryptionRequiredError` if the encryption
 *   key is unavailable.
 * - **onLoad**: Decrypts PHI/PCI fields after loading from the database.
 *   Pre-migration plaintext (no `v1:` prefix) is returned as-is with a
 *   console warning to support rolling deployments.
 *
 * The tenant ID for key derivation is read from the EntityManager's filter
 * parameters (set by the tenant context middleware).
 */
export class ComplianceEventSubscriber implements EventSubscriber {
  private readonly encryptor: FieldEncryptor;

  constructor(encryptor: FieldEncryptor) {
    this.encryptor = encryptor;
  }

  async beforeCreate(args: EventArgs<unknown>): Promise<void> {
    this.encryptFields(args);
  }

  async beforeUpdate(args: EventArgs<unknown>): Promise<void> {
    this.encryptFields(args);
  }

  async onLoad(args: EventArgs<unknown>): Promise<void> {
    this.decryptFields(args);
  }

  // ---------------------------------------------------------------------------
  // Encrypt on persist
  // ---------------------------------------------------------------------------

  private encryptFields(args: EventArgs<unknown>): void {
    const entityName = args.meta.className;
    const complianceFields = getEntityComplianceFields(entityName);
    if (!complianceFields) return;

    const tenantId = this.getTenantId(args.em);
    const entity = args.entity as Record<string, unknown>;

    for (const [fieldName, level] of complianceFields) {
      if (!ENCRYPTED_LEVELS.has(level)) continue;

      const value = entity[fieldName];
      if (value === null || value === undefined) continue;
      if (typeof value !== 'string') continue;

      // Don't double-encrypt
      if (value.startsWith(ENCRYPTED_PREFIX)) continue;

      entity[fieldName] = this.encryptor.encrypt(value, tenantId);
    }
  }

  // ---------------------------------------------------------------------------
  // Decrypt on load
  // ---------------------------------------------------------------------------

  private decryptFields(args: EventArgs<unknown>): void {
    const entityName = args.meta.className;
    const complianceFields = getEntityComplianceFields(entityName);
    if (!complianceFields) return;

    const tenantId = this.getTenantId(args.em);
    const entity = args.entity as Record<string, unknown>;

    for (const [fieldName, level] of complianceFields) {
      if (!ENCRYPTED_LEVELS.has(level)) continue;

      const value = entity[fieldName];
      if (value === null || value === undefined) continue;
      if (typeof value !== 'string') continue;

      if (value.startsWith(ENCRYPTED_PREFIX)) {
        // Encrypted — decrypt it
        try {
          entity[fieldName] = this.encryptor.decrypt(value, tenantId);
        } catch (err) {
          if (err instanceof DecryptionError) {
            throw new DecryptionError(
              `Failed to decrypt ${entityName}.${fieldName}: ${err.message}`
            );
          }
          throw err;
        }
      } else {
        // Pre-migration plaintext — return as-is, log warning
        console.warn(
          `[compliance] ${entityName}.${fieldName} contains unencrypted ${level} data. ` +
            `Run encryption migration to encrypt existing data.`
        );
      }
    }
  }

  // ---------------------------------------------------------------------------
  // Tenant ID resolution
  // ---------------------------------------------------------------------------

  /**
   * Read the tenant ID from the EntityManager's filter parameters.
   * The tenant context middleware sets this when forking the EM per request.
   */
  private getTenantId(em: EntityManager): string {
    const filters = em.getFilterParams('tenant') as
      | { tenantId?: string }
      | undefined;
    const tenantId = filters?.tenantId;
    if (!tenantId) {
      throw new EncryptionRequiredError(
        'Cannot encrypt/decrypt without tenant context. ' +
          'Ensure the tenant filter is set on the EntityManager.'
      );
    }
    return tenantId;
  }
}

// ---------------------------------------------------------------------------
// Native query blocking
// ---------------------------------------------------------------------------

/**
 * Wraps an EntityManager to block `nativeInsert`, `nativeUpdate`, and
 * `nativeDelete` on entities that have PHI or PCI compliance fields.
 *
 * This prevents bypassing the ComplianceEventSubscriber's encryption
 * by using raw queries. Call this in the tenant context middleware when
 * creating the request-scoped EM.
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
          // No compliance fields requiring encryption — allow the native query
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
 * MikroORM accepts entity name strings, entity class references, or entity instances.
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
