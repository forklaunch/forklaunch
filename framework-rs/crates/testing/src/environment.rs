use std::{collections::BTreeMap, error::Error, fmt, str::FromStr};

/// Database families accepted by TypeScript test environment configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    Postgres,
    Mysql,
    MongoDb,
    MsSql,
    Sqlite,
}

impl FromStr for DatabaseType {
    type Err = TestEnvironmentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "postgres" | "postgresql" => Ok(Self::Postgres),
            "mysql" | "mariadb" => Ok(Self::Mysql),
            "mongodb" | "mongo" => Ok(Self::MongoDb),
            "mssql" => Ok(Self::MsSql),
            "sqlite" | "libsql" => Ok(Self::Sqlite),
            _ => Err(TestEnvironmentError::UnsupportedDatabase(value.to_owned())),
        }
    }
}

impl DatabaseType {
    #[must_use]
    pub const fn container_port(self) -> Option<u16> {
        match self {
            Self::Postgres => Some(5432),
            Self::Mysql => Some(3306),
            Self::MongoDb => Some(27017),
            Self::MsSql => Some(1433),
            Self::Sqlite => None,
        }
    }
}

/// Host and published ports supplied by an already-started test container.
/// This value does not start, own or remove a container.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestContainerEndpoint {
    pub host: String,
    pub mapped_ports: BTreeMap<u16, u16>,
}

impl TestContainerEndpoint {
    fn port(&self, service: &'static str, port: u16) -> Result<u16, TestEnvironmentError> {
        self.mapped_ports
            .get(&port)
            .copied()
            .ok_or(TestEnvironmentError::MissingPort { service, port })
    }
}

#[derive(Debug, Clone, Default)]
pub struct TestEnvConfig {
    pub database: Option<TestContainerEndpoint>,
    pub database_type: Option<DatabaseType>,
    pub redis: Option<TestContainerEndpoint>,
    pub kafka: Option<TestContainerEndpoint>,
    pub s3: Option<TestContainerEndpoint>,
    /// None selects the TS test-only default; an empty string stays empty.
    pub hmac_secret: Option<String>,
    pub custom_vars: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestEnvironmentError {
    UnsupportedDatabase(String),
    MissingPort { service: &'static str, port: u16 },
}

impl fmt::Display for TestEnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedDatabase(value) => {
                write!(formatter, "Unsupported database type: {value}")
            }
            Self::MissingPort { service, port } => {
                write!(formatter, "Missing mapped {service} port: {port}")
            }
        }
    }
}
impl Error for TestEnvironmentError {}

/// Build TypeScript-compatible test environment values on top of an explicit
/// baseline. Pass the returned map to `std::process::Command::envs` or a native
/// configuration loader. The process-global environment is never changed.
/// Unrelated/stale baseline keys remain, matching TypeScript's assignment behavior;
/// custom variables override every generated default, including empty values.
///
/// # Errors
/// Returns a missing-port error if a configured container has no required mapping.
/// No partially updated environment is returned on failure.
pub fn setup_test_environment(
    config: &TestEnvConfig,
    mut environment: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, TestEnvironmentError> {
    if let Some(database_type) = config.database_type {
        environment.insert("DB_NAME".into(), "test_db".into());
        if let Some(port) = database_type.container_port() {
            if let Some(database) = &config.database {
                environment.insert("DB_HOST".into(), database.host.clone());
                environment.insert(
                    "DB_PORT".into(),
                    database.port("database", port)?.to_string(),
                );
                let (user, password) = if database_type == DatabaseType::MsSql {
                    ("SA", "Test_Password123!")
                } else {
                    ("test_user", "test_password")
                };
                environment.insert("DB_USER".into(), user.into());
                environment.insert("DB_PASSWORD".into(), password.into());
            }
        } else {
            environment.insert("DB_PATH".into(), ":memory:".into());
        }
    }
    if let Some(redis) = &config.redis {
        let port = redis.port("redis", 6379)?;
        environment.insert("REDIS_URL".into(), format!("redis://{}:{port}", redis.host));
        environment.insert("REDIS_HOST".into(), redis.host.clone());
        environment.insert("REDIS_PORT".into(), port.to_string());
    }
    if let Some(kafka) = &config.kafka {
        let port = kafka.port("kafka", 9092)?;
        environment.insert("KAFKA_BROKERS".into(), format!("{}:{port}", kafka.host));
        environment.insert("KAFKA_CLIENT_ID".into(), "test-client".into());
        environment.insert("KAFKA_GROUP_ID".into(), "test-group".into());
    }
    if let Some(s3) = &config.s3 {
        let port = s3.port("s3", 9000)?;
        environment.insert("S3_ENDPOINT".into(), format!("http://{}:{port}", s3.host));
        for (key, value) in [
            ("S3_ACCESS_KEY_ID", "minioadmin"),
            ("S3_SECRET_ACCESS_KEY", "minioadmin"),
            ("S3_REGION", "us-east-1"),
            ("S3_BUCKET", "test-bucket"),
            ("S3_FORCE_PATH_STYLE", "true"),
        ] {
            environment.insert(key.into(), value.into());
        }
    }
    environment.insert(
        "HMAC_SECRET_KEY".into(),
        config
            .hmac_secret
            .as_deref()
            .unwrap_or("test-secret-key")
            .into(),
    );
    for (key, value) in [
        (
            "JWKS_PUBLIC_KEY_URL",
            "http://localhost:3000/.well-known/jwks.json",
        ),
        ("OTEL_SERVICE_NAME", "test-service"),
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4318"),
        ("HOST", "localhost"),
        ("PORT", "3000"),
        ("NODE_ENV", "test"),
        ("VERSION", "v1"),
        ("DOCS_PATH", "/docs"),
        ("OTEL_LEVEL", "info"),
        ("DOTENV_FILE_PATH", ".env.test"),
    ] {
        environment.insert(key.into(), value.into());
    }
    environment.extend(config.custom_vars.clone());
    Ok(environment)
}
