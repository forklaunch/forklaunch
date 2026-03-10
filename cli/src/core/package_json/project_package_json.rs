use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use serde::{Deserialize, Serialize, Serializer, de::Visitor};
use serde_json::Value;

use crate::constants::Database;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct PackageExport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) types: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default: Option<String>,
}

pub(crate) type PackageExportsMap = HashMap<String, PackageExport>;

#[derive(Debug)]
struct ProjectDependenciesWithProjectName {
    project_dependencies_value: Value,
    app_name: String,
}

impl Serialize for ProjectDependenciesWithProjectName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut deps = self.project_dependencies_value.clone();
        deps["app_name"] = serde_json::to_value(self.app_name.clone()).unwrap();
        deps.serialize(serializer)
    }
}

#[derive(Debug, Serialize, Default, Clone)]
pub(crate) struct ProjectScripts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) build: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) clean: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dev: Option<String>,
    #[serde(rename = "dev:server", skip_serializing_if = "Option::is_none")]
    pub(crate) dev_server: Option<String>,
    #[serde(rename = "dev:worker", skip_serializing_if = "Option::is_none")]
    pub(crate) dev_worker: Option<String>,
    #[serde(rename = "dev:local", skip_serializing_if = "Option::is_none")]
    pub(crate) dev_local: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) docs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lint: Option<String>,
    #[serde(rename = "lint:fix", skip_serializing_if = "Option::is_none")]
    pub(crate) lint_fix: Option<String>,
    #[serde(rename = "migrate:create", skip_serializing_if = "Option::is_none")]
    pub(crate) migrate_create: Option<String>,
    #[serde(rename = "migrate:down", skip_serializing_if = "Option::is_none")]
    pub(crate) migrate_down: Option<String>,
    #[serde(rename = "migrate:init", skip_serializing_if = "Option::is_none")]
    pub(crate) migrate_init: Option<String>,
    #[serde(rename = "migrate:up", skip_serializing_if = "Option::is_none")]
    pub(crate) migrate_up: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) seed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) start: Option<String>,
    #[serde(rename = "start:server", skip_serializing_if = "Option::is_none")]
    pub(crate) start_server: Option<String>,
    #[serde(rename = "start:worker", skip_serializing_if = "Option::is_none")]
    pub(crate) start_worker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) test: Option<String>,
    #[serde(rename = "up:latest", skip_serializing_if = "Option::is_none")]
    pub(crate) up_latest: Option<String>,

    #[serde(flatten)]
    pub(crate) additional_scripts: HashMap<String, String>,
}

impl<'de> Deserialize<'de> for ProjectScripts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ProjectScriptsVisitor;

        impl<'de> Visitor<'de> for ProjectScriptsVisitor {
            type Value = ProjectScripts;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map of script names to commands")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut scripts = ProjectScripts {
                    build: None,
                    clean: None,
                    dev: None,
                    dev_server: None,
                    dev_worker: None,
                    dev_local: None,
                    docs: None,
                    format: None,
                    lint: None,
                    lint_fix: None,
                    migrate_create: None,
                    migrate_down: None,
                    migrate_init: None,
                    migrate_up: None,
                    seed: None,
                    start: None,
                    start_server: None,
                    start_worker: None,
                    test: None,
                    up_latest: None,
                    additional_scripts: HashMap::new(),
                };

                while let Some((key, value)) = access.next_entry::<String, String>()? {
                    match key.as_str() {
                        "build" => scripts.build = Some(value),
                        "clean" => scripts.clean = Some(value),
                        "dev" => scripts.dev = Some(value),
                        "dev:server" => scripts.dev_server = Some(value),
                        "dev:worker" => scripts.dev_worker = Some(value),
                        "dev:local" => scripts.dev_local = Some(value),
                        "docs" => scripts.docs = Some(value),
                        "format" => scripts.format = Some(value),
                        "lint" => scripts.lint = Some(value),
                        "lint:fix" => scripts.lint_fix = Some(value),
                        "migrate:create" => scripts.migrate_create = Some(value),
                        "migrate:down" => scripts.migrate_down = Some(value),
                        "migrate:init" => scripts.migrate_init = Some(value),
                        "migrate:up" => scripts.migrate_up = Some(value),
                        "seed" => scripts.seed = Some(value),
                        "start" => scripts.start = Some(value),
                        "start:server" => scripts.start_server = Some(value),
                        "start:worker" => scripts.start_worker = Some(value),
                        "test" => scripts.test = Some(value),
                        "up:latest" => scripts.up_latest = Some(value),
                        _ => {
                            scripts.additional_scripts.insert(key, value);
                        }
                    }
                }

                Ok(scripts)
            }
        }

        deserializer.deserialize_map(ProjectScriptsVisitor)
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ProjectDependencies {
    pub(crate) app_name: String,
    pub(crate) databases: HashSet<Database>,
    pub(crate) app_core: Option<String>,
    pub(crate) app_monitoring: Option<String>,
    pub(crate) app_client_sdk: Option<String>,
    pub(crate) app_billing: Option<String>,
    pub(crate) app_iam: Option<String>,
    pub(crate) forklaunch_better_auth_mikro_orm_fork: Option<String>,
    pub(crate) forklaunch_common: Option<String>,
    pub(crate) forklaunch_core: Option<String>,
    pub(crate) forklaunch_express: Option<String>,
    pub(crate) forklaunch_hyper_express: Option<String>,
    pub(crate) forklaunch_implementation_billing_base: Option<String>,
    pub(crate) forklaunch_implementation_billing_stripe: Option<String>,
    pub(crate) forklaunch_interfaces_billing: Option<String>,
    pub(crate) forklaunch_implementation_iam_base: Option<String>,
    pub(crate) forklaunch_interfaces_iam: Option<String>,
    pub(crate) forklaunch_implementation_worker_bullmq: Option<String>,
    pub(crate) forklaunch_implementation_worker_redis: Option<String>,
    pub(crate) forklaunch_implementation_worker_database: Option<String>,
    pub(crate) forklaunch_implementation_worker_kafka: Option<String>,
    pub(crate) forklaunch_interfaces_worker: Option<String>,
    pub(crate) forklaunch_infrastructure_redis: Option<String>,
    pub(crate) forklaunch_infrastructure_s3: Option<String>,
    pub(crate) forklaunch_internal: Option<String>,
    pub(crate) forklaunch_universal_sdk: Option<String>,
    pub(crate) forklaunch_validator: Option<String>,
    pub(crate) mikro_orm_core: Option<String>,
    pub(crate) mikro_orm_migrations: Option<String>,
    pub(crate) mikro_orm_database: Option<String>,
    pub(crate) mikro_orm_reflection: Option<String>,
    pub(crate) mikro_orm_seeder: Option<String>,
    pub(crate) opentelemetry_api: Option<String>,
    pub(crate) typebox: Option<String>,
    pub(crate) ajv: Option<String>,
    pub(crate) better_auth: Option<String>,
    pub(crate) better_sqlite3: Option<String>,
    pub(crate) bullmq: Option<String>,
    pub(crate) dotenv: Option<String>,
    pub(crate) jose: Option<String>,
    pub(crate) sqlite3: Option<String>,
    pub(crate) stripe: Option<String>,
    pub(crate) uuid: Option<String>,
    pub(crate) zod: Option<String>,
    pub(crate) pino: Option<String>,
    pub(crate) ioredis: Option<String>,

    pub(crate) additional_deps: HashMap<String, String>,
}

impl Serialize for ProjectDependencies {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;

        if let Some(ref v) = self.app_core {
            map.serialize_entry(&format!("@{}/core", self.app_name), v)?;
        }
        if let Some(ref v) = self.app_monitoring {
            map.serialize_entry(&format!("@{}/monitoring", self.app_name), v)?;
        }
        if let Some(ref v) = self.app_client_sdk {
            map.serialize_entry(&format!("@{}/client-sdk", self.app_name), v)?;
        }
        if let Some(ref v) = self.app_billing {
            map.serialize_entry(&format!("@{}/billing", self.app_name), v)?;
        }
        if let Some(ref v) = self.app_iam {
            map.serialize_entry(&format!("@{}/iam", self.app_name), v)?;
        }
        if let Some(ref v) = self.forklaunch_better_auth_mikro_orm_fork {
            map.serialize_entry("@forklaunch/better-auth-mikro-orm-fork", v)?;
        }
        if let Some(ref v) = self.forklaunch_common {
            map.serialize_entry("@forklaunch/common", v)?;
        }
        if let Some(ref v) = self.forklaunch_core {
            map.serialize_entry("@forklaunch/core", v)?;
        }
        if let Some(ref v) = self.forklaunch_express {
            map.serialize_entry("@forklaunch/express", v)?;
        }
        if let Some(ref v) = self.forklaunch_hyper_express {
            map.serialize_entry("@forklaunch/hyper-express", v)?;
        }
        if let Some(ref v) = self.forklaunch_implementation_billing_base {
            map.serialize_entry("@forklaunch/implementation-billing-base", v)?;
        }
        if let Some(ref v) = self.forklaunch_implementation_billing_stripe {
            map.serialize_entry("@forklaunch/implementation-billing-stripe", v)?;
        }
        if let Some(ref v) = self.forklaunch_interfaces_billing {
            map.serialize_entry("@forklaunch/interfaces-billing", v)?;
        }
        if let Some(ref v) = self.forklaunch_implementation_iam_base {
            map.serialize_entry("@forklaunch/implementation-iam-base", v)?;
        }
        if let Some(ref v) = self.forklaunch_interfaces_iam {
            map.serialize_entry("@forklaunch/interfaces-iam", v)?;
        }
        if let Some(ref v) = self.forklaunch_implementation_worker_bullmq {
            map.serialize_entry("@forklaunch/implementation-worker-bullmq", v)?;
        }
        if let Some(ref v) = self.forklaunch_implementation_worker_redis {
            map.serialize_entry("@forklaunch/implementation-worker-redis", v)?;
        }
        if let Some(ref v) = self.forklaunch_implementation_worker_database {
            map.serialize_entry("@forklaunch/implementation-worker-database", v)?;
        }
        if let Some(ref v) = self.forklaunch_implementation_worker_kafka {
            map.serialize_entry("@forklaunch/implementation-worker-kafka", v)?;
        }
        if let Some(ref v) = self.forklaunch_infrastructure_redis {
            map.serialize_entry("@forklaunch/infrastructure-redis", v)?;
        }
        if let Some(ref v) = self.forklaunch_infrastructure_s3 {
            map.serialize_entry("@forklaunch/infrastructure-s3", v)?;
        }
        if let Some(ref v) = self.forklaunch_interfaces_worker {
            map.serialize_entry("@forklaunch/interfaces-worker", v)?;
        }
        if let Some(ref v) = self.forklaunch_internal {
            map.serialize_entry("@forklaunch/internal", v)?;
        }
        if let Some(ref v) = self.forklaunch_universal_sdk {
            map.serialize_entry("@forklaunch/universal-sdk", v)?;
        }
        if let Some(ref v) = self.forklaunch_validator {
            map.serialize_entry("@forklaunch/validator", v)?;
        }
        if let Some(ref v) = self.mikro_orm_core {
            map.serialize_entry("@mikro-orm/core", v)?;
        }
        if let Some(ref v) = self.mikro_orm_migrations {
            for database in &self.databases {
                if database == &Database::MongoDB {
                    map.serialize_entry("@mikro-orm/migrations-mongodb", v)?;
                } else {
                    map.serialize_entry("@mikro-orm/migrations", v)?;
                }
            }
        }
        if let Some(ref v) = self.mikro_orm_database {
            for database in &self.databases {
                map.serialize_entry(&format!("@mikro-orm/{}", database.to_string()), v)?;
            }
        }
        if let Some(ref v) = self.mikro_orm_reflection {
            map.serialize_entry("@mikro-orm/reflection", v)?;
        }
        if let Some(ref v) = self.mikro_orm_seeder {
            map.serialize_entry("@mikro-orm/seeder", v)?;
        }
        if let Some(ref v) = self.opentelemetry_api {
            map.serialize_entry("@opentelemetry/api", v)?;
        }
        if let Some(ref v) = self.typebox {
            map.serialize_entry("@sinclair/typebox", v)?;
        }
        if let Some(ref v) = self.ajv {
            map.serialize_entry("ajv", v)?;
        }
        if let Some(ref v) = self.better_auth {
            map.serialize_entry("better-auth", v)?;
        }
        if let Some(ref v) = self.bullmq {
            map.serialize_entry("bullmq", v)?;
        }
        if let Some(ref v) = self.better_sqlite3 {
            map.serialize_entry("better-sqlite3", v)?;
        }
        if let Some(ref v) = self.dotenv {
            map.serialize_entry("dotenv", v)?;
        }
        if let Some(ref v) = self.jose {
            map.serialize_entry("jose", v)?;
        }
        if let Some(ref v) = self.sqlite3 {
            map.serialize_entry("sqlite3", v)?;
        }
        if let Some(ref v) = self.stripe {
            map.serialize_entry("stripe", v)?;
        }
        if let Some(ref v) = self.uuid {
            map.serialize_entry("uuid", v)?;
        }
        if let Some(ref v) = self.zod {
            map.serialize_entry("zod", v)?;
        }
        if let Some(ref v) = self.pino {
            map.serialize_entry("pino", v)?;
        }
        if let Some(ref v) = self.ioredis {
            map.serialize_entry("ioredis", v)?;
        }

        for (key, value) in &self.additional_deps {
            map.serialize_entry(key, value)?;
        }

        map.end()
    }
}

impl<'de> Deserialize<'de> for ProjectDependencies {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use std::fmt;

        use serde::de::{MapAccess, Visitor};

        struct DependenciesVisitor;

        impl<'de> Visitor<'de> for DependenciesVisitor {
            type Value = ProjectDependencies;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map of dependencies")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut deps = ProjectDependencies::default();

                while let Some((key, value)) = access.next_entry::<String, String>()? {
                    if key == "app_name" {
                        continue;
                    }
                    if deps.app_name.len() > 0
                        && key.starts_with(&format!("@{}", deps.app_name))
                        && key.ends_with("core")
                    {
                        deps.app_core = Some(value);
                        continue;
                    }
                    if deps.app_name.len() > 0
                        && key.starts_with(&format!("@{}", deps.app_name))
                        && key.ends_with("monitoring")
                    {
                        deps.app_monitoring = Some(value);
                        continue;
                    }
                    if deps.app_name.len() > 0
                        && key.starts_with(&format!("@{}", deps.app_name))
                        && key.ends_with("client-sdk")
                    {
                        deps.app_client_sdk = Some(value);
                        continue;
                    }
                    if deps.app_name.len() > 0
                        && key.starts_with(&format!("@{}", deps.app_name))
                        && key.ends_with("billing")
                    {
                        deps.app_billing = Some(value);
                        continue;
                    }
                    if deps.app_name.len() > 0
                        && key.starts_with(&format!("@{}", deps.app_name))
                        && key.ends_with("iam")
                    {
                        deps.app_iam = Some(value);
                        continue;
                    }
                    if key.starts_with("@mikro-orm") && key.ends_with("mongodb") {
                        deps.databases.insert(Database::MongoDB);
                        if key.contains("migrations") {
                            deps.mikro_orm_migrations = Some(value);
                        } else {
                            deps.mikro_orm_database = Some(value);
                        }
                        continue;
                    }
                    if key.starts_with("@mikro-orm") && key.ends_with("postgresql") {
                        deps.databases.insert(Database::PostgreSQL);
                        if key.contains("migrations") {
                            deps.mikro_orm_migrations = Some(value);
                        } else {
                            deps.mikro_orm_database = Some(value);
                        }
                        continue;
                    }
                    if key.starts_with("@mikro-orm") && key.ends_with("mysql") {
                        deps.databases.insert(Database::MySQL);
                        if key.contains("migrations") {
                            deps.mikro_orm_migrations = Some(value);
                        } else {
                            deps.mikro_orm_database = Some(value);
                        }
                        continue;
                    }
                    if key.starts_with("@mikro-orm") && key.ends_with("mariadb") {
                        deps.databases.insert(Database::MariaDB);
                        if key.contains("migrations") {
                            deps.mikro_orm_migrations = Some(value);
                        } else {
                            deps.mikro_orm_database = Some(value);
                        }
                        continue;
                    }
                    if key.starts_with("@mikro-orm") && key.ends_with("sqlite") {
                        deps.databases.insert(Database::SQLite);
                        deps.mikro_orm_database = Some(value);
                        continue;
                    }
                    if key.starts_with("@mikro-orm") && key.ends_with("better-sqlite") {
                        deps.databases.insert(Database::BetterSQLite);
                        deps.better_sqlite3 = Some(value);
                        continue;
                    }
                    if key.starts_with("@mikro-orm") && key.ends_with("libsql") {
                        deps.databases.insert(Database::LibSQL);
                        deps.mikro_orm_database = Some(value);
                        continue;
                    }
                    if key.starts_with("@mikro-orm") && key.ends_with("mssql") {
                        deps.databases.insert(Database::MsSQL);
                        deps.mikro_orm_database = Some(value);
                        continue;
                    }
                    if key.starts_with("@mikro-orm") && key.ends_with("migrations") {
                        deps.mikro_orm_migrations = Some(value);
                        continue;
                    }

                    match key.as_str() {
                        "@forklaunch/better-auth-mikro-orm-fork" => {
                            deps.forklaunch_better_auth_mikro_orm_fork = Some(value);
                        }
                        "@forklaunch/common" => deps.forklaunch_common = Some(value),
                        "@forklaunch/core" => deps.forklaunch_core = Some(value),
                        "@forklaunch/express" => deps.forklaunch_express = Some(value),
                        "@forklaunch/hyper-express" => deps.forklaunch_hyper_express = Some(value),
                        "@forklaunch/implementation-billing-base" => {
                            deps.forklaunch_implementation_billing_base = Some(value)
                        }
                        "@forklaunch/implementation-billing-stripe" => {
                            deps.forklaunch_implementation_billing_stripe = Some(value)
                        }
                        "@forklaunch/interfaces-billing" => {
                            deps.forklaunch_interfaces_billing = Some(value)
                        }
                        "@forklaunch/implementation-iam-base" => {
                            deps.forklaunch_implementation_iam_base = Some(value)
                        }
                        "@forklaunch/interfaces-iam" => {
                            deps.forklaunch_interfaces_iam = Some(value)
                        }
                        "@forklaunch/implementation-worker-bullmq" => {
                            deps.forklaunch_implementation_worker_bullmq = Some(value)
                        }
                        "@forklaunch/implementation-worker-redis" => {
                            deps.forklaunch_implementation_worker_redis = Some(value)
                        }
                        "@forklaunch/implementation-worker-database" => {
                            deps.forklaunch_implementation_worker_database = Some(value)
                        }
                        "@forklaunch/implementation-worker-kafka" => {
                            deps.forklaunch_implementation_worker_kafka = Some(value)
                        }
                        "@forklaunch/infrastructure-redis" => {
                            deps.forklaunch_infrastructure_redis = Some(value)
                        }
                        "@forklaunch/infrastructure-s3" => {
                            deps.forklaunch_infrastructure_s3 = Some(value)
                        }
                        "@forklaunch/interfaces-worker" => {
                            deps.forklaunch_interfaces_worker = Some(value)
                        }
                        "@forklaunch/internal" => deps.forklaunch_internal = Some(value),
                        "@forklaunch/universal-sdk" => deps.forklaunch_universal_sdk = Some(value),
                        "@forklaunch/validator" => deps.forklaunch_validator = Some(value),
                        "@mikro-orm/core" => deps.mikro_orm_core = Some(value),
                        "@mikro-orm/reflection" => deps.mikro_orm_reflection = Some(value),
                        "@mikro-orm/seeder" => deps.mikro_orm_seeder = Some(value),
                        "@opentelemetry/api" => deps.opentelemetry_api = Some(value),
                        "@sinclair/typebox" => deps.typebox = Some(value),
                        "ajv" => deps.ajv = Some(value),
                        "better-auth" => deps.better_auth = Some(value),
                        "bullmq" => deps.bullmq = Some(value),
                        "better-sqlite3" => deps.better_sqlite3 = Some(value),
                        "dotenv" => deps.dotenv = Some(value),
                        "jose" => deps.jose = Some(value),
                        "sqlite3" => deps.sqlite3 = Some(value),
                        "stripe" => deps.stripe = Some(value),
                        "uuid" => deps.uuid = Some(value),
                        "zod" => deps.zod = Some(value),
                        "pino" => deps.pino = Some(value),
                        "ioredis" => deps.ioredis = Some(value),
                        _ => {
                            deps.additional_deps.insert(key, value);
                        }
                    }
                }

                Ok(deps)
            }
        }

        deserializer.deserialize_map(DependenciesVisitor)
    }
}

#[derive(Debug, Serialize, Default, Clone)]
pub(crate) struct ProjectDevDependencies {
    #[serde(rename = "@biomejs/biome", skip_serializing_if = "Option::is_none")]
    pub(crate) biome: Option<String>,
    #[serde(rename = "@eslint/js", skip_serializing_if = "Option::is_none")]
    pub(crate) eslint_js: Option<String>,
    #[serde(
        rename = "@forklaunch/testing",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) testing: Option<String>,
    #[serde(rename = "@mikro-orm/cli", skip_serializing_if = "Option::is_none")]
    pub(crate) mikro_orm_cli: Option<String>,
    #[serde(rename = "@types/express", skip_serializing_if = "Option::is_none")]
    pub(crate) types_express: Option<String>,
    #[serde(rename = "@types/jest", skip_serializing_if = "Option::is_none")]
    pub(crate) types_jest: Option<String>,
    #[serde(
        rename = "@types/express-serve-static-core",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) types_express_serve_static_core: Option<String>,
    #[serde(rename = "@types/qs", skip_serializing_if = "Option::is_none")]
    pub(crate) types_qs: Option<String>,
    #[serde(rename = "@types/uuid", skip_serializing_if = "Option::is_none")]
    pub(crate) types_uuid: Option<String>,
    #[serde(rename = "@types/pino", skip_serializing_if = "Option::is_none")]
    pub(crate) types_pino: Option<String>,
    #[serde(rename = "@types/ioredis", skip_serializing_if = "Option::is_none")]
    pub(crate) types_ioredis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) eslint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) oxlint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prettier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tsx: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pino: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ioredis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) typedoc: Option<String>,
    #[serde(rename = "typescript-eslint", skip_serializing_if = "Option::is_none")]
    pub(crate) typescript_eslint: Option<String>,

    #[serde(flatten)]
    pub(crate) additional_deps: HashMap<String, String>,
}

impl<'de> Deserialize<'de> for ProjectDevDependencies {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ProjectDevDependenciesVisitor;

        impl<'de> Visitor<'de> for ProjectDevDependenciesVisitor {
            type Value = ProjectDevDependencies;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map of dev dependency names to versions")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut deps = ProjectDevDependencies {
                    biome: None,
                    eslint_js: None,
                    testing: None,
                    mikro_orm_cli: None,
                    types_express: None,
                    types_express_serve_static_core: None,
                    types_jest: None,
                    types_qs: None,
                    types_uuid: None,
                    types_pino: None,
                    types_ioredis: None,
                    eslint: None,
                    oxlint: None,
                    prettier: None,
                    tsx: None,
                    pino: None,
                    ioredis: None,
                    typedoc: None,
                    typescript_eslint: None,
                    additional_deps: HashMap::new(),
                };

                while let Some((key, value)) = access.next_entry::<String, String>()? {
                    match key.as_str() {
                        "@biomejs/biome" => deps.biome = Some(value),
                        "@eslint/js" => deps.eslint_js = Some(value),
                        "@forklaunch/testing" => deps.testing = Some(value),
                        "@mikro-orm/cli" => deps.mikro_orm_cli = Some(value),
                        "@types/express" => deps.types_express = Some(value),
                        "@types/express-serve-static-core" => {
                            deps.types_express_serve_static_core = Some(value)
                        }
                        "@types/jest" => deps.types_jest = Some(value),
                        "@types/qs" => deps.types_qs = Some(value),
                        "@types/uuid" => deps.types_uuid = Some(value),
                        "@types/pino" => deps.types_pino = Some(value),
                        "@types/ioredis" => deps.types_ioredis = Some(value),
                        "eslint" => deps.eslint = Some(value),
                        "oxlint" => deps.oxlint = Some(value),
                        "prettier" => deps.prettier = Some(value),
                        "tsx" => deps.tsx = Some(value),
                        "pino" => deps.pino = Some(value),
                        "ioredis" => deps.ioredis = Some(value),
                        "typedoc" => deps.typedoc = Some(value),
                        "typescript-eslint" => deps.typescript_eslint = Some(value),
                        _ => {
                            deps.additional_deps.insert(key, value);
                        }
                    }
                }

                Ok(deps)
            }
        }

        deserializer.deserialize_map(ProjectDevDependenciesVisitor)
    }
}

pub(crate) static MIKRO_ORM_CONFIG_PATHS: &[&str] =
    &["./mikro-orm.config.ts", "./dist/mikro-orm.config.js"];

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct ProjectMikroOrm {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manifest_paths: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Default, Clone)]
pub(crate) struct ProjectPackageJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) main: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) types: Option<String>,
    #[serde(rename = "typesVersions", skip_serializing_if = "Option::is_none")]
    pub(crate) types_versions: Option<HashMap<String, HashMap<String, Vec<String>>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exports: Option<PackageExportsMap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scripts: Option<ProjectScripts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dependencies: Option<ProjectDependencies>,
    #[serde(rename = "devDependencies", skip_serializing_if = "Option::is_none")]
    pub(crate) dev_dependencies: Option<ProjectDevDependencies>,
    #[serde(rename = "mikro-orm", skip_serializing_if = "Option::is_none")]
    pub(crate) mikro_orm: Option<ProjectMikroOrm>,

    #[serde(flatten)]
    pub(crate) additional_entries: HashMap<String, serde_json::Value>,
}

impl<'de> Deserialize<'de> for ProjectPackageJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ProjectPackageJsonVisitor;

        impl<'de> Visitor<'de> for ProjectPackageJsonVisitor {
            type Value = ProjectPackageJson;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a package.json object")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut package = ProjectPackageJson {
                    name: None,
                    version: None,
                    r#type: None,
                    description: None,
                    keywords: None,
                    license: None,
                    author: None,
                    main: None,
                    types: None,
                    types_versions: None,
                    exports: None,
                    scripts: None,
                    dependencies: None,
                    dev_dependencies: None,
                    mikro_orm: None,
                    additional_entries: HashMap::new(),
                };

                while let Some((key, value)) = access.next_entry::<String, serde_json::Value>()? {
                    match key.as_str() {
                        "name" => {
                            package.name = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "version" => {
                            package.version = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "type" => {
                            package.r#type = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "description" => {
                            package.description = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "keywords" => {
                            package.keywords = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "license" => {
                            package.license = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "author" => {
                            package.author = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "main" => {
                            package.main = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "types" => {
                            package.types = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "typesVersions" => {
                            package.types_versions = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "exports" => {
                            package.exports = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "scripts" => {
                            package.scripts = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "dependencies" => {
                            package.dependencies = Some(
                                serde_json::from_value::<ProjectDependencies>(
                                    serde_json::to_value(ProjectDependenciesWithProjectName {
                                        project_dependencies_value: value,
                                        app_name: package
                                            .name
                                            .clone()
                                            .unwrap()
                                            .split('/')
                                            .collect::<Vec<&str>>()[0]
                                            .to_string(),
                                    })
                                    .map_err(serde::de::Error::custom)?,
                                )
                                .map_err(serde::de::Error::custom)?,
                            )
                        }
                        "devDependencies" => {
                            package.dev_dependencies = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "mikro-orm" => {
                            package.mikro_orm = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        _ => {
                            package.additional_entries.insert(key, value);
                        }
                    }
                }

                Ok(package)
            }
        }

        deserializer.deserialize_map(ProjectPackageJsonVisitor)
    }
}
