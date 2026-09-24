import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { EntityManager, EntityName } from '@mikro-orm/postgresql';

export interface CodeSetRow {
  code: string;
  description: string;
  effectiveDate?: Date;
}

export interface CodeSetLoadResult {
  rowsRead: number;
  rowsUpserted: number;
  batches: number;
}

const DEFAULT_BATCH_SIZE = 1000;

/**
 * Streams already-parsed rows from any source into a code-set reference
 * table, batch-upserted on the `code` column (idempotent — safe to re-run on
 * a schedule as new releases land). This is the "ETL shape" §7 calls for:
 * the source is just an async iterable of rows, so pointing it at a
 * different real feed later — a bigger CSV, an S3 object, a customer's own
 * licensed CPT connector (§5) — is a matter of writing a new row source, not
 * changing this batching/upsert logic.
 */
export class CodeSetLoaderService {
  constructor(
    private readonly em: EntityManager,
    private readonly otel: OpenTelemetryCollector<MetricsDefinition>
  ) {}

  async load<T extends { code: string }>(
    entityClass: EntityName<T>,
    rows: AsyncIterable<CodeSetRow> | Iterable<CodeSetRow>,
    options?: {
      batchSize?: number;
      // Global reference tables (Icd10Code/HcpcsCode) are unique on `code`
      // alone; org-scoped tables (CptCode, §5) are unique on
      // (organizationId, code) instead — see cptCode.entity.ts.
      onConflictFields?: (keyof T)[];
      // Stamped onto every row when loading into an org-scoped table.
      organizationId?: string;
    }
  ): Promise<CodeSetLoadResult> {
    const batchSize = options?.batchSize ?? DEFAULT_BATCH_SIZE;
    const onConflictFields = options?.onConflictFields ?? (['code'] as (keyof T)[]);
    const organizationId = options?.organizationId;
    const result: CodeSetLoadResult = {
      rowsRead: 0,
      rowsUpserted: 0,
      batches: 0
    };
    let batch: CodeSetRow[] = [];

    const flushBatch = async () => {
      if (batch.length === 0) return;

      const em = this.em.fork();
      await em.upsertMany(
        entityClass,
        batch.map((row) => ({
          ...(organizationId != null ? { organizationId } : {}),
          code: row.code,
          description: row.description,
          effectiveDate: row.effectiveDate ?? null
        })) as unknown as T[],
        { onConflictFields }
      );

      result.rowsUpserted += batch.length;
      result.batches += 1;
      this.otel.info('[CodeSetLoaderService] Batch upserted', {
        entity: String(entityClass),
        batch: result.batches,
        count: batch.length
      });
      batch = [];
    };

    for await (const row of rows) {
      result.rowsRead += 1;
      batch.push(row);
      if (batch.length >= batchSize) {
        await flushBatch();
      }
    }
    await flushBatch();

    this.otel.info('[CodeSetLoaderService] Load complete', {
      entity: String(entityClass),
      rowsRead: result.rowsRead,
      rowsUpserted: result.rowsUpserted,
      batches: result.batches
    });

    return result;
  }
}
