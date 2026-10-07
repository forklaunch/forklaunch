import { MikroORM } from '@mikro-orm/core';

/**
 * The type `MikroORM.init()` actually returns. Its `Entities` parameter is a
 * readonly array, which is not assignable to a bare `MikroORM` annotation
 * (whose `Entities` default is mutable) — so the harness names the returned
 * type directly rather than widening the parameters to `any`, which would have
 * accepted anything at all instead of exactly what `init()` hands back.
 */
export type AnyMikroORM = Awaited<ReturnType<typeof MikroORM.init>>;
import Redis from 'ioredis';
import { StartedTestContainer } from 'testcontainers';
import { DatabaseType } from './containers';

/**
 * MikroORM options that accept any driver's `defineConfig()` return —
 * mutable or readonly entities, base or driver-specific `Options`
 * (e.g. `Options<PostgreSqlDriver, ...>` is not assignable to the
 * base-driver `Options` default). Taken from the parameter `init()` accepts,
 * so the harness tracks MikroORM's own shape instead of restating it.
 */
export type AnyMikroOrmOptions = Partial<
  NonNullable<Parameters<typeof MikroORM.init>[0]>
>;

export interface MikroOrmTestConfig {
  /**
   * MikroORM config object (imported from mikro-orm.config)
   */
  mikroOrmConfig: AnyMikroOrmOptions;

  /**
   * Database type (postgres, mysql, mongodb, etc.)
   */
  databaseType: DatabaseType;

  /**
   * Whether to use migrations (true) or schema generation (false)
   * - true: IAM blueprints (uses orm.migrator.up())
   * - false: Billing blueprints (uses orm.schema.create())
   */
  useMigrations?: boolean;

  /**
   * Path to migrations directory (required if useMigrations is true)
   */
  migrationsPath?: string;

  /**
   * Database container instance (null for file-based databases like SQLite)
   */
  container: StartedTestContainer | null;
}

/**
 * Get the default port for a database type
 */
function getDatabasePort(type: DatabaseType): number {
  switch (type) {
    case 'postgres':
    case 'postgresql':
      return 5432;
    case 'mysql':
    case 'mariadb':
      return 3306;
    case 'mongodb':
    case 'mongo':
      return 27017;
    case 'mssql':
      return 1433;
    case 'sqlite':
    case 'libsql':
      return 0; // SQLite is file-based, no port
    default:
      return 5432;
  }
}

/**
 * Setup MikroORM for testing with proper schema/migrations
 */
export async function setupTestORM(
  config: MikroOrmTestConfig
): Promise<AnyMikroORM> {
  const {
    mikroOrmConfig,
    databaseType,
    useMigrations = false,
    container
  } = config;

  const dbPort = getDatabasePort(databaseType);

  // SQLite databases are file-based
  let ormConfig: AnyMikroOrmOptions = {};
  if (databaseType === 'sqlite' || databaseType === 'libsql') {
    ormConfig = {
      ...mikroOrmConfig,
      dbName: ':memory:', // In-memory SQLite for tests
      debug: false,
      ...(useMigrations
        ? {
            migrations: {
              path: config.migrationsPath,
              glob: '!(*.d).{js,ts}',
              dropTables: true
            }
          }
        : {
            schemaGenerator: {
              createForeignKeyConstraints: false
            }
          })
    };
  } else if (container) {
    ormConfig = {
      ...mikroOrmConfig,
      dbName: 'test_db',
      host: container.getHost(),
      user: databaseType === 'mssql' ? 'SA' : 'test_user',
      password:
        databaseType === 'mssql' ? 'Test_Password123!' : 'test_password',
      port: container.getMappedPort(dbPort),
      debug: false,
      ...(useMigrations
        ? {
            migrations: {
              path: config.migrationsPath,
              glob: '!(*.d).{js,ts}',
              dropTables: true
            }
          }
        : {
            schemaGenerator: {
              createForeignKeyConstraints: false
            }
          })
    };
  }

  const orm = await MikroORM.init(ormConfig);

  if (useMigrations) {
    await orm.migrator.up();
  } else {
    await orm.schema.create();
  }

  return orm;
}

/**
 * Clear all data from the test database and/or cache
 */
export async function clearTestDatabase(options?: {
  orm?: AnyMikroORM;
  redis?: Redis;
}): Promise<void> {
  const { orm, redis } = options || {};

  if (redis) {
    await redis.flushall();
  }

  if (orm) {
    const em = orm.em.fork();
    // orm.getMetadata().getAll() returns an empty object under MikroORM v7, so
    // the configured entity list is the reliable source of what to clear.
    type Deletable = Parameters<typeof em.nativeDelete>[0];
    let remaining = [...(orm.config.get('entities') as Deletable[])];

    // The entity list is in declaration order, not FK-dependency order, so a
    // single reverse pass can still violate foreign keys. Retry the ones that
    // fail on a constraint until a full pass clears nothing new — that leaves
    // only genuine errors (a real FK cycle, which would stop making progress).
    while (remaining.length > 0) {
      const stillBlocked: Deletable[] = [];
      let lastConstraintError: Error | undefined;
      let progressed = false;

      for (const entity of remaining) {
        try {
          await em.nativeDelete(entity, {});
          progressed = true;
        } catch (error) {
          const message = (error as Error).message ?? '';
          if (message.includes('does not exist')) {
            continue; // table not created — nothing to clear
          }
          if (/foreign key|constraint/i.test(message)) {
            stillBlocked.push(entity);
            lastConstraintError = error as Error;
            continue;
          }
          throw error;
        }
      }

      if (!progressed) {
        if (lastConstraintError) {
          throw lastConstraintError; // no progress => unbreakable FK cycle
        }
        break;
      }
      remaining = stillBlocked;
    }

    await em.flush();
  }
}
