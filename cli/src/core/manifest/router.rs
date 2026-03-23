use convert_case::{Case, Casing};
use ramhorns::Content;
use serde::{Deserialize, Serialize};

use super::{InitializableManifestConfig, InitializableManifestConfigMetadata, ProjectType};
use crate::{config_struct, constants::Database, core::database::get_db_driver};

config_struct!(
    #[derive(Debug, Content, Serialize, Clone)]
    pub(crate) struct RouterManifestData {
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) router_name: String,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) camel_case_name: String,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) pascal_case_name: String,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) kebab_case_name: String,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) title_case_name: String,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_database_enabled: bool,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) database: String,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) db_driver: String,

        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_postgres: bool,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_mongo: bool,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_mysql: bool,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_mariadb: bool,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_mssql: bool,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_sqlite: bool,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_better_sqlite: bool,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_libsql: bool,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_in_memory_database: bool,

        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_worker: bool,

        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_cache_enabled: bool,
        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) is_s3_enabled: bool,

        #[serde(skip_serializing, skip_deserializing)]
        pub(crate) otel_token: String,
    }
);

impl InitializableManifestConfig for RouterManifestData {
    fn initialize(&self, metadata: InitializableManifestConfigMetadata) -> Self {
        let router_metadata = match metadata {
            InitializableManifestConfigMetadata::Router(router_metadata) => router_metadata,
            _ => unreachable!(),
        };
        let project_name = router_metadata.project_name.clone();
        let router_name = router_metadata.router_name.unwrap().clone();
        let project_entry = self
            .projects
            .iter()
            .find(|p| p.name == project_name)
            .unwrap();
        let database = project_entry
            .resources
            .as_ref()
            .unwrap()
            .database
            .clone()
            .unwrap();
        let parsed_database = database.parse::<Database>().unwrap();

        Self {
            router_name: router_name.clone(),
            camel_case_name: router_name.to_case(Case::Camel),
            pascal_case_name: router_name.to_case(Case::Pascal),
            kebab_case_name: router_name.to_case(Case::Kebab),
            database: database.clone(),
            db_driver: get_db_driver(&parsed_database),

            is_postgres: parsed_database == Database::PostgreSQL,
            is_mongo: parsed_database == Database::MongoDB,
            is_mysql: parsed_database == Database::MySQL,
            is_mariadb: parsed_database == Database::MariaDB,
            is_mssql: parsed_database == Database::MsSQL,
            is_sqlite: parsed_database == Database::SQLite,
            is_better_sqlite: parsed_database == Database::BetterSQLite,
            is_libsql: parsed_database == Database::LibSQL,
            is_in_memory_database: parsed_database == Database::SQLite
                || parsed_database == Database::BetterSQLite
                || parsed_database == Database::LibSQL,

            is_worker: matches!(project_entry.r#type, ProjectType::Worker),

            is_cache_enabled: project_entry.resources.as_ref().unwrap().cache.is_some(),
            is_s3_enabled: project_entry
                .resources
                .as_ref()
                .unwrap()
                .object_store
                .is_some(),

            otel_token: "OtelCollector".to_string(),

            ..self.clone()
        }
    }
}
