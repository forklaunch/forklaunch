import { GenericContainer, StartedTestContainer } from 'testcontainers';

export type DatabaseType =
  | 'postgres'
  | 'postgresql'
  | 'mysql'
  | 'mariadb'
  | 'mongodb'
  | 'mongo'
  | 'mssql'
  | 'libsql'
  | 'sqlite';

export interface PostgresConfig {
  user?: string;
  password?: string;
  database?: string;
  command?: string[];
}

export interface MySQLConfig {
  user?: string;
  password?: string;
  database?: string;
  rootPassword?: string;
}

export interface MongoDBConfig {
  user?: string;
  password?: string;
  database?: string;
}

export interface MSSQLConfig {
  user?: string;
  password?: string;
  database?: string;
  saPassword?: string;
}

export interface SQLiteConfig {
  database?: string;
}

export interface RedisConfig {
  command?: string[];
}

export interface KafkaConfig {
  /** Kafka cluster ID */
  clusterId?: string;
  /** Number of partitions */
  numPartitions?: number;
  /** Replication factor */
  replicationFactor?: number;
  /** Additional environment variables */
  env?: Record<string, string>;
}

export interface S3Config {
  /** MinIO root user (access key) */
  rootUser?: string;
  /** MinIO root password (secret key) */
  rootPassword?: string;
  /** Default bucket to create */
  defaultBucket?: string;
  /** Region */
  region?: string;
}

export type DatabaseConfig =
  | PostgresConfig
  | MySQLConfig
  | MongoDBConfig
  | MSSQLConfig
  | SQLiteConfig;

/**
 * Manages test containers (PostgreSQL, MySQL, MongoDB, Redis, etc.) for E2E testing
 */
export class TestContainerManager {
  private containers: StartedTestContainer[] = [];

  /**
   * Setup database container based on type
   */
  async setupDatabaseContainer(
    type: DatabaseType,
    config: DatabaseConfig = {}
  ): Promise<StartedTestContainer | null> {
    const normalizedType = this.normalizeDatabaseType(type);

    switch (normalizedType) {
      case 'postgres':
        return this.setupPostgresContainer(config as PostgresConfig);
      case 'mysql':
        return this.setupMySQLContainer(config as MySQLConfig);
      case 'mongodb':
        return this.setupMongoDBContainer(config as MongoDBConfig);
      case 'mssql':
        return this.setupMSSQLContainer(config as MSSQLConfig);
      case 'sqlite':
        // SQLite doesn't need a container (file-based)
        return null;
      default:
        throw new Error(`Unsupported database type: ${type}`);
    }
  }

  /**
   * Normalize database type aliases
   */
  private normalizeDatabaseType(
    type: DatabaseType
  ): 'postgres' | 'mysql' | 'mongodb' | 'mssql' | 'sqlite' {
    switch (type) {
      case 'postgres':
      case 'postgresql':
        return 'postgres';
      case 'mysql':
      case 'mariadb':
        return 'mysql';
      case 'mongodb':
      case 'mongo':
        return 'mongodb';
      case 'mssql':
        return 'mssql';
      case 'sqlite':
      case 'libsql':
        return 'sqlite';
      default:
        throw new Error(`Unknown database type: ${type}`);
    }
  }

  /**
   * Setup PostgreSQL test container
   */
  async setupPostgresContainer(
    config: PostgresConfig = {}
  ): Promise<StartedTestContainer> {
    const {
      user = 'test_user',
      password = 'test_password',
      database = 'test_db',
      command = ['postgres', '-c', 'log_statement=all']
    } = config;

    const container = await new GenericContainer('postgres:latest')
      .withExposedPorts(5432)
      .withEnvironment({
        POSTGRES_USER: user,
        POSTGRES_PASSWORD: password,
        POSTGRES_DB: database
      })
      .withCommand(command)
      .start();

    this.containers.push(container);
    return container;
  }

  /**
   * Setup MySQL test container
   */
  async setupMySQLContainer(
    config: MySQLConfig = {}
  ): Promise<StartedTestContainer> {
    const {
      user = 'test_user',
      password = 'test_password',
      database = 'test_db',
      rootPassword = 'root_password'
    } = config;

    const container = await new GenericContainer('mysql:8')
      .withExposedPorts(3306)
      .withEnvironment({
        MYSQL_ROOT_PASSWORD: rootPassword,
        MYSQL_DATABASE: database,
        MYSQL_USER: user,
        MYSQL_PASSWORD: password
      })
      .start();

    this.containers.push(container);
    return container;
  }

  /**
   * Setup MongoDB test container
   */
  async setupMongoDBContainer(
    config: MongoDBConfig = {}
  ): Promise<StartedTestContainer> {
    const {
      user = 'test_user',
      password = 'test_password',
      database = 'test_db'
    } = config;

    const container = await new GenericContainer('mongo:latest')
      .withExposedPorts(27017)
      .withEnvironment({
        MONGO_INITDB_ROOT_USERNAME: user,
        MONGO_INITDB_ROOT_PASSWORD: password,
        MONGO_INITDB_DATABASE: database
      })
      .start();

    this.containers.push(container);
    return container;
  }

  /**
   * Setup Microsoft SQL Server test container
   */
  async setupMSSQLContainer(
    config: MSSQLConfig = {}
  ): Promise<StartedTestContainer> {
    const {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      user = 'SA',
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      password = 'Test_Password123!',
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      database = 'test_db',
      saPassword = 'Test_Password123!'
    } = config;

    const container = await new GenericContainer(
      'mcr.microsoft.com/mssql/server:2022-latest'
    )
      .withExposedPorts(1433)
      .withEnvironment({
        ACCEPT_EULA: 'Y',
        SA_PASSWORD: saPassword,
        MSSQL_PID: 'Developer'
      })
      .start();

    // Wait a bit for SQL Server to be ready
    await new Promise((resolve) => setTimeout(resolve, 3000));

    this.containers.push(container);
    return container;
  }

  /**
   * Setup Redis test container
   */
  async setupRedisContainer(
    config: RedisConfig = {}
  ): Promise<StartedTestContainer> {
    const { command = ['redis-server', '--appendonly', 'yes'] } = config;

    const container = await new GenericContainer('redis:latest')
      .withExposedPorts(6379)
      .withCommand(command)
      .start();

    this.containers.push(container);
    return container;
  }

  /**
   * Setup Kafka test container
   */
  async setupKafkaContainer(
    config: KafkaConfig = {}
  ): Promise<StartedTestContainer> {
    const {
      clusterId = 'test-cluster',
      numPartitions = 1,
      replicationFactor = 1,
      env = {}
    } = config;

    const container = await new GenericContainer('confluentinc/cp-kafka:latest')
      .withExposedPorts(9092, 9093)
      .withEnvironment({
        KAFKA_BROKER_ID: '1',
        KAFKA_LISTENER_SECURITY_PROTOCOL_MAP:
          'PLAINTEXT:PLAINTEXT,PLAINTEXT_HOST:PLAINTEXT',
        KAFKA_ADVERTISED_LISTENERS:
          'PLAINTEXT://kafka:29092,PLAINTEXT_HOST://localhost:9092',
        KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR: replicationFactor.toString(),
        KAFKA_TRANSACTION_STATE_LOG_MIN_ISR: '1',
        KAFKA_TRANSACTION_STATE_LOG_REPLICATION_FACTOR: '1',
        KAFKA_GROUP_INITIAL_REBALANCE_DELAY_MS: '0',
        KAFKA_NUM_PARTITIONS: numPartitions.toString(),
        KAFKA_CLUSTER_ID: clusterId,
        KAFKA_PROCESS_ROLES: 'broker,controller',
        KAFKA_NODE_ID: '1',
        KAFKA_CONTROLLER_QUORUM_VOTERS: '1@localhost:9093',
        KAFKA_LISTENERS:
          'PLAINTEXT://0.0.0.0:29092,CONTROLLER://0.0.0.0:9093,PLAINTEXT_HOST://0.0.0.0:9092',
        KAFKA_INTER_BROKER_LISTENER_NAME: 'PLAINTEXT',
        KAFKA_CONTROLLER_LISTENER_NAMES: 'CONTROLLER',
        KAFKA_LOG_DIRS: '/tmp/kraft-combined-logs',
        ...env
      })
      .start();

    // Wait for Kafka to be ready
    await new Promise((resolve) => setTimeout(resolve, 5000));

    this.containers.push(container);
    return container;
  }

  /**
   * Setup MinIO (S3-compatible) test container
   */
  async setupS3Container(config: S3Config = {}): Promise<StartedTestContainer> {
    const {
      rootUser = 'minioadmin',
      rootPassword = 'minioadmin',
      defaultBucket = 'test-bucket',
      region = 'us-east-1'
    } = config;

    const container = await new GenericContainer('minio/minio:latest')
      .withExposedPorts(9000, 9001)
      .withEnvironment({
        MINIO_ROOT_USER: rootUser,
        MINIO_ROOT_PASSWORD: rootPassword,
        MINIO_REGION: region
      })
      .withCommand(['server', '/data', '--console-address', ':9001'])
      .start();

    // Wait for MinIO to be ready
    await new Promise((resolve) => setTimeout(resolve, 2000));

    // Create default bucket if specified
    if (defaultBucket) {
      try {
        // Using MinIO client command via exec
        await container.exec([
          'mc',
          'alias',
          'set',
          'local',
          'http://localhost:9000',
          rootUser,
          rootPassword
        ]);
        await container.exec(['mc', 'mb', `local/${defaultBucket}`]);
      } catch (error) {
        // Bucket creation might fail, but container is still usable
        console.warn('Could not create default bucket:', error);
      }
    }

    this.containers.push(container);
    return container;
  }

  /**
   * Cleanup all containers
   */
  async cleanup(): Promise<void> {
    await Promise.all(
      this.containers.map((container) =>
        container.stop({ remove: true, removeVolumes: true }).catch(() => {
          // Ignore cleanup errors
        })
      )
    );
    this.containers = [];
  }
}
