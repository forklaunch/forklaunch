use std::{collections::HashMap, fmt};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Default, Clone)]
pub(crate) struct ApplicationScripts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) build: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) clean: Option<String>,
    #[serde(rename = "clean:purge", skip_serializing_if = "Option::is_none")]
    pub(crate) clean_purge: Option<String>,
    #[serde(rename = "database:setup", skip_serializing_if = "Option::is_none")]
    pub(crate) database_setup: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dev: Option<String>,
    #[serde(rename = "dev:build", skip_serializing_if = "Option::is_none")]
    pub(crate) dev_build: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) docs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lint: Option<String>,
    #[serde(rename = "lint:fix", skip_serializing_if = "Option::is_none")]
    pub(crate) lint_fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) postinstall: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prepare: Option<String>,
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
    pub(crate) test: Option<String>,
    #[serde(rename = "types:build", skip_serializing_if = "Option::is_none")]
    pub(crate) types_build: Option<String>,
    #[serde(rename = "types:watch", skip_serializing_if = "Option::is_none")]
    pub(crate) types_watch: Option<String>,
    #[serde(rename = "up:packages", skip_serializing_if = "Option::is_none")]
    pub(crate) up_packages: Option<String>,

    #[serde(flatten)]
    pub(crate) additional_scripts: HashMap<String, String>,
}

impl<'de> Deserialize<'de> for ApplicationScripts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};

        struct ApplicationScriptsVisitor;

        impl<'de> Visitor<'de> for ApplicationScriptsVisitor {
            type Value = ApplicationScripts;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map of script names to commands")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut scripts = ApplicationScripts {
                    build: None,
                    clean: None,
                    clean_purge: None,
                    database_setup: None,
                    dev: None,
                    dev_build: None,
                    docs: None,
                    format: None,
                    lint: None,
                    lint_fix: None,
                    postinstall: None,
                    prepare: None,
                    migrate_create: None,
                    migrate_down: None,
                    migrate_init: None,
                    migrate_up: None,
                    seed: None,
                    test: None,
                    types_build: None,
                    types_watch: None,
                    up_packages: None,
                    additional_scripts: HashMap::new(),
                };

                while let Some((key, value)) = access.next_entry::<String, String>()? {
                    match key.as_str() {
                        "build" => scripts.build = Some(value),
                        "clean" => scripts.clean = Some(value),
                        "clean:purge" => scripts.clean_purge = Some(value),
                        "database:setup" => scripts.database_setup = Some(value),
                        "dev" => scripts.dev = Some(value),
                        "dev:build" => scripts.dev_build = Some(value),
                        "docs" => scripts.docs = Some(value),
                        "format" => scripts.format = Some(value),
                        "lint" => scripts.lint = Some(value),
                        "lint:fix" => scripts.lint_fix = Some(value),
                        "postinstall" => scripts.postinstall = Some(value),
                        "prepare" => scripts.prepare = Some(value),
                        "migrate:create" => scripts.migrate_create = Some(value),
                        "migrate:down" => scripts.migrate_down = Some(value),
                        "migrate:init" => scripts.migrate_init = Some(value),
                        "migrate:up" => scripts.migrate_up = Some(value),
                        "seed" => scripts.seed = Some(value),
                        "test" => scripts.test = Some(value),
                        "types:build" => scripts.types_build = Some(value),
                        "types:watch" => scripts.types_watch = Some(value),
                        "up:packages" => scripts.up_packages = Some(value),
                        _ => {
                            scripts.additional_scripts.insert(key, value);
                        }
                    }
                }

                Ok(scripts)
            }
        }

        deserializer.deserialize_map(ApplicationScriptsVisitor)
    }
}

#[derive(Debug, Serialize, Default, Clone)]
pub(crate) struct ApplicationDevDependencies {
    #[serde(rename = "@biomejs/biome", skip_serializing_if = "Option::is_none")]
    pub(crate) biome: Option<String>,
    #[serde(rename = "@eslint/js", skip_serializing_if = "Option::is_none")]
    pub(crate) eslint_js: Option<String>,
    #[serde(rename = "@forklaunch/bunrun", skip_serializing_if = "Option::is_none")]
    pub(crate) bunrun: Option<String>,
    #[serde(rename = "@types/jest", skip_serializing_if = "Option::is_none")]
    pub(crate) types_jest: Option<String>,
    #[serde(rename = "@types/node", skip_serializing_if = "Option::is_none")]
    pub(crate) types_node: Option<String>,
    #[serde(rename = "better-sqlite3", skip_serializing_if = "Option::is_none")]
    pub(crate) better_sqlite3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) eslint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) globals: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) husky: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) jest: Option<String>,
    #[serde(rename = "lint-staged", skip_serializing_if = "Option::is_none")]
    pub(crate) lint_staged: Option<String>,
    #[serde(rename = "node-gyp", skip_serializing_if = "Option::is_none")]
    pub(crate) node_gyp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) oxlint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prettier: Option<String>,
    #[serde(rename = "sort-package-json", skip_serializing_if = "Option::is_none")]
    pub(crate) sort_package_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sqlite3: Option<String>,
    #[serde(rename = "ts-jest", skip_serializing_if = "Option::is_none")]
    pub(crate) ts_jest: Option<String>,
    #[serde(rename = "ts-node", skip_serializing_if = "Option::is_none")]
    pub(crate) ts_node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tsx: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) typescript: Option<String>,
    #[serde(
        rename = "@typescript/native-preview",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) typescript_native_preview: Option<String>,
    #[serde(rename = "typescript-eslint", skip_serializing_if = "Option::is_none")]
    pub(crate) typescript_eslint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) vitest: Option<String>,

    #[serde(flatten)]
    pub(crate) additional_deps: HashMap<String, String>,
}

impl<'de> Deserialize<'de> for ApplicationDevDependencies {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};

        struct ApplicationDevDependenciesVisitor;

        impl<'de> Visitor<'de> for ApplicationDevDependenciesVisitor {
            type Value = ApplicationDevDependencies;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map of dependency names to versions")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut deps = ApplicationDevDependencies {
                    biome: None,
                    bunrun: None,
                    eslint_js: None,
                    types_jest: None,
                    types_node: None,
                    better_sqlite3: None,
                    eslint: None,
                    globals: None,
                    husky: None,
                    jest: None,
                    lint_staged: None,
                    node_gyp: None,
                    oxlint: None,
                    prettier: None,
                    sort_package_json: None,
                    sqlite3: None,
                    ts_jest: None,
                    ts_node: None,
                    tsx: None,
                    typescript: None,
                    typescript_native_preview: None,
                    typescript_eslint: None,
                    vitest: None,
                    additional_deps: HashMap::new(),
                };

                while let Some((key, value)) = access.next_entry::<String, String>()? {
                    match key.as_str() {
                        "@biomejs/biome" => deps.biome = Some(value),
                        "@eslint/js" => deps.eslint_js = Some(value),
                        "@forklaunch/bunrun" => deps.bunrun = Some(value),
                        "@types/jest" => deps.types_jest = Some(value),
                        "@types/node" => deps.types_node = Some(value),
                        "better-sqlite3" => deps.better_sqlite3 = Some(value),
                        "eslint" => deps.eslint = Some(value),
                        "globals" => deps.globals = Some(value),
                        "husky" => deps.husky = Some(value),
                        "jest" => deps.jest = Some(value),
                        "lint-staged" => deps.lint_staged = Some(value),
                        "node-gyp" => deps.node_gyp = Some(value),
                        "oxlint" => deps.oxlint = Some(value),
                        "prettier" => deps.prettier = Some(value),
                        "sort-package-json" => deps.sort_package_json = Some(value),
                        "sqlite3" => deps.sqlite3 = Some(value),
                        "ts-jest" => deps.ts_jest = Some(value),
                        "tsx" => deps.tsx = Some(value),
                        "typescript" => deps.typescript = Some(value),
                        "typescript-eslint" => deps.typescript_eslint = Some(value),
                        "vitest" => deps.vitest = Some(value),
                        _ => {
                            deps.additional_deps.insert(key, value);
                        }
                    }
                }

                Ok(deps)
            }
        }

        deserializer.deserialize_map(ApplicationDevDependenciesVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct ApplicationPnpm {
    #[serde(
        rename = "patchedDependencies",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) patched_dependencies: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Default, Clone)]
pub(crate) struct ApplicationPackageJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workspaces: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scripts: Option<ApplicationScripts>,
    #[serde(rename = "devDependencies", skip_serializing_if = "Option::is_none")]
    pub(crate) dev_dependencies: Option<ApplicationDevDependencies>,
    // TODO: remove this, as this is a temporary patch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pnpm: Option<ApplicationPnpm>,
    #[serde(
        rename = "patchedDependencies",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) patched_dependencies: Option<HashMap<String, String>>,

    #[serde(flatten)]
    pub(crate) additional_entries: HashMap<String, serde_json::Value>,
}

impl<'de> Deserialize<'de> for ApplicationPackageJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};

        struct ApplicationPackageJsonVisitor;

        impl<'de> Visitor<'de> for ApplicationPackageJsonVisitor {
            type Value = ApplicationPackageJson;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a package.json object")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut package = ApplicationPackageJson {
                    name: None,
                    version: None,
                    description: None,
                    keywords: None,
                    license: None,
                    author: None,
                    workspaces: None,
                    scripts: None,
                    dev_dependencies: None,
                    pnpm: None,
                    patched_dependencies: None,
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
                        "workspaces" => {
                            package.workspaces = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "scripts" => {
                            package.scripts = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "devDependencies" => {
                            package.dev_dependencies = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "pnpm" => {
                            package.pnpm = Some(
                                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
                            )
                        }
                        "patchedDependencies" => {
                            package.patched_dependencies = Some(
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

        deserializer.deserialize_map(ApplicationPackageJsonVisitor)
    }
}
