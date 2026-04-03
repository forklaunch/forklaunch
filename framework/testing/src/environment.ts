import { StartedTestContainer } from 'testcontainers';
import { DatabaseType } from './containers';

export interface TestEnvConfig {
  database: StartedTestContainer | null;
  databaseType?: DatabaseType;
  redis?: StartedTestContainer;
  kafka?: StartedTestContainer;
  s3?: StartedTestContainer;
  hmacSecret?: string;
  customVars?: Record<string, string>;
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
 * Setup test environment variables for a blueprint test
 */
export function setupTestEnvironment(config: TestEnvConfig): void {
  const {
    database,
    databaseType,
    redis,
    kafka,
    s3,
    hmacSecret = 'test-secret-key',
    customVars = {}
  } = config;

  if (databaseType) {
    const dbPort = getDatabasePort(databaseType);

    process.env.DB_NAME = 'test_db';

    // SQLite databases are file-based, no container needed
    if (databaseType === 'sqlite' || databaseType === 'libsql') {
      process.env.DB_PATH = ':memory:'; // In-memory SQLite for tests
    } else if (database) {
      process.env.DB_HOST = database.getHost();
      process.env.DB_USER = databaseType === 'mssql' ? 'SA' : 'test_user';
      process.env.DB_PASSWORD =
        databaseType === 'mssql' ? 'Test_Password123!' : 'test_password';
      process.env.DB_PORT = database.getMappedPort(dbPort).toString();
    }
  }

  if (redis) {
    process.env.REDIS_URL = `redis://${redis.getHost()}:${redis.getMappedPort(6379)}`;
    process.env.REDIS_HOST = redis.getHost();
    process.env.REDIS_PORT = redis.getMappedPort(6379).toString();
  }

  if (kafka) {
    const kafkaBroker = `${kafka.getHost()}:${kafka.getMappedPort(9092)}`;
    process.env.KAFKA_BROKERS = kafkaBroker;
    process.env.KAFKA_CLIENT_ID = 'test-client';
    process.env.KAFKA_GROUP_ID = 'test-group';
  }

  if (s3) {
    process.env.S3_ENDPOINT = `http://${s3.getHost()}:${s3.getMappedPort(9000)}`;
    process.env.S3_ACCESS_KEY_ID = 'minioadmin';
    process.env.S3_SECRET_ACCESS_KEY = 'minioadmin';
    process.env.S3_REGION = 'us-east-1';
    process.env.S3_BUCKET = 'test-bucket';
    process.env.S3_FORCE_PATH_STYLE = 'true'; // Required for MinIO
  }

  process.env.HMAC_SECRET_KEY = hmacSecret;
  process.env.JWKS_PUBLIC_KEY_URL =
    'http://localhost:3000/.well-known/jwks.json';
  process.env.OTEL_SERVICE_NAME = 'test-service';
  process.env.OTEL_EXPORTER_OTLP_ENDPOINT = 'http://localhost:4318';
  process.env.HOST = 'localhost';
  process.env.PORT = '3000';
  process.env.NODE_ENV = 'test';
  process.env.VERSION = 'v1';
  process.env.DOCS_PATH = '/docs';
  process.env.OTEL_LEVEL = 'info';
  process.env.DOTENV_FILE_PATH = '.env.test';

  // Custom environment variables
  Object.entries(customVars).forEach(([key, value]) => {
    process.env[key] = value;
  });
}
