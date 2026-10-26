//! Native capability edits. They keep application source, and removal is
//! stateless: it takes back what `infra add` writes for the service today
//! (generated files, the marked `FORKLAUNCH MANAGED CAPABILITIES` sections,
//! hooks, dependencies, settings), and keeps anything edited since.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use toml_edit::{Array, DocumentMut, InlineTable, value};

use super::{
    Capability, CapabilityEdit, GATEWAY_MOCK_SERVICE, ensure_gateway_mock, generated_gateway_mock,
    local_gateway_env, release_gateway_mock,
};
use crate::core::docker::{DependencyCondition, DependsOn, DockerCompose, DockerService};
const MODULE: &str = include_str!("../../templates/rust/capabilities/mod.rs");
const NATIVE_MODULE: &str = include_str!("../../templates/rust/capabilities/native_mod.rs");
const CONTRACT: &str = include_str!("../../templates/rust/capabilities/contract.rs");
const CONTROLLER: &str = include_str!("../../templates/rust/capabilities/controller.rs");
const CONTRACT_EXPORT: &str = "pub mod platform_events;\n";
const BUILTIN_CONTRACT_EXPORT: &str =
    "#[path = \"platform_events.rs\"]\npub mod platform_events;\n";
const NATIVE_REGISTRY: &str = "    platform_events::register_contracts(&mut registry);\n";
const HANDLER: &str = include_str!("../../templates/rust/capabilities/handler.rs");
const NONCE_STORE: &str = include_str!("../../templates/rust/capabilities/nonce_store.rs");
const BEGIN: &str = "// BEGIN FORKLAUNCH MANAGED CAPABILITIES\n";
const END: &str = "// END FORKLAUNCH MANAGED CAPABILITIES\n";
const LIB_EXPORT: &str = "pub mod capabilities;\n";
const REGISTRY: &str = "    capabilities::register_contracts(&mut registry);\n";
const STRIPE: &str = "forklaunch-billing-stripe";
/// Marks a worker's HTTP server in docker-compose with the project it serves.
const WORKER_OWNER: &str = "x-forklaunch-worker-for";
/// The capabilities module while no capability receives events: the client
/// factories alone, without the platform-events route.
const FACTORIES_MODULE: &str = "// BEGIN FORKLAUNCH MANAGED CAPABILITIES\n// Clients for the platform-held capabilities `forklaunch infra add` gave this\n// project. Each factory reads the managed gateway settings\n// (`PLATFORM_GATEWAY_URL`, `INSTANCE_ID`, `INSTANCE_HMAC_KEY`) when called.\nuse forklaunch_core::domain::BlueprintResult;\nuse forklaunch_core::http::instance_gateway::InstanceGatewayClient;\nuse std::sync::OnceLock;\n__CLIENT_IMPORTS__\nfn instance_gateway() -> BlueprintResult<InstanceGatewayClient> {\n    // Reuse the connection pool after the first successful resolution. A\n    // configuration error is not cached, so it can be corrected by the caller.\n    static GATEWAY: OnceLock<InstanceGatewayClient> = OnceLock::new();\n    if let Some(gateway) = GATEWAY.get() { return Ok(gateway.clone()); }\n    let gateway = InstanceGatewayClient::from_env()?;\n    Ok(GATEWAY.get_or_init(|| gateway).clone())\n}\n\n__CLIENT_FACTORIES__\n// END FORKLAUNCH MANAGED CAPABILITIES\n";

fn events_var(feature: &str) -> String {
    format!("MOCK_EVENTS_URL_{}", feature.to_uppercase())
}

/// Whether `url` is an event destination `infra add` writes: a compose
/// service's own `http://<service>:<port>`. Anything else is the developer's.
fn service_callback(compose: &DockerCompose, url: &str) -> bool {
    url.strip_prefix("http://")
        .and_then(|rest| rest.split_once(':'))
        .is_some_and(|(host, port)| {
            compose.services.contains_key(host)
                && !port.is_empty()
                && port.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// The `depends_on` entry add gives a service on the gateway mock.
fn generated_gateway_dependency() -> DependsOn {
    DependsOn {
        condition: DependencyCondition::ServiceStarted,
        ..Default::default()
    }
}

/// Remove the gateway mock once no project holds a capability, while it is
/// still the generated service (its event destinations aside). An edited
/// mock is the developer's.
fn release_unused_gateway_mock(edit: &CapabilityEdit, compose: &mut DockerCompose) -> Result<()> {
    let Some(current) = compose.services.get(GATEWAY_MOCK_SERVICE) else {
        return Ok(());
    };
    let mut candidate = current.clone();
    if let Some(env) = candidate.environment.as_mut() {
        env.retain(|key, _| !key.starts_with("MOCK_EVENTS_URL_"));
    }
    let generated = generated_gateway_mock(current.networks.clone());
    if serde_json::to_value(&candidate)? == serde_json::to_value(&generated)? {
        compose.services.shift_remove(GATEWAY_MOCK_SERVICE);
    } else {
        edit.note_kept_setting(
            &edit.app_root.join("docker-compose.yaml"),
            &format!("service {GATEWAY_MOCK_SERVICE}"),
        );
    }
    Ok(())
}

fn capability_owners(
    manifest: &toml::Value,
    updated: Option<(&str, &[String])>,
) -> BTreeMap<String, String> {
    let mut owners = BTreeMap::new();
    if let Some(projects) = manifest.get("projects").and_then(toml::Value::as_array) {
        for project in projects {
            let Some(service) = project.get("name").and_then(toml::Value::as_str) else {
                continue;
            };
            let features = if let Some((_, features)) = updated.filter(|(name, _)| *name == service)
            {
                features.to_vec()
            } else {
                project
                    .get("resources")
                    .and_then(|resources| resources.get("capabilities"))
                    .and_then(toml::Value::as_array)
                    .map(|features| {
                        features
                            .iter()
                            .filter_map(toml::Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default()
            };
            for feature in features {
                owners.entry(feature).or_insert_with(|| service.into());
            }
        }
    }
    owners
}

fn retain_callback_target(
    compose: &mut DockerCompose,
    removed: &str,
    feature: &str,
    previous: Option<String>,
    owners: &BTreeMap<String, String>,
) {
    let Some(owner) = owners.get(feature) else {
        return;
    };
    let (service, port) = event_receiver(compose, owner);
    let target = previous
        .filter(|url| retarget_service_url(url, removed, &service).is_none())
        .unwrap_or_else(|| format!("http://{service}:{port}"));
    if let Some(mock) = compose.services.get_mut(GATEWAY_MOCK_SERVICE) {
        mock.environment
            .get_or_insert_with(Default::default)
            .insert(events_var(feature), target);
    }
}

/// After a service rename: the mock's event destinations follow it.
pub(crate) fn rename_native_capabilities(root: &Path, old: &str, new: &str) -> Result<()> {
    if !root.join("docker-compose.yaml").exists() {
        return Ok(());
    }
    let mut edit = CapabilityEdit::new(root, "", new, root, vec![], vec![]);
    edit.native = true;
    edit.update_compose(|compose| {
        if let Some(mock) = compose.services.get_mut(GATEWAY_MOCK_SERVICE) {
            rename_callback_targets(mock, old, new);
        }
        Ok(())
    })?;
    edit.commit()
}

fn rename_callback_targets(mock: &mut DockerService, old: &str, new: &str) {
    if let Some(env) = mock.environment.as_mut() {
        for (key, value) in env {
            if !key.starts_with("MOCK_EVENTS_URL_") {
                continue;
            }
            if let Some(target) = retarget_service_url(value, old, new) {
                *value = target;
            }
        }
    }
}

fn retarget_service_url(url: &str, old: &str, new: &str) -> Option<String> {
    for scheme in ["http", "https"] {
        if let Some(suffix) = url.strip_prefix(&format!("{scheme}://{old}")) {
            if suffix.is_empty() || suffix.starts_with([':', '/', '?', '#']) {
                return Some(format!("{scheme}://{new}{suffix}"));
            }
        }
    }
    None
}

/// After `delete service`: drop the deleted service's event destinations,
/// promote another holder of each feature, and remove the mock with the last.
pub(crate) fn cleanup_deleted_native_service(
    root: &Path,
    name: &str,
    features: &[String],
) -> Result<()> {
    if features.is_empty() || !root.join("docker-compose.yaml").exists() {
        return Ok(());
    }
    let mut edit = CapabilityEdit::new(root, "", name, root, vec![], vec![]);
    edit.native = true;
    let manifest: toml::Value = toml::from_str(&std::fs::read_to_string(
        root.join(".forklaunch/manifest.toml"),
    )?)?;
    let remaining = capability_owners(&manifest, None);
    let mut compose = edit
        .read(&root.join("docker-compose.yaml"))?
        .map(|text| serde_yml::from_str::<DockerCompose>(&text))
        .transpose()?
        .context("docker-compose.yaml is missing")?;
    for feature in features {
        let previous = compose
            .services
            .get(GATEWAY_MOCK_SERVICE)
            .and_then(|mock| mock.environment.as_ref())
            .and_then(|env| env.get(&events_var(feature)))
            .cloned();
        let events = receives_events(feature);
        let (receiver, _) = event_receiver(&compose, name);
        for key in compose_processes(&compose, name) {
            release_gateway_mock(
                &mut compose,
                &key,
                feature,
                events && key == receiver,
                true,
                false,
            );
        }
        retain_callback_target(&mut compose, &receiver, feature, previous, &remaining);
    }
    for (feature, owner) in &remaining {
        let (service, port) = event_receiver(&compose, owner);
        if let Some(mock) = compose.services.get_mut(GATEWAY_MOCK_SERVICE) {
            mock.environment
                .get_or_insert_with(Default::default)
                .entry(events_var(feature))
                .or_insert_with(|| format!("http://{service}:{port}"));
        }
    }
    if remaining.is_empty() {
        release_unused_gateway_mock(&edit, &mut compose)?;
    }
    edit.update_compose(|current| {
        *current = compose;
        Ok(())
    })?;
    edit.commit()
}

/// The compose services that run a native project's code: the project's own
/// service and, for a worker, the HTTP server marked
/// `x-forklaunch-worker-for: <project>`. The last one serves HTTP and
/// receives the gateway mock's events.
pub(crate) fn compose_processes(compose: &DockerCompose, project: &str) -> Vec<String> {
    let mut keys: Vec<String> = compose
        .services
        .contains_key(project)
        .then(|| project.to_owned())
        .into_iter()
        .collect();
    keys.extend(
        compose
            .services
            .iter()
            .filter(|(_, service)| {
                service
                    .additional_properties
                    .get(WORKER_OWNER)
                    .and_then(|owner| owner.as_str())
                    == Some(project)
            })
            .map(|(name, _)| name.clone()),
    );
    keys
}

/// The compose service (and its port) that serves `project`'s
/// `/platform-events` route.
fn event_receiver(compose: &DockerCompose, project: &str) -> (String, String) {
    let key = compose_processes(compose, project)
        .pop()
        .unwrap_or_else(|| project.to_owned());
    let port = compose
        .services
        .get(&key)
        .and_then(|service| service.environment.as_ref())
        .and_then(|env| env.get("SERVICE_PORT").or_else(|| env.get("PORT")))
        .cloned()
        .unwrap_or_else(|| "8000".into());
    (key, port)
}

fn receives_events(feature: &str) -> bool {
    super::find(feature).is_none_or(|c| c.receives_events)
}

pub(super) fn apply(
    edit: &mut CapabilityEdit,
    capability: &Capability,
    adding: bool,
) -> Result<()> {
    let cargo_path = edit.service_path.join("Cargo.toml");
    let flattened = edit.read(&edit.service_path.join("lib.rs"))?.is_some();
    let source_root = if flattened {
        edit.service_path.clone()
    } else {
        edit.service_path.join("src")
    };
    let native_router = flattened
        && (edit
            .read(&edit.service_path.join("server.rs"))?
            .is_some_and(|source| source.contains("fn http_router("))
            || edit
                .read(&source_root.join("api/routes/mod.rs"))?
                .is_some_and(|source| source.contains("ContractRouter")));
    let builtin_contract = flattened
        && !native_router
        && edit
            .read(&source_root.join("api/contracts/registry.rs"))?
            .is_some();
    let shared_contract = native_router || builtin_contract;
    let module_path = source_root.join("capabilities/mod.rs");
    let replay_store_file = source_root.join("capabilities/nonce_store.rs");
    let inline_contract = native_router
        && edit
            .read(&source_root.join(".forklaunch/contracts/lib.rs"))?
            .is_some();
    let contract_path = source_root.join(if inline_contract {
        "api/controllers/platform_events.rs"
    } else {
        "api/contracts/platform_events.rs"
    });
    let contract_template = if builtin_contract {
        builtin_capability_contract()
    } else if inline_contract {
        CONTROLLER.into()
    } else {
        CONTRACT.into()
    };
    let current_contract = if shared_contract {
        edit.read(&contract_path)?
    } else {
        None
    };
    let lib_path = source_root.join("lib.rs");
    let main_path = if native_router {
        source_root.join("api/routes/mod.rs")
    } else if flattened {
        source_root.join("server.rs")
    } else {
        source_root.join("main.rs")
    };
    let registry_path = if inline_contract {
        source_root.join(".forklaunch/contracts/lib.rs")
    } else if builtin_contract {
        source_root.join("api/contracts/registry.rs")
    } else if native_router {
        source_root.join("api/contracts/mod.rs")
    } else if flattened {
        source_root.join("sdk.rs")
    } else {
        lib_path.clone()
    };
    let registry_hook = if shared_contract {
        NATIVE_REGISTRY
    } else if flattened {
        "    crate::capabilities::register_contracts(&mut registry);\n"
    } else {
        REGISTRY
    };
    let cargo_text = edit
        .read(&cargo_path)?
        .context("Native service Cargo.toml is missing")?;
    let mut cargo: DocumentMut = cargo_text.parse()?;
    let crate_module = cargo["package"]["name"]
        .as_str()
        .context("Native service package.name is missing")?
        .replace('-', "_");
    let namespace = if flattened { "crate" } else { &crate_module };
    let import = format!("use {namespace}::capabilities::ManagedCapabilitiesRouterExt;\n");
    let mut lib = edit
        .read(&lib_path)?
        .context("Native service src/lib.rs is missing")?;
    let mut main = edit
        .read(&main_path)?
        .context("Native service src/main.rs is missing")?;
    let mut registry_source = edit
        .read(&registry_path)?
        .context("Service contract registry source is missing")?;
    let current_module = edit.read(&module_path)?;
    let depth = edit
        .service_path
        .strip_prefix(&edit.app_root)?
        .components()
        .count();
    let prefix = std::iter::repeat_n("..", depth)
        .collect::<Vec<_>>()
        .join("/");
    let prefix = if prefix.is_empty() {
        "."
    } else {
        prefix.as_str()
    };
    let add_stripe = |cargo: &mut DocumentMut| {
        ensure_dependency(
            cargo,
            STRIPE,
            &format!(
                "{prefix}/.forklaunch/blueprint-rs/crates/implementations/billing/billing-stripe"
            ),
            None,
        )
    };
    // The service's capabilities after this change: the module holds a
    // client factory for each, and the platform-events route is mounted
    // while any of them receives events (`models` does not).
    let wants_module = !edit.service_capabilities.is_empty();
    let wants_events = edit
        .service_capabilities
        .iter()
        .any(|feature| receives_events(feature));

    // This capability's event handler.
    if capability.receives_events {
        let handler = source_root.join(format!("capabilities/{}.rs", capability.id));
        match edit.read(&handler)? {
            None if adding => edit.write(handler, HANDLER.into()),
            Some(text) if !adding && edit.is_generated(&handler, &text, HANDLER) => {
                edit.delete(handler)
            }
            Some(_) if !adding => edit.note_kept(&handler),
            _ => {}
        }
    }

    // The platform-events route: its contract, registry hook, router mount
    // and nonce store.
    if registry_path == lib_path {
        registry_source = lib.clone();
    }
    if wants_events {
        if edit.read(&replay_store_file)?.is_none() {
            edit.write(replay_store_file.clone(), NONCE_STORE.into());
        }
        if !main.contains(&import) {
            main = after_inner_docs(&main, &import);
        }
        if shared_contract {
            if !inline_contract && !builtin_contract && !registry_source.contains(CONTRACT_EXPORT) {
                registry_source = format!("{CONTRACT_EXPORT}{registry_source}");
            }
            if current_contract.is_none() {
                edit.write(contract_path.clone(), contract_template.clone());
            }
        }
        if builtin_contract {
            registry_source = builtin_capability_registry(&registry_source, true, false)?;
        } else if inline_contract {
            registry_source = inline_capability_registry(&registry_source, true)?;
        } else if !registry_source.contains(registry_hook) {
            let function = if native_router {
                "fn registry("
            } else {
                "fn contract_registry("
            };
            let start = registry_source
                .find(function)
                .context("Service must expose a contract registry")?;
            let end = start
                + registry_source[start..]
                    .find("    registry\n}")
                    .context("Contract registry must return its registry")?;
            registry_source.insert_str(end, registry_hook);
        }
        if !main.contains(".with_managed_capabilities()") {
            main = if native_router {
                with_native_capability_router(&main)?
            } else {
                with_capability_router(&main)?
            };
        }
        if builtin_contract {
            for scope in ["dependencies", "build-dependencies"] {
                ensure_dependency_feature(&mut cargo, scope, "forklaunch-core", "typed-schema")?;
            }
        }
    } else {
        match edit.read(&replay_store_file)? {
            Some(text) if edit.is_generated(&replay_store_file, &text, NONCE_STORE) => {
                edit.delete(replay_store_file)
            }
            Some(_) => edit.note_kept(&replay_store_file),
            None => {}
        }
        if builtin_contract {
            for scope in ["dependencies", "build-dependencies"] {
                release_dependency_feature(&mut cargo, scope, "forklaunch-core", "typed-schema")?;
            }
            registry_source = builtin_capability_registry(&registry_source, false, false)?;
        } else if inline_contract {
            registry_source = inline_capability_registry(&registry_source, false)?;
        } else {
            registry_source = registry_source.replace(registry_hook, "");
        }
        if shared_contract {
            let contract_kept = match &current_contract {
                Some(text) if edit.is_generated(&contract_path, text, &contract_template) => {
                    edit.delete(contract_path.clone());
                    false
                }
                Some(_) => {
                    edit.note_kept(&contract_path);
                    true
                }
                None => false,
            };
            if !inline_contract && !contract_kept {
                registry_source = if builtin_contract {
                    builtin_capability_registry(&registry_source, false, true)?
                } else {
                    registry_source.replace(CONTRACT_EXPORT, "")
                };
            }
        }
        main = without_capability_router(&main_path, &main, &import);
    }
    if registry_path == lib_path {
        lib = registry_source.clone();
    }

    // The capabilities module: a client factory per capability.
    let excluded = [source_root.join("capabilities")];
    if !adding
        && !edit.service_capabilities.iter().any(|id| id == "payments")
        && !dependency_in_use(&edit.service_path, &cargo, STRIPE, &excluded)
    {
        release_dependency(edit, &cargo_path, &mut cargo, STRIPE, add_stripe)?;
    }
    let pairs = local_gateway_env();
    let pairs: Vec<_> = pairs
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    if wants_module {
        if !lib.contains(LIB_EXPORT) {
            crate::infra::native_local::insert_module_export(&mut lib, LIB_EXPORT, "capabilities");
        }
        // Platform clients live in the framework, as in `@forklaunch/core/http`.
        ensure_dependency_feature(
            &mut cargo,
            "dependencies",
            "forklaunch-core",
            "instance-gateway",
        )?;
        if adding && capability.id == "payments" {
            add_stripe(&mut cargo)?;
        }
        let generated = render_module_with_layout(&edit.service_capabilities, native_router)?;
        let generated = if !wants_events {
            generated
        } else if inline_contract {
            inline_capability_module(&generated)
        } else if builtin_contract {
            builtin_capability_module(&generated)?
        } else {
            generated
        };
        let module = match current_module {
            Some(current) if current.contains(BEGIN) => replace_owned_block(&current, &generated)?,
            Some(current) => format!(
                "{current}{}{generated}",
                if current.ends_with('\n') { "" } else { "\n" }
            ),
            None => generated,
        };
        // Staged in rustfmt's form on remove too: the module is regenerated
        // from the remaining capabilities, not restored.
        let module = if edit.format_rust {
            module
        } else {
            crate::init::rust_project::format_rust_source(&module_path, &module)
        };
        edit.write(module_path, module);
        edit.ensure_env_local(&pairs)?;
        // Blueprint dependencies resolve through the application's
        // `[framework]` selection, like every generated manifest.
        let updated = crate::init::rust_project::dependencies::render_for_path(
            &cargo_path,
            &cargo.to_string(),
        )?;
        if updated != cargo_text {
            edit.write(cargo_path, updated);
        }
    } else {
        release_dependency_feature(
            &mut cargo,
            "dependencies",
            "forklaunch-core",
            "instance-gateway",
        )?;
        if cargo.to_string() != cargo_text {
            edit.write(cargo_path, cargo.to_string());
        }
        // The marked section is the CLI's; the rest of the module is the developer's.
        let kept = match current_module {
            Some(current) if current.contains(BEGIN) => replace_owned_block(&current, "")?,
            Some(current) => current,
            None => String::new(),
        };
        if !kept.trim().is_empty() {
            edit.note_kept(&module_path);
            edit.write(module_path, kept);
        } else {
            edit.delete(module_path);
            lib = lib.replace(LIB_EXPORT, "");
        }
        edit.release_env_local(&pairs)?;
    }

    // docker-compose: every process of the project reaches the gateway mock;
    // the one serving HTTP receives the feature's events.
    let service_name = edit.service_name.clone();
    let feature = capability.id;
    let events = receives_events(feature);
    if adding {
        edit.update_compose(|compose| {
            let keys = compose_processes(compose, &service_name);
            anyhow::ensure!(!keys.is_empty(), "Native service is missing from Compose");
            for key in &keys {
                // One destination per feature, set below rather than appended.
                ensure_gateway_mock(compose, key, feature, false)?;
                let env = compose
                    .services
                    .get_mut(key)
                    .context("Native service is missing from Compose")?
                    .environment
                    .get_or_insert_with(Default::default);
                env.entry("FORKLAUNCH_LOCAL_GATEWAY_MOCK".into())
                    .or_insert_with(|| "true".into());
            }
            let (receiver, port) = event_receiver(compose, &service_name);
            let current = compose
                .services
                .get(GATEWAY_MOCK_SERVICE)
                .and_then(|mock| mock.environment.as_ref())
                .and_then(|env| env.get(&events_var(feature)))
                .cloned();
            // The newest holder receives the events, unless the developer
            // pointed them elsewhere.
            if events
                && current.is_none_or(|url| service_callback(compose, &url))
                && let Some(mock) = compose.services.get_mut(GATEWAY_MOCK_SERVICE)
            {
                mock.environment
                    .get_or_insert_with(Default::default)
                    .insert(events_var(feature), format!("http://{receiver}:{port}"));
            }
            Ok(())
        })?;
    } else {
        let service_empty = edit.service_capabilities.is_empty();
        let app_empty = edit.app_capabilities.is_empty();
        let manifest: toml::Value = toml::from_str(
            &edit
                .read(&edit.app_root.join(".forklaunch/manifest.toml"))?
                .context("Application manifest is missing")?,
        )?;
        let owners =
            capability_owners(&manifest, Some((&service_name, &edit.service_capabilities)));
        let mut compose = edit
            .read(&edit.app_root.join("docker-compose.yaml"))?
            .map(|text| serde_yml::from_str::<DockerCompose>(&text))
            .transpose()?;
        if let Some(compose) = compose.as_mut() {
            let keys = compose_processes(compose, &service_name);
            let (receiver, _) = event_receiver(compose, &service_name);
            let dependencies: Vec<(String, Option<DependsOn>)> = keys
                .iter()
                .map(|key| {
                    let dependency = compose
                        .services
                        .get(key)
                        .and_then(|target| target.depends_on.as_ref())
                        .and_then(|dependencies| dependencies.get(GATEWAY_MOCK_SERVICE))
                        .cloned();
                    (key.clone(), dependency)
                })
                .collect();
            let previous = compose
                .services
                .get(GATEWAY_MOCK_SERVICE)
                .and_then(|mock| mock.environment.as_ref())
                .and_then(|env| env.get(&events_var(feature)))
                .cloned();
            for key in &keys {
                release_gateway_mock(
                    compose,
                    key,
                    feature,
                    events && *key == receiver,
                    service_empty,
                    false,
                );
            }
            retain_callback_target(compose, &receiver, feature, previous, &owners);
            if service_empty {
                for (key, dependency) in dependencies {
                    let Some(target) = compose.services.get_mut(&key) else {
                        continue;
                    };
                    let env = target.environment.get_or_insert_with(Default::default);
                    if env
                        .get("FORKLAUNCH_LOCAL_GATEWAY_MOCK")
                        .is_some_and(|value| value == "true")
                    {
                        env.shift_remove("FORKLAUNCH_LOCAL_GATEWAY_MOCK");
                    }
                    // A dependency with the developer's own options stays.
                    if let Some(dependency) = dependency
                        && serde_json::to_value(&dependency)?
                            != serde_json::to_value(generated_gateway_dependency())?
                    {
                        target
                            .depends_on
                            .get_or_insert_with(Default::default)
                            .insert(GATEWAY_MOCK_SERVICE.into(), dependency);
                    }
                }
            }
            if app_empty {
                release_unused_gateway_mock(edit, compose)?;
            }
        }
        if let Some(compose) = compose {
            edit.update_compose(|current| {
                *current = compose;
                Ok(())
            })?;
        }
    }
    if registry_path != lib_path {
        edit.write(registry_path, registry_source);
    }
    edit.write(lib_path, lib);
    edit.write(main_path, main);
    Ok(())
}
pub(crate) fn ensure_dependency(
    cargo: &mut DocumentMut,
    name: &str,
    path: &str,
    feature: Option<&str>,
) -> Result<()> {
    if cargo["dependencies"].get(name).is_none() {
        let mut table = InlineTable::new();
        table.insert("path", path.into());
        cargo["dependencies"][name] = value(table);
    }
    if let Some(feature) = feature {
        ensure_dependency_feature(cargo, "dependencies", name, feature)?;
    }
    Ok(())
}
fn ensure_dependency_feature(
    cargo: &mut DocumentMut,
    scope: &str,
    name: &str,
    feature: &str,
) -> Result<()> {
    anyhow::ensure!(
        cargo.get(scope).and_then(|deps| deps.get(name)).is_some(),
        "Native {scope} must declare {name}"
    );
    if let Some(version) = cargo[scope][name].as_str() {
        let mut table = InlineTable::new();
        table.insert("version", version.into());
        cargo[scope][name] = value(table);
    }
    if cargo[scope][name].get("features").is_none() {
        cargo[scope][name]["features"] = value(Array::new());
    }
    let features = cargo[scope][name]["features"]
        .as_array_mut()
        .context("Native dependency features must be an array")?;
    if !features.iter().any(|value| value.as_str() == Some(feature)) {
        // Follow the array's layout, so a one-per-line list stays one per line.
        let prefix = features
            .iter()
            .last()
            .and_then(|last| last.decor().prefix())
            .and_then(|prefix| prefix.as_str())
            .map(str::to_owned);
        let mut item = toml_edit::Value::from(feature);
        if let Some(prefix) = prefix {
            item.decor_mut().set_prefix(prefix);
        }
        features.push_formatted(item);
    }
    Ok(())
}

/// Undo [`ensure_dependency_feature`]: drop `feature` from `name`'s list, the
/// list once empty, and the table form a bare version requirement was given.
fn release_dependency_feature(
    cargo: &mut DocumentMut,
    scope: &str,
    name: &str,
    feature: &str,
) -> Result<()> {
    let Some(dependency) = cargo
        .get_mut(scope)
        .and_then(|deps| deps.get_mut(name))
        .and_then(|item| item.as_table_like_mut())
    else {
        return Ok(());
    };
    let Some(features) = dependency
        .get_mut("features")
        .and_then(|item| item.as_array_mut())
    else {
        return Ok(());
    };
    let Some(index) = features.iter().position(|value| value.as_str() == Some(feature)) else {
        return Ok(());
    };
    features.remove(index);
    if !features.is_empty() {
        return Ok(());
    }
    dependency.remove("features");
    let version = cargo[scope][name]
        .as_inline_table()
        .filter(|table| table.len() == 1)
        .and_then(|table| table.get("version"))
        .and_then(|version| version.as_str())
        .map(str::to_owned);
    if let Some(version) = version {
        cargo[scope][name] = value(version);
    }
    Ok(())
}

/// Remove dependency `name` while it still has the form `add` gives it in
/// this manifest (rendered through the application's `[framework]`
/// selection). One the developer changed stays, with a note.
pub(crate) fn release_dependency(
    edit: &mut CapabilityEdit,
    cargo_path: &Path,
    cargo: &mut DocumentMut,
    name: &str,
    add: impl Fn(&mut DocumentMut) -> Result<()>,
) -> Result<()> {
    let current: toml::Value = toml::from_str(&cargo.to_string())?;
    let Some(current) = current.get("dependencies").and_then(|deps| deps.get(name)) else {
        return Ok(());
    };
    let mut fresh = cargo.clone();
    if let Some(deps) = fresh
        .get_mut("dependencies")
        .and_then(|deps| deps.as_table_like_mut())
    {
        deps.remove(name);
    }
    add(&mut fresh)?;
    let rendered = crate::init::rust_project::dependencies::render_for_path(cargo_path, &fresh.to_string())?;
    let generated: toml::Value = toml::from_str(&rendered)?;
    if generated.get("dependencies").and_then(|deps| deps.get(name)) == Some(current) {
        if let Some(deps) = cargo
            .get_mut("dependencies")
            .and_then(|deps| deps.as_table_like_mut())
        {
            deps.remove(name);
        }
    } else {
        edit.note_kept_setting(cargo_path, &format!("dependency {name}"));
    }
    Ok(())
}

/// Whether the crate at `root` uses dependency `name` other than through
/// `excluded` paths: from a `[features]` entry, or in Rust source.
pub(crate) fn dependency_in_use(
    root: &Path,
    cargo: &DocumentMut,
    name: &str,
    excluded: &[PathBuf],
) -> bool {
    let feature_uses = cargo
        .get("features")
        .and_then(|features| features.as_table_like())
        .is_some_and(|features| {
            features.iter().any(|(_, entries)| {
                entries.as_array().is_some_and(|entries| {
                    entries.iter().filter_map(|entry| entry.as_str()).any(|entry| {
                        entry == format!("dep:{name}")
                            || entry.starts_with(&format!("{name}/"))
                            || entry.starts_with(&format!("{name}?/"))
                    })
                })
            })
        });
    let ident = name.replace('-', "_");
    fn sources_use(dir: &Path, ident: &str, excluded: &[PathBuf]) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let path = entry.path();
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if excluded.iter().any(|excluded| path.starts_with(excluded))
                || file_name.starts_with('.')
                || file_name == "target"
                || file_name == "node_modules"
            {
                return false;
            }
            if path.is_dir() {
                sources_use(&path, ident, excluded)
            } else {
                path.extension().is_some_and(|extension| extension == "rs")
                    && std::fs::read_to_string(&path).is_ok_and(|text| text.contains(ident))
            }
        })
    }
    feature_uses || sources_use(root, &ident, excluded)
}

fn replace_owned_block(current: &str, replacement: &str) -> Result<String> {
    let start = current
        .find(BEGIN)
        .context("Native capability generated section is missing")?;
    let end = start
        + current[start..]
            .find(END)
            .context("Native capability generated section is incomplete")?
        + END.len();
    Ok(format!(
        "{}{}{}",
        &current[..start],
        replacement,
        &current[end..]
    ))
}
fn render_module_with_layout(features: &[String], native_router: bool) -> Result<String> {
    let mut modules = Vec::new();
    let mut arms = Vec::new();
    let mut known = Vec::new();
    let mut factories = Vec::new();
    let mut clients = Vec::new();
    for feature in features {
        let Some(capability) = super::find(feature) else {
            bail!("Unknown native managed capability '{feature}'");
        };
        if capability.receives_events {
            modules.push(format!("pub mod {feature};"));
            arms.push(format!(
                "                        \"{feature}\" => {feature}::handle(event).await,"
            ));
            known.push(format!("\"{feature}\""));
        }
        let (module, ty) = match feature.as_str() {
            "email" => ("email_client", "EmailClient"),
            "sms" => ("sms_client", "SmsClient"),
            "whatsapp" => ("whatsapp_client", "WhatsAppClient"),
            "voice" => ("voice_client", "VoiceClient"),
            "payments" => ("payments_client", "PaymentsClient"),
            "models" => ("model_gateway", "ModelGatewayClient"),
            _ => bail!("Native managed capability '{feature}' has no Rust client in this CLI"),
        };
        clients.push(format!("{module}::{ty}"));
        let factory = if feature == "models" {
            "models_client"
        } else {
            &format!("{feature}_client")
        };
        let doc = match feature.as_str() {
            "models" => {
                "/// Platform-hosted models through the managed model gateway: no provider key;\n/// the platform enforces the product's model allowlist, token budget and rate\n/// limit. `models()` lists the catalog, `chat_completion` and\n/// `chat_completion_stream` call a model."
            }
            "email" => "/// Email through the platform (Amazon SES).",
            "sms" => "/// Text messages through the platform.",
            "whatsapp" => "/// WhatsApp messages through the platform (no Meta token in the app).",
            "voice" => "/// Outbound phone calls through the platform.",
            _ => {
                "/// The instance's Stripe Connect account through the platform (no Stripe key in the app)."
            }
        };
        if feature == "sms" {
            factories.push(format!("{doc}\npub fn sms_client() -> BlueprintResult<SmsClient> {{\n    // SMS stays deferred until the managed gateway settings are all present.\n    for name in [\"PLATFORM_GATEWAY_URL\", \"INSTANCE_ID\", \"INSTANCE_HMAC_KEY\"] {{\n        if forklaunch_core::config::optional::<String>(name)?.is_none() {{\n            return Ok(SmsClient::deferred());\n        }}\n    }}\n    Ok(SmsClient::new(instance_gateway()?))\n}}"));
        } else {
            factories.push(format!("{doc}\npub fn {factory}() -> BlueprintResult<{ty}> {{ Ok({ty}::new(instance_gateway()?)) }}"));
        }
        if feature == "payments" {
            factories.push("/// Stripe billing through the platform's connected account.\npub fn stripe_client() -> BlueprintResult<StripeBillingClient> { Ok(StripeBillingClient::with_gateway(instance_gateway()?)) }".into());
        }
    }
    let mut imports = String::new();
    match clients.as_slice() {
        [] => {}
        [client] => imports.push_str(&format!("use forklaunch_core::http::{client};\n")),
        clients => imports.push_str(&format!(
            "use forklaunch_core::http::{{{}}};\n",
            clients.join(", ")
        )),
    }
    if features.iter().any(|feature| feature == "payments") {
        imports.push_str("use forklaunch_billing_stripe::client::StripeBillingClient;\n");
    }
    let template = if known.is_empty() {
        FACTORIES_MODULE
    } else if native_router {
        NATIVE_MODULE
    } else {
        MODULE
    };
    Ok(template
        .replace("__EVENT_MODULES__", &modules.join("\n"))
        .replace("__HANDLER_ARMS__", &arms.join("\n"))
        .replace("__KNOWN_FEATURES__", &known.join(" | "))
        .replace("__CLIENT_IMPORTS__", &imports)
        .replace("__CLIENT_FACTORIES__", &factories.join("\n")))
}

// Built-in modules compile api/contracts/registry.rs both in their build host
// and at runtime. Keep this declaration independent of the runtime adapter.
fn builtin_capability_contract() -> String {
    CONTRACT
        .replace("pub occurred_at: Option<String>", "pub occurred_at: String")
        .replace("pub data: Option<JsonValue>", "pub data: JsonValue")
        .replace("BodyContract, JsonValue};", "BodyContract, BodyKind, JsonValue, SchemaRef};")
        .replace("    route.options.mcp = false;", "    for status in [401, 404, 500, 503] {\n        route = route.with_response(status, Some(BodyContract { kind: BodyKind::Text, schema: SchemaRef::named(\"String\"), content_type: Some(\"text/plain\".into()) }));\n    }\n    route.options.mcp = false;")
}

fn builtin_capability_module(source: &str) -> Result<String> {
    let start = source
        .find("fn text_body()")
        .context("Managed callback declaration is missing")?;
    let end = source
        .find("pub trait ManagedCapabilitiesRouterExt")
        .context("Managed callback adapter is missing")?;
    let mut source = format!(
        "{}pub use crate::api::contracts::registry::platform_events::{{contracts, register_contracts}};\n\n{}",
        &source[..start],
        &source[end..]
    );
    source = source
        .replace("use forklaunch_core::contract::{BodyContract, ContractRegistry, FieldContract, RouteContract, BodyKind, SchemaRef};\n", "")
        .replace("use forklaunch_core::contract::AccessLevel;\n", "");
    Ok(source)
}

fn builtin_capability_export(source: &str) -> Result<bool> {
    let file = syn::parse_file(source).context("Cannot parse built-in contract registry")?;
    for item in &file.items {
        if let syn::Item::Mod(module) = item {
            if module.ident == "platform_events" {
                let expected_path = module.attrs.iter().any(|attr| match &attr.meta {
                    syn::Meta::NameValue(value) if value.path.is_ident("path") => matches!(&value.value, syn::Expr::Lit(value) if matches!(&value.lit, syn::Lit::Str(path) if path.value() == "platform_events.rs")),
                    _ => false,
                });
                anyhow::ensure!(
                    expected_path
                        && module.content.is_none()
                        && matches!(module.vis, syn::Visibility::Public(_)),
                    "Built-in platform_events module must publicly declare the sibling platform_events.rs contract"
                );
                return Ok(true);
            }
        }
    }
    Ok(false)
}

const BUILTIN_REGISTRY_MARKER: &str = "// ForkLaunch managed platform event contract.\n";

fn builtin_capability_registry(source: &str, adding: bool, remove_export: bool) -> Result<String> {
    use syn::spanned::Spanned;
    fn registry_value(expr: &syn::Expr) -> bool {
        match expr {
            syn::Expr::Path(path) => path.path.is_ident("registry"),
            syn::Expr::Call(call) => {
                matches!(&*call.func, syn::Expr::Path(path) if path.path.is_ident("Ok"))
                    && call.args.len() == 1
                    && registry_value(&call.args[0])
            }
            syn::Expr::Return(value) => value.expr.as_deref().is_some_and(registry_value),
            _ => false,
        }
    }
    fn mutable_registry(pat: &syn::Pat) -> bool {
        match pat {
            syn::Pat::Ident(ident) => ident.ident == "registry" && ident.mutability.is_some(),
            syn::Pat::Type(ty) => mutable_registry(&ty.pat),
            _ => false,
        }
    }
    fn hook(stmt: &syn::Stmt) -> bool {
        let syn::Stmt::Expr(syn::Expr::Call(call), _) = stmt else {
            return false;
        };
        matches!(&*call.func, syn::Expr::Path(path) if path.path.segments.len() == 2 && path.path.segments[0].ident == "platform_events" && path.path.segments[1].ident == "register_contracts")
            && call.args.len() == 1
            && matches!(&call.args[0], syn::Expr::Reference(reference) if reference.mutability.is_some() && matches!(&*reference.expr, syn::Expr::Path(path) if path.path.is_ident("registry")))
    }
    let exported = builtin_capability_export(source)?;
    let file = syn::parse_file(source)?;
    let mut edits = Vec::new();
    let mut builders = 0;
    for item in &file.items {
        if let syn::Item::Mod(module) = item
            && remove_export
            && module.ident == "platform_events"
        {
            // With the line break add wrote after it.
            let mut range = module.span().byte_range();
            if source[range.end..].starts_with('\n') {
                range.end += 1;
            }
            edits.push((range, String::new()));
        }
        let syn::Item::Fn(function) = item else {
            continue;
        };
        if !function
            .sig
            .ident
            .to_string()
            .starts_with("contract_registry")
        {
            continue;
        }
        let stmts = &function.block.stmts;
        if adding {
            if !stmts
                .iter()
                .any(|stmt| matches!(stmt, syn::Stmt::Local(local) if mutable_registry(&local.pat)))
            {
                continue;
            }
            let tail = stmts
                .last()
                .context("Built-in contract registry builder is empty")?;
            anyhow::ensure!(
                matches!(tail, syn::Stmt::Expr(expr, _) if registry_value(expr)),
                "Built-in contract registry builder must return registry or Ok(registry)"
            );
            builders += 1;
            if !stmts.iter().any(hook) {
                let at = tail.span().byte_range().start;
                let line = source[..at].rfind('\n').map_or(0, |n| n + 1);
                let indent = &source[line..at];
                let indent = if indent.trim().is_empty() {
                    indent
                } else {
                    "    "
                };
                edits.push((at..at, format!("{BUILTIN_REGISTRY_MARKER}{indent}platform_events::register_contracts(&mut registry);\n{indent}")));
            }
        } else {
            for statement in stmts.iter().filter(|stmt| hook(stmt)) {
                let range = statement.span().byte_range();
                // Remove only our marked hook, preserving any app-owned use of
                // the declaration.
                let prefix = &source[..range.start];
                if let Some(start) = prefix.rfind(BUILTIN_REGISTRY_MARKER)
                    && prefix[start + BUILTIN_REGISTRY_MARKER.len()..]
                        .trim()
                        .is_empty()
                {
                    // With the separator add wrote before the return.
                    let end = range.end
                        + source[range.end..]
                            .find(|c: char| !c.is_whitespace())
                            .unwrap_or(source.len() - range.end);
                    edits.push((start..end, String::new()));
                }
            }
        }
    }
    if adding {
        anyhow::ensure!(
            builders > 0,
            "Built-in module must expose a mutable contract registry builder returning registry or Ok(registry)"
        );
        if !exported {
            let at = file
                .items
                .first()
                .map(|item| item.span().byte_range().start)
                .unwrap_or(source.len());
            edits.push((at..at, BUILTIN_CONTRACT_EXPORT.into()));
        }
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut output = source.to_owned();
    for (range, replacement) in edits {
        output.replace_range(range, &replacement);
    }
    syn::parse_file(&output)
        .context("Managed built-in contract registry edit produced invalid Rust")?;
    Ok(output)
}

fn inline_capability_module(source: &str) -> String {
    source.replace("pub use contracts::platform_events::{contracts, register_contracts};", "pub use contracts::platform_events::contract;")
        .replace("use forklaunch_axum::native::{ContractRouter, EndpointError, EndpointPolicy, PolicyCapabilities};", "use forklaunch_axum::native::{ContractRouter, EndpointError, EndpointPolicy, PolicyCapabilities, State};")
        .replace("use forklaunch_core::contract::RouteContract;", "use forklaunch_core::contract::{RouteContract, bindings::OriginalBodySignedPolicy};")
        .replace("impl EndpointPolicy<AppState> for CallbackPolicy {", "impl OriginalBodySignedPolicy for CallbackPolicy {}\nimpl EndpointPolicy<AppState> for CallbackPolicy {")
        .replace("self.bind_authorized(contracts::platform_events::endpoint(), CallbackPolicy(verifier),", "contract::Bindings { receive_platform_event: contract::ReceivePlatformEvent::bind_authorized(CallbackPolicy(verifier),")
        .replace("|_, context, _body|", "|_: State<AppState>, context: CallbackContext, _body: contract::PlatformEvent|")
        .replace("        })\n    }\n}", "        }) }.mount(self)\n    }\n}")
}

const INLINE_CONTRACT: &str =
    "forklaunch_core::contract::controller_contract!(platform_events, \"../../api/controllers/platform_events.rs\");";

/// Add or remove the platform-events contract in a private contract host,
/// as text: the rest of the file keeps its bytes, so removal returns the
/// host as it was before add.
fn inline_capability_registry(source: &str, adding: bool) -> Result<String> {
    use syn::{spanned::Spanned, visit::Visit};
    struct CapabilityReference(bool);
    impl<'ast> Visit<'ast> for CapabilityReference {
        fn visit_path(&mut self, path: &'ast syn::Path) {
            self.0 |= path
                .segments
                .first()
                .is_some_and(|segment| segment.ident == "platform_events");
            syn::visit::visit_path(self, path);
        }
    }
    let references = |statement: &syn::Stmt| {
        let mut found = CapabilityReference(false);
        found.visit_stmt(statement);
        found.0
    };
    let file = syn::parse_file(source)?;
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let contracts: Vec<_> = file
        .items
        .iter()
        .filter(|item| matches!(item, syn::Item::Macro(item) if item.mac.path.segments.last().is_some_and(|s| s.ident == "controller_contract") && item.mac.tokens.to_string().starts_with("platform_events ,")))
        .collect();
    if adding {
        if contracts.is_empty() {
            let addition = if source.is_empty() || source.ends_with('\n') {
                format!("{INLINE_CONTRACT}\n")
            } else {
                format!("\n{INLINE_CONTRACT}")
            };
            edits.push((source.len()..source.len(), addition));
        }
    } else {
        for item in contracts {
            let mut range = item.span().byte_range();
            if source[range.end..].starts_with('\n') {
                range.end += 1;
            } else if source[..range.start].ends_with('\n') {
                range.start -= 1;
            }
            edits.push((range, String::new()));
        }
    }
    for (name, hook) in [
        (
            "routes",
            "routes.extend(platform_events::contract::registry().routes().iter().cloned());",
        ),
        (
            "binding_manifest",
            "declarations.extend(platform_events::contract::binding_manifest()?);",
        ),
    ] {
        let function = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(item) if item.sig.ident == name => Some(item),
                _ => None,
            })
            .context("Private contract host is missing its collection function")?;
        let statements = &function.block.stmts;
        if adding {
            if statements.iter().any(references) {
                continue;
            }
            // Before the final return expression, separated from it like the
            // statement before it.
            let tail = statements.last().context("Contract collection is empty")?;
            let at = tail.span().byte_range().start;
            let gap_start = statements
                .len()
                .checked_sub(2)
                .map(|index| statements[index].span().byte_range().end)
                .unwrap_or_else(|| function.block.brace_token.span.open().byte_range().end);
            let gap = &source[gap_start..at];
            let separator = if gap.trim().is_empty() && !gap.is_empty() { gap } else { "\n" };
            edits.push((at..at, format!("{hook}{separator}")));
        } else {
            for statement in statements.iter().filter(|statement| references(statement)) {
                let range = statement.span().byte_range();
                let end = range.end
                    + source[range.end..]
                        .find(|c: char| !c.is_whitespace())
                        .unwrap_or(source.len() - range.end);
                edits.push((range.start..end, String::new()));
            }
        }
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut output = source.to_owned();
    for (range, replacement) in edits {
        output.replace_range(range, &replacement);
    }
    syn::parse_file(&output).context("Managed contract host edit produced invalid Rust")?;
    Ok(output)
}

/// Undo the router mount and its import. Add formats the file, which can
/// break the call chain over lines; a file still in rustfmt's form is
/// formatted again, so it returns to the form it had before add.
fn without_capability_router(path: &Path, source: &str, import: &str) -> String {
    let call = regex::Regex::new(r"\s*\.with_managed_capabilities\(\)\??").expect("fixed pattern");
    let stripped = call
        .replace_all(&source.replace(import, ""), "")
        .into_owned();
    if stripped == source {
        return stripped;
    }
    let format = |text: &str| crate::init::rust_project::format_rust_source(path, text);
    if format(source) == source {
        format(&stripped)
    } else {
        stripped
    }
}

fn with_native_capability_router(source: &str) -> Result<String> {
    anyhow::ensure!(
        source.matches(".finish()").count() == 1,
        "Native capability wiring requires one contract router finish"
    );
    Ok(source.replacen(".finish()", ".with_managed_capabilities()?.finish()", 1))
}

/// Insert `text` after the file's generated header and inner doc comments,
/// which must stay first.
fn after_inner_docs(source: &str, text: &str) -> String {
    let mut end = 0;
    let mut header = source.starts_with("/*");
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();
        if header {
            header = !trimmed.ends_with("*/");
        } else if !(trimmed.is_empty() || trimmed.starts_with("//!")) {
            break;
        }
        end += line.len();
    }
    format!("{}{text}{}", &source[..end], &source[end..])
}

fn with_capability_router(main: &str) -> Result<String> {
    let markers = [
        ".into_router_with_default_mcp()",
        ".serve_with_default_mcp(",
    ];
    if markers
        .iter()
        .map(|marker| main.matches(marker).count())
        .sum::<usize>()
        != 1
    {
        bail!("Native capability wiring requires one generated HTTP/MCP router mount");
    }
    let marker = markers
        .into_iter()
        .find(|marker| main.contains(marker))
        .context("Expected generated HTTP/MCP router mount")?;
    Ok(main.replacen(marker, &format!(".with_managed_capabilities(){marker}"), 1))
}

#[cfg(test)]
mod tests;
