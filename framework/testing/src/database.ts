import { MikroORM, Options } from '@mikro-orm/core';
import Redis from 'ioredis';
import { StartedTestContainer } from 'testcontainers';
import { DatabaseType } from './containers';

export interface MikroOrmTestConfig {
  /**
   * MikroORM config object (imported from mikro-orm.config)
   */
  mikroOrmConfig: Options;

  /**
   * Database type (postgres, mysql, mongodb, etc.)
   */
  databaseType: DatabaseType;

  /**
   * Whether to use migrations (true) or schema generation (false)
   * - true: IAM blueprints (uses getMigrator().up())
   * - false: Billing blueprints (uses getSchemaGenerator().createSchema())
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
    case 'better-sqlite':
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
): Promise<MikroORM> {
  const {
    mikroOrmConfig,
    databaseType,
    useMigrations = false,
    container
  } = config;

  const dbPort = getDatabasePort(databaseType);

  // SQLite databases are file-based
  let ormConfig: Options = {};
  if (
    databaseType === 'sqlite' ||
    databaseType === 'better-sqlite' ||
    databaseType === 'libsql'
  ) {
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
    await orm.getMigrator().up();
  } else {
    await orm.getSchemaGenerator().createSchema();
  }

  return orm;
}

/**
 * Clear all data from the test database and/or cache
 */
export async function clearTestDatabase(options?: {
  orm?: MikroORM;
  redis?: Redis;
}): Promise<void> {
  const { orm, redis } = options || {};

  if (redis) {
    await redis.flushall();
  }

  if (orm) {
    const em = orm.em.fork();
    const entities = Object.values(orm.getMetadata().getAll());

    // Delete in reverse order to avoid foreign key constraints
    for (const entity of entities.reverse()) {
      try {
        await em.nativeDelete(entity.class, {});
      } catch (error) {
        // Ignore "table does not exist" errors
        if (!(error as Error).message?.includes('does not exist')) {
          throw error;
        }
      }
    }

    await em.flush();
  }
}
