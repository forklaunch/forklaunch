import type { EntityManager, EventSubscriber, MikroORM } from '@mikro-orm/core';
import { TENANT_FILTER_NAME } from './tenantFilter';

/**
 * Structural subset of {@link TransactionEventArgs} used by
 * `RlsEventSubscriber` so that callers (and tests) only need to provide
 * the properties the subscriber actually reads.
 */
export interface RlsTransactionEventArgs {
  em: {
    getFilterParams: EntityManager['getFilterParams'];
    getConnection(): {
      execute(
        query: string,
        params?: unknown[],
        method?: 'all' | 'get' | 'run',
        ctx?: unknown
      ): Promise<unknown>;
    };
  };
  transaction?: unknown;
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

export interface RlsConfig {
  /**
   * Whether to enable PostgreSQL Row-Level Security.
   * Defaults to `true` when the driver is PostgreSQL, `false` otherwise.
   * Set to `false` to opt out even on PostgreSQL.
   */
  enabled?: boolean;
}

// ---------------------------------------------------------------------------
// RLS EventSubscriber
// ---------------------------------------------------------------------------

/**
 * MikroORM EventSubscriber that executes `SET LOCAL app.tenant_id = :tenantId`
 * at the start of every transaction when PostgreSQL RLS is enabled.
 *
 * This ensures that even if the MikroORM global filter is somehow bypassed,
 * the database-level RLS policy enforces tenant isolation.
 *
 * The tenant ID is read from the EntityManager's filter parameters
 * (set by the tenant context middleware).
 */
export class RlsEventSubscriber implements EventSubscriber {
  async afterTransactionStart(args: RlsTransactionEventArgs): Promise<void> {
    const tenantId = this.getTenantId(args.em);
    if (!tenantId) {
      // No tenant context (e.g., super-admin or public route) — skip SET LOCAL
      return;
    }

    const connection = args.em.getConnection();
    // Execute SET LOCAL within the transaction context
    // SET LOCAL only persists for the current transaction — no connection leakage
    await connection.execute(
      `SET LOCAL app.tenant_id = '${escapeSqlString(tenantId)}'`,
      [],
      'run',
      args.transaction
    );
  }

  private getTenantId(
    em: Pick<EntityManager, 'getFilterParams'>
  ): string | undefined {
    const params = em.getFilterParams(TENANT_FILTER_NAME) as
      | { tenantId?: string }
      | undefined;
    return params?.tenantId;
  }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

/**
 * Sets up PostgreSQL Row-Level Security integration.
 *
 * 1. Registers the `RlsEventSubscriber` to run `SET LOCAL app.tenant_id`
 *    at the start of every transaction.
 * 2. Validates that RLS policies exist on tenant-scoped tables (warns if missing).
 *
 * Call this at application bootstrap after `MikroORM.init()` and `setupTenantFilter()`.
 *
 * @param orm - The initialized MikroORM instance
 * @param config - RLS configuration (enabled defaults to auto-detect PostgreSQL)
 */
export function setupRls(orm: MikroORM, config: RlsConfig = {}): void {
  const isPostgres = isPostgresDriver(orm);
  const enabled = config.enabled ?? isPostgres;

  if (!enabled) {
    if (!isPostgres) {
      // Non-PostgreSQL — RLS not available, ORM filter is the sole enforcement
      return;
    }
    // PostgreSQL but explicitly disabled
    console.info(
      '[compliance] PostgreSQL RLS disabled by configuration. ORM filter is the sole tenant enforcement layer.'
    );
    return;
  }

  if (!isPostgres) {
    console.warn(
      '[compliance] RLS enabled but database driver is not PostgreSQL. ' +
        'RLS is only supported on PostgreSQL. Falling back to ORM filter only.'
    );
    return;
  }

  // Register the RLS transaction subscriber
  orm.em.getEventManager().registerSubscriber(new RlsEventSubscriber());

  // Validate RLS policies exist
  validateRlsPolicies(orm).catch((err) => {
    console.warn('[compliance] Failed to validate RLS policies:', err);
  });
}

// ---------------------------------------------------------------------------
// RLS policy validation
// ---------------------------------------------------------------------------

/**
 * Checks that tenant-scoped entities have RLS policies on their tables.
 * Logs warnings with the SQL needed to create missing policies.
 */
async function validateRlsPolicies(orm: MikroORM): Promise<void> {
  const metadata = orm.em.getMetadata().getAll();

  for (const meta of Object.values(metadata)) {
    const hasOrgId = meta.properties['organizationId'] != null;
    const hasOrg = meta.properties['organization'] != null;

    if (!hasOrgId && !hasOrg) continue;

    const tableName = meta.tableName;
    try {
      const connection = orm.em.getConnection();
      const result = await connection.execute<{ policyname: string }[]>(
        `SELECT policyname FROM pg_policies WHERE tablename = '${escapeSqlString(tableName)}'`,
        [],
        'all'
      );

      const policies = Array.isArray(result) ? result : [];
      const hasTenantPolicy = policies.some((p: { policyname: string }) =>
        p.policyname.includes('tenant')
      );

      if (!hasTenantPolicy) {
        console.warn(
          `[compliance] No tenant RLS policy found on table '${tableName}'. ` +
            `Create one with:\n` +
            `  ALTER TABLE "${tableName}" ENABLE ROW LEVEL SECURITY;\n` +
            `  CREATE POLICY tenant_isolation ON "${tableName}"\n` +
            `    USING (organization_id = current_setting('app.tenant_id'));`
        );
      }
    } catch {
      // Query failed — likely not connected yet or table doesn't exist
      // Skip validation for this table
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Detect whether the ORM is using a PostgreSQL driver.
 * Checks the platform constructor name which is 'PostgreSqlPlatform' for PG.
 */
function isPostgresDriver(orm: MikroORM): boolean {
  try {
    const platform = orm.em.getPlatform();
    const name = platform.constructor.name.toLowerCase();
    return name.includes('postgre');
  } catch {
    return false;
  }
}

/**
 * Escape a string for safe inclusion in SQL. Prevents SQL injection in
 * the SET LOCAL statement.
 */
function escapeSqlString(value: string): string {
  return value.replace(/'/g, "''");
}
