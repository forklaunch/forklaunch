import {
  getEntityComplianceFields,
  type ComplianceLevel
} from './complianceTypes';

/**
 * Turns a read that cannot decrypt into a read that returns everything it CAN
 * decrypt, and says exactly how to fix it.
 *
 * Without this, a read of an entity whose encrypted columns were written under
 * a different tenant throws:
 *
 *   Failed to decrypt encrypted column value: Decryption failed: ciphertext is
 *   corrupted or the wrong key was used
 *
 * which surfaces as a 500. Three separate endpoints hit it in one week —
 * sign-up, the onboarding /me call, and an invitation lookup — because the
 * mistake is invisible until a tenant id is actually in play.
 *
 * ## Why this retries rather than nulling the fields
 *
 * The obvious implementation is for `EncryptedType.convertToJSValue` to return
 * `undefined` instead of throwing. That DESTROYS DATA, verified against a real
 * database: MikroORM hydrates the property as `undefined`, and the next flush
 * of any unrelated field writes NULL over the ciphertext. A `Type` only ever
 * sees a value — it has no handle on the entity, so it cannot mark a property
 * as not-loaded, and anything it returns is treated as the loaded value.
 *
 * A `fields` selection is different: MikroORM never hydrates the omitted
 * columns, knows they are absent, and leaves them out of the UPDATE. Also
 * verified — the ciphertext survives a flush untouched. So the retry re-issues
 * the same query with the encrypted columns excluded, which is the one
 * mechanism that both degrades and preserves the data.
 *
 * The cost is one extra query, and only on the failing path.
 *
 * ## What it does not hide
 *
 * Genuine ciphertext corruption produces the same error as a wrong tenant —
 * AES-GCM cannot tell them apart. Degrading silently would therefore hide real
 * data damage, so every degraded read logs a warning naming the entity, the
 * columns that were dropped, and the two ways to fix it properly.
 */

/** Classifications whose columns are encrypted at rest. */
const ENCRYPTED_LEVELS: ReadonlySet<ComplianceLevel> = new Set([
  'pii',
  'phi',
  'pci'
]);

/** Reads that hydrate entities, and so can hit a decryption failure. */
const HYDRATING_READS = new Set([
  'findOne',
  'findOneOrFail',
  'find',
  'findAll',
  'findAndCount'
]);

const isDecryptionFailure = (error: unknown): boolean =>
  error instanceof Error &&
  (error.message.includes('Failed to decrypt encrypted column value') ||
    error.message.includes(
      'Decryption failed: ciphertext is corrupted or the wrong key was used'
    ));

/** Best-effort entity name from whatever a caller passed as the first argument. */
const resolveEntityName = (entity: unknown): string | undefined => {
  if (typeof entity === 'string') return entity;
  if (entity == null) return undefined;
  if (typeof entity === 'object') {
    const named = entity as { name?: unknown; className?: unknown };
    if (typeof named.name === 'string') return named.name;
    if (typeof named.className === 'string') return named.className;
  }
  if (typeof entity === 'function') {
    return (entity as { name?: string }).name;
  }
  return undefined;
};

/** The encrypted-at-rest column names registered for an entity. */
const encryptedColumnsOf = (entityName: string): string[] => {
  const fields = getEntityComplianceFields(entityName);
  if (!fields) return [];
  return [...fields.entries()]
    .filter(([, level]) => ENCRYPTED_LEVELS.has(level))
    .map(([field]) => field);
};

/** Minimal logger shape, so this does not depend on the telemetry package. */
export interface ForgivingDecryptionLogger {
  warn(message: string, meta?: Record<string, unknown>): void;
}

const buildWarning = (
  entityName: string,
  dropped: string[]
): [string, Record<string, unknown>] => [
  `[encryption] Read of '${entityName}' could not decrypt ${dropped.length} ` +
    `column(s) and returned without them: ${dropped.join(', ')}. ` +
    'The row was NOT modified. Fix this properly by either (a) binding the ' +
    'owning tenant — wrapEmWithTenantContext(em, tenantId), or withEncryptionContext(tenantId, ...) — ' +
    'before the read, or (b) if the tenant genuinely is not known yet, because ' +
    'this read is what discovers it, selecting only the columns you need: ' +
    `findOne(${entityName}, where, { fields: [...] }). ` +
    'If the tenant WAS bound correctly, this is not a context problem — ' +
    'suspect real ciphertext corruption and investigate the row.',
  { entity: entityName, droppedColumns: dropped }
];

/**
 * Wraps an EntityManager so hydrating reads degrade instead of throwing.
 *
 * Returns a Proxy; every non-read member passes through untouched. Reads that
 * succeed are unaffected and pay nothing.
 */
export function wrapEmWithForgivingDecryption<TEntityManager extends object>(
  em: TEntityManager,
  logger: ForgivingDecryptionLogger
): TEntityManager {
  return new Proxy(em, {
    get(target, prop, receiver) {
      const value = Reflect.get(target, prop, receiver);

      if (typeof prop !== 'string' || !HYDRATING_READS.has(prop)) {
        return typeof value === 'function' ? value.bind(target) : value;
      }
      if (typeof value !== 'function') return value;

      const read = value as (...args: unknown[]) => Promise<unknown>;

      return async (...args: unknown[]) => {
        try {
          return await read.apply(target, args);
        } catch (error) {
          if (!isDecryptionFailure(error)) throw error;

          const entityName = resolveEntityName(args[0]);
          const dropped = entityName ? encryptedColumnsOf(entityName) : [];

          // Nothing known to drop means the retry would be identical, so the
          // original error is the honest outcome.
          if (!entityName || dropped.length === 0) throw error;

          const metadata = (
            target as { getMetadata?: () => { find?: (n: string) => unknown } }
          ).getMetadata?.();
          const meta = metadata?.find?.(entityName) as
            { props?: Record<string, unknown> } | undefined;
          const allProps = meta?.props ? Object.keys(meta.props) : [];
          const safeFields = allProps.filter((p) => !dropped.includes(p));

          // Without metadata there is no way to name the complement, and a
          // guess could omit something the caller needs. Surface the original.
          if (safeFields.length === 0) throw error;

          const [message, meta_] = buildWarning(entityName, dropped);
          logger.warn(message, meta_);

          const [entity, where, options] = args as [
            unknown,
            unknown,
            Record<string, unknown> | undefined
          ];
          return await read.call(target, entity, where, {
            ...(options ?? {}),
            fields: safeFields
          });
        }
      };
    }
  });
}
