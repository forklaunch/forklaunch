use std::path::Path;

use crate::{choice, core::choices::Choice};

// --- Environment Detection ---------------------------------------------------
// If the binary is running from target/debug or target/release, use dev URLs.
// Otherwise (installed binary), use prod URLs.
// All URLs can be overridden via environment variables.

const DEV_PLATFORM_MANAGEMENT_API_URL: &str = "http://localhost:8004";
const DEV_OBSERVABILITY_API_URL: &str = "http://localhost:8007";
const DEV_IAM_API_URL: &str = "http://localhost:8001";
const DEV_BILLING_API_URL: &str = "http://localhost:8000";
const DEV_PLATFORM_UI_URL: &str = "http://localhost:5173";
const DEV_STUDIO_ORCHESTRATOR_API_URL: &str = "http://localhost:8008";
const DEV_RESOURCE_MANAGEMENT_API_URL: &str = "http://localhost:8005";
const DEV_DEVELOPER_TOOLS_API_URL: &str = "http://localhost:8006";

const PROD_PLATFORM_MANAGEMENT_API_URL: &str = "https://platform-management.forklaunch.com";
const PROD_OBSERVABILITY_API_URL: &str = "https://observability-api.forklaunch.com";
const PROD_IAM_API_URL: &str = "https://iam.forklaunch.com";
const PROD_BILLING_API_URL: &str = "https://billing.forklaunch.com";
const PROD_PLATFORM_UI_URL: &str = "https://forklaunch.com";
const PROD_STUDIO_ORCHESTRATOR_API_URL: &str = "https://studio-orchestrator.forklaunch.com";
const PROD_RESOURCE_MANAGEMENT_API_URL: &str = "https://resource-management.forklaunch.com";
// NOT A STABLE ALIAS, unlike the other services above: developer-tools has
// no custom domain configured yet, so this is the raw per-deployment
// hostname (`<service>.<app>-<env>-<region>-<hash>.app.forklaunch.com`) for
// ForkLaunch's own production app, confirmed reachable (`/health` and
// `/api/v1/openapi` both 200). The `-0aef7f56` hash is tied to this specific
// deployment — if the forklaunch app is ever redeployed fresh or migrated,
// this will need updating (or, better, developer-tools gets a real alias
// like the others). Override via `FORKLAUNCH_DEVELOPER_TOOLS_API_URL` if it
// breaks before this constant is fixed.
const PROD_DEVELOPER_TOOLS_API_URL: &str =
    "https://developer-tools.forklaunch-production-us-west-2-0aef7f56.app.forklaunch.com";

pub(crate) fn is_dev_build() -> bool {
    std::env::current_exe()
        .ok()
        .map(|path| {
            path.components()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|window| {
                    window[0].as_os_str() == "target"
                        && (window[1].as_os_str() == "debug" || window[1].as_os_str() == "release")
                })
        })
        .unwrap_or(false)
}

// Manifest schema 1.1.0 adds `optional` to each environment variable
// requirement. The platform's ingestion rejects a schema version it does not
// know rather than degrading, so this must not move ahead of the platform —
// forklaunch-platform#389 landed 1.1.0 support before this bump.
pub(crate) const RELEASE_MANIFEST_SCHEMA_VERSION: &str = "1.1.0";

pub(crate) fn get_platform_management_api_url() -> String {
    std::env::var("FORKLAUNCH_PLATFORM_MANAGEMENT_API_URL").unwrap_or_else(|_| {
        if is_dev_build() {
            DEV_PLATFORM_MANAGEMENT_API_URL
        } else {
            PROD_PLATFORM_MANAGEMENT_API_URL
        }
        .to_string()
    })
}

pub(crate) fn get_observability_api_url() -> String {
    std::env::var("FORKLAUNCH_OBSERVABILITY_API_URL")
        .ok()
        .map(|api_url| api_url.trim().to_string())
        .filter(|api_url| !api_url.is_empty())
        .unwrap_or_else(|| {
            if is_dev_build() {
                DEV_OBSERVABILITY_API_URL
            } else {
                PROD_OBSERVABILITY_API_URL
            }
            .to_string()
        })
}

pub(crate) fn get_iam_api_url() -> String {
    std::env::var("FORKLAUNCH_IAM_API_URL").unwrap_or_else(|_| {
        if is_dev_build() {
            DEV_IAM_API_URL
        } else {
            PROD_IAM_API_URL
        }
        .to_string()
    })
}

pub(crate) fn get_billing_api_url() -> String {
    std::env::var("FORKLAUNCH_BILLING_API_URL").unwrap_or_else(|_| {
        if is_dev_build() {
            DEV_BILLING_API_URL
        } else {
            PROD_BILLING_API_URL
        }
        .to_string()
    })
}

pub(crate) fn get_developer_tools_api_url() -> String {
    std::env::var("FORKLAUNCH_DEVELOPER_TOOLS_API_URL").unwrap_or_else(|_| {
        if is_dev_build() {
            DEV_DEVELOPER_TOOLS_API_URL
        } else {
            PROD_DEVELOPER_TOOLS_API_URL
        }
        .to_string()
    })
}

pub(crate) fn get_platform_ui_url() -> String {
    std::env::var("FORKLAUNCH_PLATFORM_UI_URL").unwrap_or_else(|_| {
        if is_dev_build() {
            DEV_PLATFORM_UI_URL
        } else {
            PROD_PLATFORM_UI_URL
        }
        .to_string()
    })
}

pub(crate) fn get_resource_management_api_url() -> String {
    std::env::var("FORKLAUNCH_RESOURCE_MANAGEMENT_API_URL").unwrap_or_else(|_| {
        if is_dev_build() {
            DEV_RESOURCE_MANAGEMENT_API_URL
        } else {
            PROD_RESOURCE_MANAGEMENT_API_URL
        }
        .to_string()
    })
}

choice! {
    pub(crate) enum Database {
        PostgreSQL = Choice {
            id: "postgresql",
            description: None,
            exclusive_files: Some(&["sql.base.properties.ts"]),
        },
        MySQL = Choice {
            id: "mysql",
            description: None,
            exclusive_files: Some(&["sql.base.properties.ts"]),
        },
        MariaDB = Choice {
            id: "mariadb",
            description: None,
            exclusive_files: Some(&["sql.base.properties.ts"]),
        },
        MsSQL = Choice {
            id: "mssql",
            description: None,
            exclusive_files: Some(&["sql.base.properties.ts"]),
        },
        MongoDB = Choice {
            id: "mongodb",
            description: None,
            exclusive_files: Some(&["nosql.base.properties.ts"]),
        },
        LibSQL = Choice {
            id: "libsql",
            description: None,
            exclusive_files: Some(&["sql.base.properties.ts"]),
        },
        SQLite = Choice {
            id: "sqlite",
            description: None,
            exclusive_files: Some(&["sql.base.properties.ts"]),
        },
        BetterSQLite = Choice {
            id: "better-sqlite",
            description: None,
            exclusive_files: Some(&["sql.base.properties.ts"]),
        },
    }

    pub(crate) enum Infrastructure {
        Redis = Choice {
            id: "redis",
            description: None,
            exclusive_files: None,
        },
        S3 = Choice {
            id: "s3",
            description: None,
            exclusive_files: None,
        },
    }

    pub(crate) enum WorkerType {
        Database = Choice {
            id: "database",
            description: None,
            exclusive_files: None,
        },
        RedisCache = Choice {
            id: "redis",
            description: None,
            exclusive_files: None,
        },
        Kafka = Choice {
            id: "kafka",
            description: None,
            exclusive_files: None,
        },
        BullMQCache = Choice {
            id: "bullmq",
            description: None,
            exclusive_files: None,
        },
    }

    pub(crate) enum Validator {
        Zod = Choice {
            id: "zod",
            description: None,
            exclusive_files: None,
        },
        Typebox = Choice {
            id: "typebox",
            description: None,
            exclusive_files: None,
        },
    }

    pub(crate) enum Formatter {
        Prettier = Choice {
            id: "prettier",
            description: None,
            exclusive_files: Some(&[".prettierignore", ".prettierrc"]),
        },
        Biome = Choice {
            id: "biome",
            description: None,
            exclusive_files: Some(&["biome.json"]),
        },
    }

    pub(crate) enum Linter {
        Eslint = Choice {
            id: "eslint",
            description: None,
            exclusive_files: Some(&["eslint.config.mjs"])  ,
        },
        Oxlint = Choice {
            id: "oxlint",
            description: None,
            exclusive_files: Some(&[".oxlint.config.json"]),
        },
    }

    pub(crate) enum Framework {
        Express = Choice {
            id: "express",
            description: None,
            exclusive_files: None,
        },
        HyperExpress = Choice {
            id: "hyper-express",
            description: None,
            exclusive_files: None,
        },
    }

    pub(crate) enum HttpFramework {
        Express = Choice {
            id: "express",
            description: None,
            exclusive_files: None,
        },
        HyperExpress = Choice {
            id: "hyper-express",
            description: None,
            exclusive_files: None,
        },
    }

    pub(crate) enum TestFramework {
        Vitest = Choice {
            id: "vitest",
            description: None,
            exclusive_files: Some(&["vitest.config.ts"]),
        },
        Jest = Choice {
            id: "jest",
            description: None,
            exclusive_files: Some(&["jest.config.ts"]),
        },
    }

    pub(crate) enum Runtime {
        Node = Choice {
            id: "node",
            description: None,
            exclusive_files: None,
        },
        Bun = Choice {
            id: "bun",
            description: None,
            exclusive_files: None,
        },
    }

    pub(crate) enum License {
        Apgl = Choice {
            id: "AGPL-3.0",
            description: None,
            exclusive_files: Some(&["agpl-3.0"]),
        },
        Gpl = Choice {
            id: "GPL-3.0",
            description: None,
            exclusive_files: Some(&["gpl-3.0"]),
        },
        Lgpl = Choice {
            id: "LGPL-3.0",
            description: None,
            exclusive_files: Some(&["lgpl-3.0"]),
        },
        Apache = Choice {
            id: "Apache-2.0",
            description: None,
            exclusive_files: Some(&["apache-2.0"]),
        },
        Mit = Choice {
            id: "MIT",
            description: None,
            exclusive_files: Some(&["mit"]),
        },
        Mozilla = Choice {
            id: "Mozilla-2.0",
            description: None,
            exclusive_files: Some(&["mpl-2.0"]),
        },
        Boost = Choice {
            id: "Boost-1.0",
            description: None,
            exclusive_files: Some(&["boost-1.0"]),
        },
        Unlicense = Choice {
            id: "Unlicense",
            description: None,
            exclusive_files: Some(&["unlicense"]),
        },
        None = Choice {
            id: "none",
            description: None,
            exclusive_files: None,
        },
    }

    pub(crate) enum Module {
        BaseBilling = Choice {
            id: "billing-base",
            description: Some("app hooks only"),
            exclusive_files: Some(&["billing-base"]),
        },
        StripeBilling = Choice {
            id: "billing-stripe",
            description: Some("stripe billing implementation"),
            exclusive_files: Some(&["billing-stripe"]),
        },
        BaseIam = Choice {
            id: "iam-base",
            description: Some("authorization only"),
            exclusive_files: Some(&["iam-base"]),
        },
        BetterAuthIam = Choice {
            id: "iam-better-auth",
            description: Some("better auth implementation for iam"),
            exclusive_files: Some(&["iam-better-auth"])
        },
        StripeEcommerce = Choice {
            id: "ecommerce-stripe",
            description: Some("stripe ecommerce implementation (catalog, cart, orders, payments, subscriptions)"),
            exclusive_files: Some(&["ecommerce-stripe"])
        },
        BaseMessaging = Choice {
            id: "messaging-base",
            description: Some("messaging hooks only (no delivery provider)"),
            exclusive_files: Some(&["messaging-base"]),
        },
        TwilioMessaging = Choice {
            id: "messaging-twilio",
            description: Some("twilio sms implementation for messaging"),
            exclusive_files: Some(&["messaging-twilio"]),
        },
        BaseCac = Choice {
            id: "cac-base",
            description: Some("computer-assisted coding hooks only"),
            exclusive_files: Some(&["cac-base"]),
        },
        Relay = Choice {
            id: "relay",
            description: Some("managed-apps OAuth relay session-ingest endpoint (adds to an existing iam service)"),
            exclusive_files: Some(&["relay"]),
        }
    }

    pub(crate) enum ModulesPath {
        Src = Choice {
            id: "src/modules",
            description: Some("src/modules"),
            exclusive_files: None,
        },
        Modules = Choice {
            id: "modules",
            description: Some("modules"),
            exclusive_files: None,
        },
    }

    pub(crate) enum InitializeType {
        Service = Choice {
            id: "service",
            description: Some("service"),
            exclusive_files: None,
        },
        Library = Choice {
            id: "library",
            description: Some("library"),
            exclusive_files: None,
        },
        Worker = Choice {
            id: "worker",
            description: Some("worker"),
            exclusive_files: None,
        },
        Module = Choice {
            id: "module",
            description: Some("module"),
            exclusive_files: None,
        },
        Router = Choice {
            id: "router",
            description: Some("router"),
            exclusive_files: None,
        },
    }

    pub(crate) enum SdkModeType {
        Generated = Choice {
            id: "generated",
            description: Some("generated"),
            exclusive_files: None,
        },
        Live = Choice {
            id: "live",
            description: Some("live"),
            exclusive_files: None,
        },
    }
}

// ERRORS
pub(crate) fn error_failed_to_read_file(path: &Path) -> String {
    format!(
        "Failed to read file {}. Please check file permissions.",
        path.to_string_lossy()
    )
}

pub(crate) fn error_failed_to_write_file(path: &Path) -> String {
    format!(
        "Failed to write file {}. Please check file permissions.",
        path.to_string_lossy()
    )
}
pub(crate) fn error_failed_to_create_symlink(path: &Path) -> String {
    format!(
        "Failed to create symlink {}. Please check your target directory is writable.",
        path.to_string_lossy()
    )
}
pub(crate) fn error_failed_to_create_dir(path: &Path) -> String {
    format!(
        "Failed to create directory {}. Please check your target directory is writable.",
        path.to_string_lossy()
    )
}

pub(crate) const ERROR_FAILED_TO_SEND_REQUEST: &str =
    "Failed to send request. Please check internet connectivity.";
pub(crate) const ERROR_FAILED_TO_GET_CWD: &str = "Failed to get current working directory.";
pub(crate) const ERROR_FAILED_TO_READ_MANIFEST: &str =
    "Failed to read manifest file. Please check your target directory is a forklaunch application.";
pub(crate) const ERROR_FAILED_TO_PARSE_MANIFEST: &str =
    "Failed to parse manifest file. Please verify the file is valid toml.";
pub(crate) const ERROR_FAILED_TO_WRITE_MANIFEST: &str =
    "Failed to write manifest file. Please check your target directory is writable.";
pub(crate) const ERROR_FAILED_TO_READ_DOCKER_COMPOSE: &str = "Failed to read docker-compose.yaml. Please check your target directory is a forklaunch application.";
pub(crate) const ERROR_FAILED_TO_PARSE_DOCKER_COMPOSE: &str =
    "Failed to parse docker-compose.yaml. Please verify the file is valid yaml.";
pub(crate) const ERROR_FAILED_TO_WRITE_DOCKER_COMPOSE: &str =
    "Failed to write docker-compose.yaml. Please check your target directory is writable.";
pub(crate) const ERROR_FAILED_TO_CREATE_PACKAGE_JSON: &str =
    "Failed to create package.json. Please check your target directory is writable.";
pub(crate) const ERROR_FAILED_TO_READ_PACKAGE_JSON: &str =
    "Failed to read package.json. Please check your target directory is a forklaunch application.";
pub(crate) const ERROR_FAILED_TO_PARSE_PACKAGE_JSON: &str =
    "Failed to parse package.json. Please verify the file is valid json.";
pub(crate) const ERROR_FAILED_TO_READ_PNPM_WORKSPACE: &str = "Failed to read pnpm-workspace.yaml. Please check your target directory is a forklaunch application.";
pub(crate) const ERROR_FAILED_TO_PARSE_PNPM_WORKSPACE: &str =
    "Failed to parse pnpm-workspace.yaml. Please verify the file is valid yaml.";
pub(crate) const ERROR_FAILED_TO_CREATE_SYMLINKS: &str =
    "Failed to create symlinks. Please check your target directory is writable.";
pub(crate) const ERROR_FAILED_TO_CREATE_DIR: &str =
    "Failed to create directory. Please check your target directory is writable.";
pub(crate) const ERROR_FAILED_TO_CREATE_MANIFEST: &str =
    "Failed to create manifest file. Please check your target directory is writable.";
pub(crate) const ERROR_FAILED_TO_CREATE_TSCONFIG: &str =
    "Failed to create tsconfig file. Please check your target directory is writable.";
pub(crate) const ERROR_FAILED_TO_CREATE_GITIGNORE: &str =
    "Failed to create gitignore file. Please check your target directory is writable.";
pub(crate) const ERROR_FAILED_TO_CREATE_LICENSE: &str =
    "Failed to create license file. Please check your target directory is writable.";
pub(crate) const ERROR_FAILED_TO_GENERATE_PNPM_WORKSPACE: &str =
    "Failed to generate pnpm-workspace.yaml.";
pub(crate) const ERROR_FAILED_TO_GENERATE_BUNFIG: &str = "Failed to generate bunfig.toml.";
pub(crate) const ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE: &str =
    "Failed to add project metadata to docker compose yaml.";
pub(crate) const ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST: &str =
    "Failed to add project metadata to manifest.";
pub(crate) const ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PACKAGE_JSON: &str =
    "Failed to add project metadata to package.json.";
pub(crate) const ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE: &str =
    "Failed to add project metadata to pnpm-workspace.yaml.";
pub(crate) const ERROR_UNSUPPORTED_DATABASE: &str =
    "Unsupported database. Failed to create base entity.";
pub(crate) const ERROR_DATABASE_INFORMATION: &str = "Database information not found, please ensure your service defines a database in your manifest.";
pub(crate) const ERROR_FAILED_TO_ADD_ROUTER_TO_APP: &str = "Failed to add router metadata to app.";
pub(crate) const ERROR_FAILED_TO_ADD_ROUTER_TO_BOOTSTRAPPER: &str =
    "Failed to add router metadata to bootstrapper.";
pub(crate) const ERROR_FAILED_TO_ADD_ROUTER_METADATA_TO_MANIFEST: &str =
    "Failed to add router metadata to manifest.";
pub(crate) const ERROR_FAILED_TO_ADD_ROUTER_TEST_UTILITIES: &str =
    "Failed to add router test utilities.";
pub(crate) const ERROR_FAILED_TO_REMOVE_PROJECT_METADATA_FROM_MANIFEST: &str =
    "Failed to remove project metadata from manifest.";
pub(crate) const ERROR_FAILED_TO_CREATE_DATABASE_EXPORT_INDEX_TS: &str =
    "Failed to create database export index.ts in core/persistence.";
pub(crate) const ERROR_FAILED_TO_CREATE_LIBRARY_PACKAGE_JSON: &str =
    "Failed to create library package.json.";
pub(crate) const ERROR_FAILED_TO_READ_STOREFRONT_MANIFEST: &str =
    "Failed to read the storefront manifest.json. Please check the --from path is correct.";
pub(crate) const ERROR_FAILED_TO_PARSE_STOREFRONT_MANIFEST: &str = "Failed to parse the storefront manifest.json. Please verify the file is valid json produced by the storefront capture tool.";
pub(crate) const ERROR_STOREFRONT_PROJECT_ALREADY_EXISTS: &str = "A project directory with this name already exists. Please choose a different name or remove the existing directory.";
pub(crate) const ERROR_FAILED_TO_COPY_STOREFRONT_SITE: &str = "Failed to copy captured storefront site assets. Please check the source site/ directory and target directory are accessible.";
pub(crate) const ERROR_FAILED_TO_ADD_SERVICE_METADATA_TO_ARTIFACTS: &str =
    "Failed to add service metadata to artifacts.";
pub(crate) const ERROR_FAILED_TO_UPDATE_APPLICATION_PACKAGE_JSON: &str =
    "Failed to update application package.json.";
pub(crate) const ERROR_FAILED_TO_ADD_BASE_ENTITY_TO_CORE: &str =
    "Failed to add base entity to core.";
pub(crate) const ERROR_FAILED_TO_WRITE_SERVICE_FILES: &str = "Failed to write service files.";
pub(crate) const ERROR_FAILED_TO_EJECT_DIRECTORY_NOT_EJECTABLE: &str = "Failed to eject directory. Please check your target directory is a preconfigured forklaunch module.";
pub(crate) const ERROR_FAILED_TO_UPDATE_DOCKERFILE: &str = "Failed to update Dockerfile.";
pub(crate) const ERROR_MANIFEST_NOT_FOUND: &str = "Could not find .forklaunch/manifest.toml. Make sure you're in a valid project directory or specify the correct base_path.";

pub(crate) fn get_core_module_description(name: &str) -> String {
    format!(
        "Core library for {}, containing shared foundational infrastrucuture and utilities",
        name
    )
}
pub(crate) fn get_monitoring_module_description(name: &str) -> String {
    format!(
        "Monitoring library for {}, defining metrics, logs, and trace building blocks",
        name
    )
}
pub(crate) fn get_client_sdk_module_description(name: &str) -> String {
    format!(
        "Client SDK for {}, containing shared foundational infrastrucuture and utilities",
        name
    )
}
pub(crate) fn get_service_module_name(service_type: &Module) -> String {
    match service_type {
        Module::BaseBilling | Module::StripeBilling => "billing".to_string(),
        Module::BaseIam | Module::BetterAuthIam => "iam".to_string(),
        Module::StripeEcommerce => "ecommerce".to_string(),
        Module::BaseMessaging | Module::TwilioMessaging => "messaging".to_string(),
        Module::BaseCac => "cac".to_string(),
        // Relay does not scaffold its own service - it injects the
        // session-ingest endpoint into the existing iam service (see
        // init/relay.rs). This name is only used for conflict detection, so it
        // gets its own class rather than colliding with "iam".
        Module::Relay => "relay".to_string(),
    }
}

pub(crate) fn get_service_module_description(name: &str, service_type: &Module) -> String {
    format!(
        "{} service implementation for {}, providing {}",
        name,
        get_service_module_name(service_type),
        match service_type {
            Module::BaseBilling | Module::StripeBilling => "billing service APIs",
            Module::BaseIam | Module::BetterAuthIam => "identity and access management APIs",
            Module::StripeEcommerce => "ecommerce service APIs",
            Module::BaseMessaging | Module::TwilioMessaging => "messaging service APIs",
            Module::BaseCac => "computer-assisted coding service APIs",
            Module::Relay => "the managed-apps OAuth relay session-ingest endpoint",
        }
    )
}
pub(crate) fn get_service_module_cache(service_type: &Module) -> Option<String> {
    match service_type {
        Module::BaseBilling | Module::StripeBilling => Some(Infrastructure::Redis.to_string()),
        // The messaging blueprint's registrations wire a RedisTtlCache.
        Module::BaseMessaging | Module::TwilioMessaging => {
            Some(Infrastructure::Redis.to_string())
        }
        // The ecommerce blueprint reads REDIS_URL at startup for both the cart
        // cache and the order-event queue, and exits if it is unset.
        Module::StripeEcommerce => Some(Infrastructure::Redis.to_string()),
        _ => None,
    }
}

// Sync constants - directories and services to ignore during sync operations
pub(crate) const DIRS_TO_IGNORE: &[&str] = &[
    "node_modules",
    "assets",
    "patches",
    ".git",
    ".github",
    ".vscode",
    "monitoring",
    "core",
    "client-sdk",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every module whose blueprint reads REDIS_URL at startup must declare a
    /// cache resource, otherwise `init module` writes a manifest with no Redis
    /// in it and the scaffolded project cannot boot once deployed.
    #[test]
    fn modules_needing_redis_declare_a_cache_resource() {
        for module in [
            Module::BaseBilling,
            Module::StripeBilling,
            Module::BaseMessaging,
            Module::TwilioMessaging,
            Module::StripeEcommerce,
        ] {
            assert_eq!(
                get_service_module_cache(&module),
                Some(Infrastructure::Redis.to_string()),
                "{module:?} needs a Redis cache resource in its manifest"
            );
        }
    }

    /// The iam modules wire no cache, so the helper must keep returning None
    /// for them rather than handing every module a Redis resource it will
    /// never use.
    #[test]
    fn modules_without_a_cache_declare_none() {
        assert_eq!(get_service_module_cache(&Module::BaseIam), None);
        assert_eq!(get_service_module_cache(&Module::BetterAuthIam), None);
    }
}

/// Studio orchestrator, which owns the analysis + report-card endpoints. Mirrors
/// the client's `STUDIO_API_URL` resolution so the CLI and the dashboard reach
/// the same service.
pub(crate) fn get_studio_orchestrator_api_url() -> String {
    std::env::var("FORKLAUNCH_STUDIO_ORCHESTRATOR_API_URL").unwrap_or_else(|_| {
        if is_dev_build() {
            DEV_STUDIO_ORCHESTRATOR_API_URL
        } else {
            PROD_STUDIO_ORCHESTRATOR_API_URL
        }
        .to_string()
    })
}
