import type { EntityManager } from '@mikro-orm/core';
import { getEntityComplianceFields } from './complianceTypes';
import {
  isEncryptedCiphertext,
  stampedKeyId,
  type FieldEncryptor
} from './fieldEncryptor';

/**
 * Rotation sweep: rewrite every compliance-encrypted column that is still
 * under a previous master key so it is under the current one.
 *
 * Rotating a key is two steps. First the old key moves into the ring
 * (`LEGACY_ENCRYPTION_KEYS`) and the new key becomes `ENCRYPTION_KEY`; the
 * service keeps reading everything because {@link FieldEncryptor} tries
 * every key in the ring. Then this sweep runs, usually from a migration,
 * and rewrites what the old key still holds. When the report shows nothing
 * left under the old key it can be dropped from the ring.
 *
 * Per value:
 *   plaintext  - not a `v1:`/`v2:` ciphertext; left alone
 *   current    - opens with the current key in the current format; left alone
 *   rewritten  - opens only with a previous key, or is unstamped while the
 *                encryptor writes v3; re-encrypted under the current key in
 *                the current format with the SAME tenant id it was written with
 *   unreadable - opens with no key under any candidate tenant; left alone
 *                and reported by row id
 *
 * Tenants: keys are derived per tenant, so a value only opens under the
 * tenant it was written with. Each row is tried under its likely tenants
 * first (its `organization_id`, then the empty tenant, extended by
 * `tenantIdsFor`) and, only when those fail, under `fallbackTenantIds`
 * (typically every organization the database knows, see
 * {@link collectFallbackTenantIds}). That catches rows written under a
 * leaked or mismatched context.
 *
 * Raw SQL throughout: going through entities would decrypt on hydrate and
 * re-encrypt on flush, which is both slower and blind to which key opened
 * the value.
 */

const ENCRYPTED_LEVELS = new Set(['pii', 'phi', 'pci']);

export const isEncryptedValue = isEncryptedCiphertext;

export type RotationOutcome =
  | { kind: 'plaintext' }
  | { kind: 'current'; tenantId: string; keyId: string }
  | { kind: 'rewritten'; tenantId: string; keyId: string; next: string }
  | { kind: 'unreadable' };

const tryOpen = (
  encryptor: FieldEncryptor,
  value: string,
  tenantId: string
) => {
  try {
    return encryptor.open(value, tenantId);
  } catch {
    return null;
  }
};

/**
 * Classify one stored value against the encryptor's ring. `tenantIds` are
 * the row's likely tenants, most likely first; `fallbackTenantIds` are
 * tried only when none of those open the value with any key.
 */
export function classifyEncryptedValue(
  value: unknown,
  encryptor: FieldEncryptor,
  tenantIds: readonly string[],
  fallbackTenantIds: readonly string[] = []
): RotationOutcome {
  if (!isEncryptedValue(value)) return { kind: 'plaintext' };
  const primary = tenantIds.length ? tenantIds : [''];
  const candidates = [
    ...primary,
    ...fallbackTenantIds.filter((t) => !primary.includes(t))
  ];
  for (const tenantId of candidates) {
    const opened = tryOpen(encryptor, value, tenantId);
    if (!opened) continue;
    if (!opened.stale) {
      return { kind: 'current', tenantId, keyId: opened.keyId };
    }
    const next = encryptor.encrypt(opened.plaintext, tenantId);
    return next
      ? { kind: 'rewritten', tenantId, keyId: opened.keyId, next }
      : { kind: 'unreadable' };
  }
  return { kind: 'unreadable' };
}

export interface RotationTableReport {
  table: string;
  columns: string[];
  scanned: number;
  plaintext: number;
  current: number;
  rewritten: number;
  unreadable: number;
  /** Row ids (primary key as string) with at least one unreadable value. */
  unreadableIds: string[];
  /** How many values each previous key still held before this pass. */
  byKeyId: Record<string, number>;
  /** Stamped key ids seen on unreadable values: keys that must be added to the ring. */
  missingKeyIds: Record<string, number>;
}

export type SqlExecute = (sql: string, params?: unknown[]) => Promise<unknown>;

export interface ReencryptOptions {
  em: EntityManager;
  /** The ring: current key plus every previous key. */
  encryptor: FieldEncryptor;
  /** Report only; write nothing. */
  dryRun?: boolean;
  /**
   * Candidate tenant ids for a row, most likely first. The default tries
   * the row's `organization_id` (when the table has one) and then the empty
   * tenant. Modules with other tenant shapes (an organization is its own
   * tenant) extend it.
   */
  tenantIdsFor?: (entityName: string, row: Record<string, unknown>) => string[];
  /**
   * Tenants to try only when a row's own candidates open nothing. Pass the
   * result of {@link collectFallbackTenantIds} to catch rows written under a
   * mismatched context. Empty by default.
   */
  fallbackTenantIds?: readonly string[];
  /**
   * How to run SQL. A migration must pass its own `this.execute`: MikroORM
   * applies pending migrations inside one transaction, and a pooled
   * connection cannot see tables that earlier migrations created but have
   * not committed yet. Defaults to the entity manager's connection.
   */
  execute?: SqlExecute;
  log?: (message: string) => void;
}

/** `organization_id` first when the row carries one, then the empty tenant. */
export function defaultTenantIdsFor(
  _entityName: string,
  row: Record<string, unknown>
): string[] {
  const candidates: string[] = [];
  for (const key of ['organization_id', 'organizationId']) {
    const v = row[key];
    if (typeof v === 'string' && v) candidates.push(v);
  }
  candidates.push('');
  return [...new Set(candidates)];
}

const quote = (id: string) => `"${id.replace(/"/g, '""')}"`;

/**
 * Every tenant id the database knows: the distinct `organization_id` of
 * every table in the current schema that has one, plus `organization.id`
 * where the module owns organizations. Used as the last-resort tenant list.
 * PostgreSQL only (information_schema + to_regclass).
 */
export async function collectFallbackTenantIds(
  execute: SqlExecute
): Promise<string[]> {
  const ids = new Set<string>();
  const tables = (await execute(
    `select table_name from information_schema.columns where table_schema = current_schema() and column_name = 'organization_id'`
  )) as { table_name: string }[];
  for (const { table_name } of tables) {
    const rows = (await execute(
      `select distinct "organization_id" as id from ${quote(table_name)} where "organization_id" is not null`
    )) as { id: unknown }[];
    for (const { id } of rows) if (typeof id === 'string' && id) ids.add(id);
  }
  const org = (await execute(`select to_regclass('"organization"') as t`)) as {
    t: string | null;
  }[];
  if (org[0]?.t) {
    const rows = (await execute(`select "id" from "organization"`)) as {
      id: unknown;
    }[];
    for (const { id } of rows) if (typeof id === 'string' && id) ids.add(id);
  }
  return [...ids];
}

export interface EntityMetadataLike {
  className: string;
  tableName?: string;
  abstract?: boolean;
  embeddable?: boolean;
  primaryKeys: readonly string[];
  properties: Record<
    string,
    { fieldNames: string[]; columnTypes?: string[] } | undefined
  >;
}

/**
 * Every entity MikroORM discovered. `getMetadata().getAll()` is a Map in
 * MikroORM 7 and was a plain object before; accept both shapes.
 */
export function entityMetadataList(em: EntityManager): EntityMetadataLike[] {
  const all = em.getMetadata().getAll() as unknown;
  if (all instanceof Map) return [...all.values()] as EntityMetadataLike[];
  if (Array.isArray(all)) return all as EntityMetadataLike[];
  if (all && typeof all === 'object')
    return Object.values(all as Record<string, EntityMetadataLike>);
  return [];
}

/**
 * Walk every registered entity with encrypted compliance fields and rewrite
 * previous-key ciphertexts under the current key. Safe to re-run: every
 * value is classified before anything is written.
 */
export async function reencryptEncryptedColumns(
  options: ReencryptOptions
): Promise<RotationTableReport[]> {
  const { em, encryptor, dryRun = false, log = () => undefined } = options;
  const tenantIdsFor = options.tenantIdsFor ?? defaultTenantIdsFor;
  const fallbackTenantIds = options.fallbackTenantIds ?? [];
  const connection = em.getConnection();
  const execute: SqlExecute =
    options.execute ??
    ((sql: string, params?: unknown[]) => connection.execute(sql, params));
  const reports: RotationTableReport[] = [];
  const ids = encryptor.keyIds;

  log(
    `[key-rotation] current key ${ids.current}; previous: ${ids.previous.join(', ') || 'none'}; ${fallbackTenantIds.length} fallback tenant(s)${dryRun ? '; DRY RUN' : ''}`
  );

  const metas = entityMetadataList(em);
  if (metas.length === 0) {
    throw new Error(
      '[key-rotation] MikroORM reported no entities; refusing to report a sweep over nothing'
    );
  }
  for (const meta of metas) {
    if (meta.abstract || meta.embeddable || !meta.tableName) continue;
    const fields = getEntityComplianceFields(meta.className);
    if (!fields) continue;
    const props = meta.properties;
    const encryptedProps = [...fields.entries()]
      .filter(([, level]) => ENCRYPTED_LEVELS.has(level))
      .map(([prop]) => props[prop])
      .filter(
        (p): p is { fieldNames: string[]; columnTypes?: string[] } =>
          !!p && Array.isArray(p.fieldNames) && p.fieldNames.length === 1
      );
    if (encryptedProps.length === 0) continue;

    const pkProp = props[meta.primaryKeys[0] as string];
    if (!pkProp) continue;
    const pkColumn = pkProp.fieldNames[0];
    const columns = encryptedProps.map((p) => p.fieldNames[0]);
    const report: RotationTableReport = {
      table: meta.tableName,
      columns,
      scanned: 0,
      plaintext: 0,
      current: 0,
      rewritten: 0,
      unreadable: 0,
      unreadableIds: [],
      byKeyId: {},
      missingKeyIds: {}
    };

    // An entity can be registered before its table exists (some auth
    // libraries create tables at runtime; a fresh harness runs this first).
    const exists = (await execute(`select to_regclass(?) as t`, [
      quote(meta.tableName)
    ])) as { t: string | null }[];
    if (!exists[0]?.t) {
      log(
        `[key-rotation] ${meta.tableName}: table does not exist yet, skipped`
      );
      continue;
    }

    const rows = (await execute(
      `select * from ${quote(meta.tableName)}`
    )) as Record<string, unknown>[];

    for (const row of rows) {
      report.scanned += 1;
      const tenantIds = tenantIdsFor(meta.className, row);
      const updates: { column: string; next: string; jsonb: boolean }[] = [];
      let rowUnreadable = false;
      for (const prop of encryptedProps) {
        const column = prop.fieldNames[0];
        const outcome = classifyEncryptedValue(
          row[column],
          encryptor,
          tenantIds,
          fallbackTenantIds
        );
        if (outcome.kind === 'plaintext') report.plaintext += 1;
        else if (outcome.kind === 'current') report.current += 1;
        else if (outcome.kind === 'rewritten') {
          report.rewritten += 1;
          report.byKeyId[outcome.keyId] =
            (report.byKeyId[outcome.keyId] ?? 0) + 1;
          updates.push({
            column,
            next: outcome.next,
            jsonb: (prop.columnTypes?.[0] ?? '')
              .toLowerCase()
              .startsWith('json')
          });
        } else {
          report.unreadable += 1;
          rowUnreadable = true;
          const missing = stampedKeyId(String(row[column]));
          if (missing)
            report.missingKeyIds[missing] =
              (report.missingKeyIds[missing] ?? 0) + 1;
        }
      }
      if (rowUnreadable) report.unreadableIds.push(String(row[pkColumn]));
      if (updates.length && !dryRun) {
        const sets = updates.map(
          (u) => `${quote(u.column)} = ${u.jsonb ? 'to_jsonb(?::text)' : '?'}`
        );
        await execute(
          `update ${quote(meta.tableName)} set ${sets.join(', ')} where ${quote(pkColumn)} = ?`,
          [...updates.map((u) => u.next), row[pkColumn]]
        );
      }
    }

    const byKey = Object.entries(report.byKeyId)
      .map(([k, n]) => `${k}: ${n}`)
      .join(', ');
    const missing = Object.entries(report.missingKeyIds)
      .map(([k, n]) => `${k}: ${n}`)
      .join(', ');
    log(
      `[key-rotation] ${meta.tableName}: scanned ${report.scanned} rows, ` +
        `${report.rewritten} values ${dryRun ? 'would be ' : ''}rewritten${byKey ? ` (from ${byKey})` : ''}, ` +
        `${report.current} already current, ${report.plaintext} plaintext, ` +
        `${report.unreadable} unreadable${missing ? ` (stamped with keys not in the ring: ${missing})` : ''}${report.unreadableIds.length ? ` (rows ${report.unreadableIds.slice(0, 10).join(', ')}${report.unreadableIds.length > 10 ? ', ...' : ''})` : ''}`
    );
    reports.push(report);
  }
  return reports;
}

/**
 * How many values in a column are stamped with each key (`v3`), and how
 * many are unstamped (`v1`/`v2`). Cheap, read-only, no decryption: the
 * answer to "can this key be dropped yet?" once a table is on `v3`.
 */
export async function countValuesByKeyId(
  execute: SqlExecute,
  table: string,
  column: string
): Promise<{
  byKeyId: Record<string, number>;
  unstamped: number;
  plaintext: number;
}> {
  const rows = (await execute(
    `select case when ${quote(column)} like 'v3:%' then split_part(${quote(column)}, ':', 2) when ${quote(column)} like 'v1:%' or ${quote(column)} like 'v2:%' then '' else null end as key_id, count(*)::int as n from ${quote(table)} group by 1`
  )) as { key_id: string | null; n: number }[];
  const byKeyId: Record<string, number> = {};
  let unstamped = 0;
  let plaintext = 0;
  for (const { key_id, n } of rows) {
    if (key_id === null) plaintext += Number(n);
    else if (key_id === '') unstamped += Number(n);
    else byKeyId[key_id] = (byKeyId[key_id] ?? 0) + Number(n);
  }
  return { byKeyId, unstamped, plaintext };
}

/** Sum a report field across tables. */
export function rotationTotals(reports: readonly RotationTableReport[]) {
  const sum = (pick: (r: RotationTableReport) => number) =>
    reports.reduce((n, r) => n + pick(r), 0);
  return {
    tables: reports.length,
    scanned: sum((r) => r.scanned),
    rewritten: sum((r) => r.rewritten),
    current: sum((r) => r.current),
    plaintext: sum((r) => r.plaintext),
    unreadable: sum((r) => r.unreadable)
  };
}
