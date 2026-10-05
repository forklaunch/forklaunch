import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { EntityManager, EntityName } from '@mikro-orm/postgresql';
import { normalizeCode } from '../../domain/codeNormalization';

export interface CodeSetRow {
  code: string;
  description: string;
  effectiveDate?: Date;
}

export interface CodeSetLoadResult {
  rowsRead: number;
  rowsUpserted: number;
  batches: number;
  // lines of the source file that held no usable code and description
  rowsSkipped: number;
  // codes in the table that the release no longer contains, removed when
  // the load replaced a full release
  rowsRetired: number;
}

const DEFAULT_BATCH_SIZE = 1000;
// A full release smaller than this share of the rows already loaded is
// treated as a truncated file, and nothing is retired.
const MIN_RELEASE_SHARE = 0.5;

/**
 * Streams already-parsed rows from any source into a code-set reference
 * table, batch-upserted on the `code` column (idempotent — safe to re-run on
 * a schedule as new releases land). This is the "ETL shape" §7 calls for:
 * the source is just an async iterable of rows, so pointing it at a
 * different real feed later — a bigger CSV, an S3 object, a customer's own
 * licensed CPT connector (§5) — is a matter of writing a new row source, not
 * changing this batching/upsert logic.
 *
 * Codes are stored normalized (see normalizeCode). A code that appears
 * twice in one release is loaded once, the later row winning. With
 * `replaceRelease`, codes absent from the release are removed afterwards,
 * so a code retired by CDC/CMS stops validating.
 */
export class CodeSetLoaderService {
  constructor(
    private readonly em: EntityManager,
    private readonly otel: OpenTelemetryCollector<MetricsDefinition>
  ) {}

  async load<T extends { code: string }>(
    entityClass: EntityName<T>,
    rows: AsyncIterable<CodeSetRow> | Iterable<CodeSetRow>,
    options: {
      // the entity's table, for retiring codes
      tableName: string;
      batchSize?: number;
      // Global reference tables (Icd10Code/HcpcsCode) are unique on `code`
      // alone; org-scoped tables (CptCode, §5) are unique on
      // (organizationId, code) instead — see cptCode.entity.ts.
      onConflictFields?: (keyof T)[];
      // Stamped onto every row when loading into an org-scoped table.
      organizationId?: string;
      // The rows are a complete release: remove codes it no longer has.
      replaceRelease?: boolean;
      // lines the row source could not use, reported with the result
      skipped?: () => number;
    }
  ): Promise<CodeSetLoadResult> {
    const batchSize = options.batchSize ?? DEFAULT_BATCH_SIZE;
    const onConflictFields = options.onConflictFields ?? (['code'] as (keyof T)[]);
    const organizationId = options.organizationId;
    const result: CodeSetLoadResult = {
      rowsRead: 0,
      rowsUpserted: 0,
      batches: 0,
      rowsSkipped: 0,
      rowsRetired: 0
    };
    const loaded = new Set<string>();
    // keyed by code: Postgres refuses an upsert that touches one row twice
    // ("ON CONFLICT DO UPDATE command cannot affect row a second time")
    let batch = new Map<string, CodeSetRow>();

    const flushBatch = async () => {
      if (batch.size === 0) return;

      const em = this.em.fork();
      await em.upsertMany(
        entityClass,
        [...batch.values()].map((row) => ({
          ...(organizationId != null ? { organizationId } : {}),
          code: row.code,
          description: row.description,
          effectiveDate: row.effectiveDate ?? null
        })) as unknown as T[],
        { onConflictFields }
      );

      result.rowsUpserted += batch.size;
      result.batches += 1;
      this.otel.info('[CodeSetLoaderService] Batch upserted', {
        entity: options.tableName,
        batch: result.batches,
        count: batch.size
      });
      batch = new Map();
    };

    for await (const row of rows) {
      result.rowsRead += 1;
      const code = normalizeCode(row.code);
      if (!code) continue;
      // a code also seen in an earlier, already-flushed batch is upserted
      // again, which is harmless; within one batch the later row wins
      batch.set(code, { ...row, code });
      loaded.add(code);
      if (batch.size >= batchSize) {
        await flushBatch();
      }
    }
    await flushBatch();
    result.rowsSkipped = options.skipped?.() ?? 0;

    if (options.replaceRelease) {
      result.rowsRetired = await this.retireMissing(options.tableName, loaded, organizationId);
    }

    this.otel.info('[CodeSetLoaderService] Load complete', {
      entity: options.tableName,
      ...result
    });

    return result;
  }

  private async retireMissing(
    table: string,
    loaded: Set<string>,
    organizationId: string | undefined
  ): Promise<number> {
    const connection = this.em.fork().getConnection();
    const scope = organizationId != null ? ' and organization_id = ?' : '';
    const scopeParams = organizationId != null ? [organizationId] : [];
    const [{ count }] = await connection.execute<{ count: string }[]>(
      `select count(*) as count from "${table}" where true${scope}`,
      scopeParams
    );
    const existing = Number(count);
    if (existing > 0 && loaded.size < existing * MIN_RELEASE_SHARE) {
      throw new Error(
        `Refusing to retire codes from ${table}: the release has ${loaded.size} codes but ${existing} are loaded. A full release is never under half the current table; is the file truncated?`
      );
    }
    // One array parameter, not one per code: a release has tens of
    // thousands of codes and Postgres allows 65,535 parameters. Each element
    // is quoted, since a licensed CPT feed may use any characters.
    const codes = `{${[...loaded].map((c) => `"${c.replace(/(["\\])/g, '\\$1')}"`).join(',')}}`;
    const deleted = await connection.execute(
      `delete from "${table}" where not (code = any(?::text[]))${scope}`,
      [codes, ...scopeParams],
      'run'
    );
    return Number((deleted as { affectedRows?: number }).affectedRows ?? 0);
  }
}
