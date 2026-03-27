use std::collections::HashSet;

use crate::{
    constants::{Database, Formatter, Linter, Runtime, TestFramework},
    core::database::is_in_memory_database,
};

// Shared package.json devDpendencies constants
// @biomejs/biome
pub(crate) const BIOME_VERSION: &str = "1.9.4";
// @eslint/js
pub(crate) const ESLINT_VERSION: &str = "^10.0.1";
// @types/jest
pub(crate) const JEST_TYPES_VERSION: &str = "^30.0.0";
// @types/node
pub(crate) const TYPES_NODE_VERSION: &str = "^25.5.0";
// jest
pub(crate) const JEST_VERSION: &str = "^30.3.0";
// oxlint
pub(crate) const OXLINT_VERSION: &str = "^0.16.6";
// prettier
pub(crate) const PRETTIER_VERSION: &str = "^3.8.1";
// ts-jest
pub(crate) const TS_JEST_VERSION: &str = "^29.4.6";
// ts-nodef
pub(crate) const TS_NODE_VERSION: &str = "^10.9.2";
// typescript-eslint
pub(crate) const TYPESCRIPT_ESLINT_VERSION: &str = "^8.57.2";
// vitest
pub(crate) const VITEST_VERSION: &str = "^4.1.2";

// Application package.json dependencies constants
// @typescript/native-preview
pub(crate) const TYPESCRIPT_NATIVE_PREVIEW_VERSION: &str = "7.0.0-dev.20260320.1";
// globals
pub(crate) const GLOBALS_VERSION: &str = "^17.4.0";
// husky
pub(crate) const HUSKY_VERSION: &str = "^9.1.7";
// lint-staged
pub(crate) const LINT_STAGED_VERSION: &str = "^15.4.3";
// node-gyp
pub(crate) const NODE_GYP_VERSION: &str = "^12.2.0";
// sort-package-json
pub(crate) const SORT_PACKAGE_JSON_VERSION: &str = "^3.0.0";
// tsx
pub(crate) const TSX_VERSION: &str = "^4.21.0";
// typescript
pub(crate) const TYPESCRIPT_VERSION: &str = "^6.0.2";

// Application package.json scripts constants
pub(crate) const APP_DEV_SCRIPT: &str = "docker compose up";
pub(crate) const APP_DEV_BUILD_SCRIPT: &str = "docker compose build --no-cache";
pub(crate) const APP_PREPARE_SCRIPT: &str = "husky";
pub(crate) const TYPES_WATCH_SCRIPT: &str = "tsgo -b -w";
pub(crate) const TYPES_BUILD_SCRIPT: &str = "tsgo -b --emitDeclarationOnly";

pub(crate) fn application_format_script(formatter: &Formatter) -> String {
    String::from(match formatter {
        Formatter::Prettier => {
            "prettier --ignore-path=.prettierignore --config .prettierrc '**/*.{ts,tsx,json}' --write"
        }
        Formatter::Biome => "biome format --write .",
    })
}

pub(crate) fn application_lint_script(linter: &Linter) -> String {
    String::from(match linter {
        Linter::Eslint => "eslint . -c eslint.config.mjs",
        Linter::Oxlint => "oxlint --config .oxlint.config.json",
    })
}

pub(crate) fn application_lint_fix_script(linter: &Linter) -> String {
    String::from(match linter {
        Linter::Eslint => "eslint . -c eslint.config.mjs --fix",
        Linter::Oxlint => "oxlint --fix --config .oxlint.config.json",
    })
}

pub(crate) fn application_build_script(runtime: &Runtime) -> String {
    match runtime {
        Runtime::Bun => "bunx -y @forklaunch/bunrun build".to_string(),
        Runtime::Node => "pnpm -r --no-bail run build".to_string(),
    }
}

pub(crate) fn application_clean_script(runtime: &Runtime) -> String {
    String::from(match runtime {
        Runtime::Bun => {
            "rm -rf node_modules bun.lockb bun.lock tsconfig.tsbuildinfo && bun --filter='*' clean"
        }
        Runtime::Node => {
            "rm -rf node_modules pnpm-lock.yaml tsconfig.tsbuildinfo && pnpm --parallel -r --no-bail clean && pnpm store prune"
        }
    })
}

pub(crate) fn application_clean_purge_script(runtime: &Runtime) -> String {
    String::from(match runtime {
        Runtime::Bun => "bun run clean",
        Runtime::Node => "pnpm run clean && pnpm store clear",
    })
}

pub(crate) fn application_docs_script(runtime: &Runtime) -> String {
    String::from(match runtime {
        Runtime::Bun => "bun --filter='*' docs",
        Runtime::Node => "pnpm --parallel -r --no-bail run docs",
    })
}

fn get_package_manager(runtime: &Runtime) -> String {
    let package_manager = match runtime {
        Runtime::Bun => "bun --filter='*'",
        Runtime::Node => "pnpm -r --no-bail",
    };

    String::from(package_manager)
}

fn get_db_init(databases: &HashSet<Database>) -> Option<String> {
    match databases.contains(&Database::MongoDB) {
        true => Some("mongo-init".to_string()),
        false => None,
    }
}

pub(crate) fn application_migrate_script<'a>(
    runtime: &Runtime,
    databases: &HashSet<Database>,
    migration_suffix: &'a str,
) -> String {
    let package_manager = get_package_manager(runtime);
    let db_init = get_db_init(databases);

    let sleep = if migration_suffix == "init" {
        "sleep 5 && "
    } else {
        ""
    };

    let mut databases = databases
        .iter()
        .filter(|db| !is_in_memory_database(db))
        .map(|db| db.to_string())
        .collect::<Vec<String>>();
    if let Some(db_init) = db_init {
        databases.push(db_init);
    }

    format!(
        "docker compose up{}{} && {}{} run migrate:{}",
        if databases.len() > 0 { " -d " } else { "" },
        databases.join(" -d "),
        sleep,
        package_manager,
        migration_suffix
    )
}

pub(crate) fn application_seed_script<'a>(
    runtime: &Runtime,
    databases: &HashSet<Database>,
) -> String {
    let package_manager = get_package_manager(runtime);
    let db_init = get_db_init(databases);

    let mut databases = databases
        .iter()
        .map(|db| db.to_string())
        .collect::<Vec<String>>();
    if let Some(db_init) = db_init {
        databases.push(db_init);
    }

    format!(
        "docker compose up -d {} && {} run seed",
        databases.join(" -d "),
        package_manager
    )
}

pub(crate) fn application_setup_script(runtime: &Runtime) -> String {
    match runtime {
        Runtime::Bun => {
            String::from("bun run build && bun run migrate:init && bun run migrate:up && bun run seed")
        }
        Runtime::Node => {
            String::from("pnpm build && pnpm migrate:init && pnpm migrate:up && pnpm seed")
        }
    }
}

pub(crate) fn application_test_script<'a>(
    _runtime: &Runtime,
    test_framework: &Option<TestFramework>,
) -> Option<String> {
    match test_framework {
        Some(test_framework_definition) => match test_framework_definition {
            TestFramework::Jest => Some("pnpm jest --passWithNoTests".to_string()),
            TestFramework::Vitest => Some("pnpm vitest --passWithNoTests".to_string()),
        },
        None => None,
    }
}

pub(crate) fn application_up_packages_script(runtime: &Runtime) -> String {
    String::from(match runtime {
        Runtime::Bun => "bunx -y @forklaunch/bunrun up:latest --sequential",
        Runtime::Node => "pnpm -r update --latest",
    })
}

pub(crate) fn project_up_latest_script(runtime: &Runtime) -> Option<String> {
    match runtime {
        Runtime::Bun => Some(String::from("bun update --latest")),
        Runtime::Node => None,
    }
}

// Project package.json dependencies constants
// @forklaunch/better-auth-mikro-orm-fork
pub(crate) const BETTER_AUTH_MIKRO_ORM_VERSION: &str = "~0.5.0";
// @forklaunch/blueprint-core
pub(crate) const APP_CORE_VERSION: &str = "workspace:*";
// @forklaunch/blueprint-billing
pub(crate) const APP_BILLING_VERSION: &str = "workspace:*";
// @forklaunch/blueprint-iam
pub(crate) const APP_IAM_VERSION: &str = "workspace:*";
// @forklaunch/blueprint-monitoring
pub(crate) const APP_MONITORING_VERSION: &str = "workspace:*";
// @forklaunch/bunrun
pub(crate) const BUNRUN_VERSION: &str = "~0.0.2";
// @forklaunch/common
pub(crate) const COMMON_VERSION: &str = "~1.2.2";
// @forklaunch/core
pub(crate) const CORE_VERSION: &str = "~1.2.3";
// @forklaunch/express
pub(crate) const EXPRESS_VERSION: &str = "~1.2.3";
// @forklaunch/hyper-express
pub(crate) const HYPER_EXPRESS_VERSION: &str = "~1.2.3";
// @forklaunch/implementation-billing-base
pub(crate) const BILLING_BASE_VERSION: &str = "~1.0.10";
// @forklaunch/implementation-billing-stripe
pub(crate) const BILLING_STRIPE_VERSION: &str = "~1.1.10";
// @forklaunch/implementation-iam-base
pub(crate) const IAM_BASE_VERSION: &str = "~1.0.10";
// @forklaunch/implementation-worker-bullmq
pub(crate) const WORKER_BULLMQ_VERSION: &str = "~1.0.11";
// @forklaunch/implementation-worker-redis
pub(crate) const WORKER_REDIS_VERSION: &str = "~1.0.11";
// @forklaunch/implementation-worker-database
pub(crate) const WORKER_DATABASE_VERSION: &str = "~1.0.11";
// @forklaunch/implementation-worker-kafka
pub(crate) const WORKER_KAFKA_VERSION: &str = "~1.0.11";
// @forklaunch/infrastructure-redis
pub(crate) const INFRASTRUCTURE_REDIS_VERSION: &str = "~1.2.3";
// @forklaunch/infrastructure-s3
pub(crate) const INFRASTRUCTURE_S3_VERSION: &str = "~1.2.3";
// @forklaunch/interfaces-billing
pub(crate) const BILLING_INTERFACES_VERSION: &str = "~1.0.10";
// @forklaunch/interfaces-iam
pub(crate) const IAM_INTERFACES_VERSION: &str = "~1.0.10";
// @forklaunch/interfaces-worker
pub(crate) const WORKER_INTERFACES_VERSION: &str = "~1.0.10";
// @forklaunch/internal
pub(crate) const INTERNAL_VERSION: &str = "~1.2.2";
// @forklaunch/testing
pub(crate) const TESTING_VERSION: &str = "~1.2.2";
// @forklaunch/universal-sdk
pub(crate) const UNIVERSAL_SDK_VERSION: &str = "~1.2.2";
// @forklaunch/validator
pub(crate) const VALIDATOR_VERSION: &str = "~1.2.2";
// @mikro-orm/core
pub(crate) const MIKRO_ORM_CORE_VERSION: &str = "7.0.6";
// @mikro-orm/migrations
pub(crate) const MIKRO_ORM_MIGRATIONS_VERSION: &str = "7.0.6";
// @mikro-orm/postgresql,@mikro-orm/mongodb,@mikro-orm/mysql,@mikro-orm/sqlite,@mikro-orm/mariadb,@mikro-orm/libsql,@mikro-orm/mssql
pub(crate) const MIKRO_ORM_DATABASE_VERSION: &str = "7.0.6";
// @mikro-orm/seeder
pub(crate) const MIKRO_ORM_SEEDER_VERSION: &str = "7.0.6";
// @opentelemetry/api
pub(crate) const OPENTELEMETRY_API_VERSION: &str = "^1.9.1";
// @sinclair/typebox
pub(crate) const TYPEBOX_VERSION: &str = "^0.34.48";
// ajv
pub(crate) const AJV_VERSION: &str = "^8.18.0";
// better-auth
pub(crate) const BETTER_AUTH_VERSION: &str = "^1.5.6";
// bullmq
pub(crate) const BULLMQ_VERSION: &str = "^5.71.1";
// better-sqlite3
pub(crate) const BETTER_SQLITE3_VERSION: &str = "^12.8.0";
// dotenv
pub(crate) const DOTENV_VERSION: &str = "^17.3.1";
// jose
pub(crate) const JOSE_VERSION: &str = "^6.2.2";
// sqlite3
pub(crate) const SQLITE3_VERSION: &str = "^6.0.1";
// stripe
pub(crate) const STRIPE_VERSION: &str = "^21.0.1";
// uuid
pub(crate) const UUID_VERSION: &str = "^13.0.0";
// zod
pub(crate) const ZOD_VERSION: &str = "^4.3.6";

// Project package.json devDependencies constants
// @mikro-orm/cli
pub(crate) const MIKRO_ORM_CLI_VERSION: &str = "7.0.6";
// @types/express
pub(crate) const TYPES_EXPRESS_VERSION: &str = "^5.0.6";
// @types/express-serve-static-core
pub(crate) const TYPES_EXPRESS_SERVE_STATIC_CORE_VERSION: &str = "^5.1.1";
// @types/jest
pub(crate) const TYPES_JEST_VERSION: &str = "^30.0.0";
// @types/uuid
pub(crate) const TYPES_UUID_VERSION: &str = "^11.0.0";
// @types/qs
pub(crate) const TYPES_QS_VERSION: &str = "^6.15.0";
// pino
pub(crate) const PINO_VERSION: &str = "^10.3.1";
// ioredis
pub(crate) const IOREDIS_VERSION: &str = "^5.10.1";
// typedoc
pub(crate) const TYPEDOC_VERSION: &str = "^0.28.18";

// Project package.json scripts constants
pub(crate) const PROJECT_BUILD_SCRIPT: &str = "tsgo -b";
pub(crate) const PROJECT_DOCS_SCRIPT: &str = "typedoc --out docs *";
pub(crate) const PROJECT_SEED_SCRIPT: &str = "[ -z $DOTENV_FILE_PATH ] && export DOTENV_FILE_PATH=.env.local; NODE_OPTIONS='--import=tsx' mikro-orm seeder:run";

pub(crate) fn project_retention_enforce_script(runtime: &Runtime) -> String {
    String::from(match runtime {
        Runtime::Bun => "bun run scripts/enforce-retention.ts",
        Runtime::Node => "pnpm tsx scripts/enforce-retention.ts",
    })
}

pub(crate) fn project_format_script(formatter: &Formatter) -> String {
    String::from(match formatter {
        Formatter::Prettier => {
            "prettier --ignore-path=.prettierignore --config .prettierrc '**/*.{ts,tsx,json}' --write"
        }
        Formatter::Biome => "biome format --write .",
    })
}

pub(crate) fn project_lint_script(linter: &Linter) -> String {
    String::from(match linter {
        Linter::Eslint => "eslint . -c eslint.config.mjs",
        Linter::Oxlint => "oxlint --config .oxlint.config.json",
    })
}

pub(crate) fn project_lint_fix_script(linter: &Linter) -> String {
    String::from(match linter {
        Linter::Eslint => "eslint . -c eslint.config.mjs --fix",
        Linter::Oxlint => "oxlint --fix --config .oxlint.config.json",
    })
}

pub(crate) fn project_clean_script(runtime: &Runtime) -> String {
    String::from(match runtime {
        Runtime::Bun => "rm -rf dist lib bun.lockb bun.lock tsconfig.tsbuildinfo node_modules",
        Runtime::Node => "rm -rf dist lib pnpm.lock.yaml tsconfig.tsbuildinfo node_modules",
    })
}

pub(crate) fn project_dev_server_script(runtime: &Runtime, database: Option<Database>) -> String {
    String::from(match runtime {
        Runtime::Bun => format!(
            "{}bun --watch server.ts",
            if database.is_some_and(|db| db != Database::MongoDB) {
                "bun run migrate:up && "
            } else {
                ""
            }
        ),
        Runtime::Node => format!(
            "{}pnpm tsx watch server.ts",
            if database.is_some_and(|db| db != Database::MongoDB) {
                "pnpm migrate:up && "
            } else {
                ""
            }
        ),
    })
}

pub(crate) fn project_dev_local_script(runtime: &Runtime, database: Option<Database>) -> String {
    String::from(match runtime {
        Runtime::Bun => format!(
            "{}DOTENV_FILE_PATH=.env.local bun --watch server.ts",
            if database.is_some_and(|db| db != Database::MongoDB) {
                "DOTENV_FILE_PATH=.env.local bun run migrate:up && "
            } else {
                ""
            }
        ),
        Runtime::Node => format!(
            "{}DOTENV_FILE_PATH=.env.local pnpm tsx watch server.ts",
            if database.is_some_and(|db| db != Database::MongoDB) {
                "DOTENV_FILE_PATH=.env.local pnpm migrate:up && "
            } else {
                ""
            }
        ),
    })
}

pub(crate) fn project_test_script(
    runtime: &Runtime,
    test_framework: &Option<TestFramework>,
) -> Option<String> {
    match runtime {
        Runtime::Bun => None,
        Runtime::Node => match test_framework {
            Some(test_framework_definition) => match test_framework_definition {
                TestFramework::Jest => Some("pnpm jest --passWithNoTests".to_string()),
                TestFramework::Vitest => Some("pnpm vitest --passWithNoTests".to_string()),
            },
            None => None,
        },
    }
}

pub(crate) fn project_migrate_script(command: &str) -> String {
    let base = "[ -z $DOTENV_FILE_PATH ] && export DOTENV_FILE_PATH=.env.local; NODE_OPTIONS='--import=tsx' mikro-orm migration:";
    match command {
        "create" => format!("{}{}", base, "create"),
        "down" => format!("{}{}", base, "down"),
        "init" => format!(
            "if [ ! -f migrations/Migration* ]; then {}{}; fi",
            base, "create --initial"
        ),
        "up" => format!("{}{}", base, "up"),
        _ => panic!("Unsupported migration command"),
    }
}

pub(crate) fn project_start_server_script(runtime: &Runtime, database: Option<Database>) -> String {
    format!(
        "{}[ -f .env.prod ] && export DOTENV_FILE_PATH=.env.prod; {} dist/server.js",
        if database.is_some_and(|db| db != Database::MongoDB) {
            format!(
                "[ -f .env.prod ] && export DOTENV_FILE_PATH=.env.prod; {} migrate:up && ",
                if runtime == &Runtime::Node {
                    "pnpm"
                } else {
                    "bun"
                }
            )
        } else {
            "".to_string()
        },
        match runtime {
            Runtime::Bun => "bun",
            Runtime::Node => "node --import=tsx",
        }
    )
}
pub(crate) fn project_start_worker_script(runtime: &Runtime, database: Option<Database>) -> String {
    format!(
        "{}[ -f .env.prod ] && export DOTENV_FILE_PATH=.env.prod; {} dist/worker.js",
        if database.is_some_and(|db| db != Database::MongoDB) {
            format!(
                "[ -f .env.prod ] && export DOTENV_FILE_PATH=.env.prod; {} migrate:up && ",
                if runtime == &Runtime::Node {
                    "pnpm"
                } else {
                    "bun"
                }
            )
        } else {
            "".to_string()
        },
        match runtime {
            Runtime::Bun => "bun",
            Runtime::Node => "node --import=tsx",
        }
    )
}

pub(crate) fn project_dev_worker_client_script(runtime: &Runtime) -> String {
    String::from(match runtime {
        Runtime::Bun => "bun --watch worker.ts",
        Runtime::Node => "pnpm tsx watch worker.ts",
    })
}

pub(crate) fn project_dev_local_worker_script(
    runtime: &Runtime,
    database: Option<Database>,
) -> String {
    String::from(match runtime {
        Runtime::Bun => format!(
            "{}DOTENV_FILE_PATH=.env.local bun --watch server.ts && DOTENV_FILE_PATH=.env.local bun --watch worker.ts",
            if database.is_some_and(|db| db != Database::MongoDB) {
                "DOTENV_FILE_PATH=.env.local bun run migrate:up && "
            } else {
                ""
            }
        ),
        Runtime::Node => format!(
            "{}DOTENV_FILE_PATH=.env.local pnpm tsx watch server.ts && DOTENV_FILE_PATH=.env.local pnpm tsx watch worker.ts",
            if database.is_some_and(|db| db != Database::MongoDB) {
                "DOTENV_FILE_PATH=.env.local pnpm migrate:up && "
            } else {
                ""
            }
        ),
    })
}

pub(crate) const SQLITE_POSTINSTALL_SCRIPT: &str =
    "cd node_modules/sqlite3 && node-gyp configure && node-gyp build";
pub(crate) const BETTER_SQLITE_POSTINSTALL_SCRIPT: &str =
    "cd node_modules/better-sqlite3 && node-gyp configure && node-gyp build";