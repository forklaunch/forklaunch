//! In-place native Redis/S3 wiring. Removal is stateless: it takes back what
//! `infra add` would write for the service today, and keeps anything edited.
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::ArgMatches;
use indexmap::IndexMap;
use toml_edit::DocumentMut;

use super::{
    capabilities::{
        CapabilityEdit, dependency_in_use, ensure_native_dependency, release_dependency,
    },
    in_place,
};
use crate::{
    constants::Infrastructure,
    core::{
        docker::{
            DependencyCondition, DependsOn, DockerCompose, MINIO_INIT, add_redis_to_docker_compose,
            add_s3_to_docker_compose, ensure_minio_buckets, minio_buckets,
        },
        manifest::{ProjectEntry, application::ApplicationManifestData},
    },
};

const REDIS: &str = include_str!("../templates/rust/infrastructure/redis.rs");
const S3: &str = include_str!("../templates/rust/infrastructure/s3.rs");

pub(crate) fn requested(matches: &ArgMatches) -> Option<Vec<String>> {
    matches
        .try_contains_id("infrastructure")
        .unwrap_or(false)
        .then(|| {
            matches
                .get_many::<String>("infrastructure")
                .into_iter()
                .flatten()
                .cloned()
                .collect()
        })
}

pub(crate) fn reconcile(
    matches: &ArgMatches,
    root: &Path,
    service: &str,
    desired: &[String],
    replace: bool,
) -> Result<()> {
    for wanted in [Infrastructure::Redis, Infrastructure::S3] {
        let id = wanted.to_string();
        let manifest: ApplicationManifestData = toml::from_str(&std::fs::read_to_string(
            root.join(".forklaunch/manifest.toml"),
        )?)?;
        let project = manifest
            .projects
            .iter()
            .find(|project| project.name == service)
            .context("Native infrastructure project is missing")?;
        let present = project
            .resources
            .as_ref()
            .is_some_and(|resources| match wanted {
                Infrastructure::Redis => resources.cache.as_deref() == Some(id.as_str()),
                Infrastructure::S3 => resources.object_store.as_deref() == Some(id.as_str()),
            });
        let adding = desired.contains(&id);
        if adding != present && (adding || replace) {
            change(matches, root, &manifest, project, wanted, adding)?;
        }
    }
    Ok(())
}

pub(crate) fn service_path(root: &Path, project: &ProjectEntry) -> Result<PathBuf> {
    let default = format!("crates/{}", project.name);
    let relative = Path::new(
        project
            .metadata
            .as_ref()
            .and_then(|meta| meta.crate_path.as_deref())
            .unwrap_or(&default),
    );
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        bail!("Native infrastructure crate path must remain within the application");
    }
    let path = root.join(relative);
    if !path.canonicalize()?.starts_with(root.canonicalize()?) {
        bail!("Native infrastructure crate path must remain within the application");
    }
    Ok(path)
}

/// The compose value of `ENCRYPTION_KEY` for the encrypted cache and object
/// store helpers; `.env` holds the key.
pub(crate) const ENCRYPTION_KEY_REFERENCE: &str =
    "${ENCRYPTION_KEY:?Configure a field encryption key}";

/// The Redis database index a compose `REDIS_URL` add writes selects.
fn generated_redis_database(url: &str) -> Option<u32> {
    url.strip_prefix("redis://redis:6379/")?.parse().ok()
}

/// The first Redis database no other compose service uses, so the project's
/// keys never collide with another's.
fn free_redis_database(compose: &DockerCompose, keys: &[String]) -> u32 {
    let taken: std::collections::BTreeSet<u32> = compose
        .services
        .iter()
        .filter(|(name, _)| !keys.contains(name))
        .filter_map(|(_, service)| service.environment.as_ref()?.get("REDIS_URL"))
        .filter_map(|url| {
            let rest = url.strip_prefix("redis://redis:6379")?;
            match rest.strip_prefix('/') {
                Some(index) => index.parse().ok(),
                None => rest.is_empty().then_some(0),
            }
        })
        .collect();
    (0..)
        .find(|index| !taken.contains(index))
        .unwrap_or_default()
}

/// The local field encryption key: `.env`'s, else one the application's
/// projects already use (so their values stay readable to each other), else
/// a new one.
fn local_encryption_key(root: &Path, modules_path: &str) -> String {
    use base64::Engine;
    let from_env = std::fs::read_to_string(root.join(".env"))
        .ok()
        .and_then(|text| {
            text.lines()
                .filter(|line| !line.trim_start().starts_with('#'))
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("ENCRYPTION_KEY=")
                        .map(str::to_owned)
                })
                .filter(|value| !value.is_empty())
        });
    from_env
        .or_else(|| {
            crate::core::env_defaults::find_existing_encryption_key(&root.join(modules_path))
        })
        .unwrap_or_else(|| {
            let mut key = [0u8; 32];
            getrandom::fill(&mut key).expect("Failed to generate random bytes");
            base64::engine::general_purpose::STANDARD.encode(key)
        })
}

pub(crate) fn change(
    matches: &ArgMatches,
    root: &Path,
    manifest: &ApplicationManifestData,
    project: &ProjectEntry,
    wanted: Infrastructure,
    adding: bool,
) -> Result<()> {
    let id = wanted.to_string();
    let (resource, backend, feature, generated) = match wanted {
        Infrastructure::Redis => ("cache", "redis", "redis-client", REDIS),
        Infrastructure::S3 => ("object_store", "minio", "aws-sdk", S3),
    };
    let dependency = format!("forklaunch-infrastructure-{id}");
    let path = service_path(root, project)?;
    let mut edit = CapabilityEdit::new(
        root,
        &manifest.app_name,
        &project.name,
        &path,
        vec![],
        vec![],
    );
    edit.native = true;
    edit.format_rust = adding;
    let cargo_path = path.join("Cargo.toml");
    let source_root = if path.join("lib.rs").is_file() {
        path.clone()
    } else {
        path.join("src")
    };
    let lib_path = source_root.join("lib.rs");
    let source_path = source_root.join(format!("infrastructure/{id}.rs"));
    let export = format!("#[path = \"infrastructure/{id}.rs\"]\npub mod {id}_infrastructure;\n");
    // The other encrypted helper, which shares ENCRYPTION_KEY.
    let other_helper = source_root.join(match wanted {
        Infrastructure::Redis => "infrastructure/s3.rs",
        Infrastructure::S3 => "infrastructure/redis.rs",
    });
    let other_helper_present = edit.read(&other_helper)?.is_some();
    let cargo_text = edit
        .read(&cargo_path)?
        .context("Native service Cargo.toml is missing")?;
    let mut cargo: DocumentMut = cargo_text.parse()?;
    let mut lib = edit
        .read(&lib_path)?
        .context("Native service library export file is missing")?;
    let source = edit.read(&source_path)?;
    let compose_path = root.join("docker-compose.yaml");
    let compose_text = edit.read(&compose_path)?;
    let mut compose = compose_text
        .as_deref()
        .map(serde_yml::from_str::<DockerCompose>)
        .transpose()?;
    // One-shot migrate/seed jobs run the project's binary with its settings,
    // so they gain and lose infrastructure settings with it.
    let keys = compose
        .as_ref()
        .map(|compose| {
            let mut keys = super::capabilities::compose_processes(compose, &project.name);
            keys.extend(
                [format!("{}-migrate", project.name), format!("{}-seed", project.name)]
                    .into_iter()
                    .filter(|job| compose.services.contains_key(job)),
            );
            keys
        })
        .unwrap_or_default();
    let bucket = format!("{}-{}-dev", manifest.app_name, project.name);
    // The Redis database: the one the project's processes already select,
    // else a free one.
    let redis_database = compose.as_ref().map_or(0, |compose| {
        keys.iter()
            .filter_map(|key| {
                compose
                    .services
                    .get(key)?
                    .environment
                    .as_ref()?
                    .get("REDIS_URL")
            })
            .find_map(|url| generated_redis_database(url))
            .unwrap_or_else(|| free_redis_database(compose, &keys))
    });
    let encryption_key = local_encryption_key(root, &manifest.modules_path);
    let redis_url = format!("redis://localhost:6379/{redis_database}");
    let mut env: Vec<(&str, &str)> = match wanted {
        Infrastructure::Redis => vec![("REDIS_URL", &redis_url)],
        Infrastructure::S3 => vec![
            ("S3_URL", "http://localhost:9000"),
            ("S3_BUCKET", &bucket),
            ("S3_REGION", "us-east-1"),
            ("S3_ACCESS_KEY_ID", "minioadmin"),
            ("S3_SECRET_ACCESS_KEY", "minioadmin"),
        ],
    };
    // Kept while the other encrypted helper still reads it.
    if adding || !other_helper_present {
        env.push(("ENCRYPTION_KEY", &encryption_key));
    }
    let add_dependency = |cargo: &mut DocumentMut| -> Result<()> {
        let target = root.join(".forklaunch/framework-rs/crates").join(&id);
        let relative = crate::core::relative_path::get_relative_path(&path, &target);
        for feature in [feature, "encryption"] {
            ensure_native_dependency(
                cargo,
                &dependency,
                &relative.to_string_lossy(),
                Some(feature),
            )?;
        }
        Ok(())
    };
    if adding {
        add_dependency(&mut cargo)?;
        if source.is_none() {
            edit.write(source_path.clone(), generated.into());
        }
        if !lib.contains(&export) {
            insert_module_export(&mut lib, &export, &format!("{id}_infrastructure"));
        }
        edit.ensure_env_local(&env)?;
        // Compose reads the key from `.env`, which keeps it after removal so
        // values encrypted under it stay readable.
        let env_path = root.join(".env");
        let current = edit.read(&env_path)?.unwrap_or_default();
        let present = current
            .lines()
            .any(|line| in_place::env_key(line) == Some("ENCRYPTION_KEY"));
        if compose.is_some() && !present {
            let separator = if current.is_empty() || current.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            edit.write(
                env_path,
                format!("{current}{separator}ENCRYPTION_KEY={encryption_key}\n"),
            );
        }
    } else {
        // An edited factory is the developer's, and can still reference the
        // adapter; it keeps its module export and dependency.
        let kept = source
            .as_deref()
            .is_some_and(|text| !edit.is_generated(&source_path, text, generated));
        if kept {
            edit.note_kept(&source_path);
        } else {
            edit.delete(source_path.clone());
            lib = lib.replace(&export, "");
            if !dependency_in_use(
                &path,
                &cargo,
                &dependency,
                std::slice::from_ref(&source_path),
            ) {
                release_dependency(
                    &mut edit,
                    &cargo_path,
                    &mut cargo,
                    &dependency,
                    add_dependency,
                )?;
            }
        }
        edit.release_env_local(&env)?;
    }
    if let Some(compose) = &mut compose {
        anyhow::ensure!(
            !keys.is_empty(),
            "'{}' is missing from docker-compose.yaml",
            project.name
        );
        let add_backend = |compose: &mut DockerCompose,
                           environment: &mut IndexMap<String, String>| {
            match wanted {
                Infrastructure::Redis => add_redis_to_docker_compose(
                    &manifest.app_name,
                    compose,
                    environment,
                    redis_database,
                )
                .map(|_| ()),
                Infrastructure::S3 => add_s3_to_docker_compose(
                    &manifest.app_name,
                    &project.name,
                    compose,
                    environment,
                )
                .map(|_| ()),
            }
        };
        let mut generated_compose = DockerCompose::default();
        let mut generated_env = IndexMap::new();
        add_backend(&mut generated_compose, &mut generated_env)?;
        if adding || !other_helper_present {
            generated_env.insert("ENCRYPTION_KEY".into(), ENCRYPTION_KEY_REFERENCE.into());
        }
        // Each process waits for the backend: Redis healthy, the bucket created.
        let (ready, condition) = match wanted {
            Infrastructure::Redis => (backend, DependencyCondition::ServiceHealthy),
            Infrastructure::S3 => (
                MINIO_INIT,
                DependencyCondition::ServiceCompletedSuccessfully,
            ),
        };
        let generated_dependency = DependsOn {
            condition,
            ..Default::default()
        };
        if adding {
            add_backend(compose, &mut IndexMap::new())?;
            if wanted == Infrastructure::S3 {
                ensure_minio_buckets(compose, &manifest.app_name, &[&bucket]);
            }
        }
        for key in &keys {
            let Some(service) = compose.services.get_mut(key) else {
                continue;
            };
            let environment = service.environment.get_or_insert_with(IndexMap::new);
            if adding {
                // Values the service already sets are the application's.
                for (name, value) in &generated_env {
                    environment
                        .entry(name.clone())
                        .or_insert_with(|| value.clone());
                }
                service
                    .depends_on
                    .get_or_insert_with(IndexMap::new)
                    .entry(ready.to_owned())
                    .or_insert_with(|| generated_dependency.clone());
            } else {
                for (name, value) in &generated_env {
                    let ours = match environment.get(name) {
                        Some(current) if name == "REDIS_URL" => {
                            generated_redis_database(current).is_some()
                        }
                        Some(current) => current == value,
                        None => continue,
                    };
                    if ours {
                        environment.shift_remove(name);
                    } else {
                        edit.note_kept_setting(&compose_path, &format!("{key} {name}"));
                    }
                }
                if let Some(dependencies) = service.depends_on.as_mut() {
                    if dependencies
                        .get(ready)
                        .map(serde_json::to_value)
                        .transpose()?
                        == Some(serde_json::to_value(&generated_dependency)?)
                    {
                        dependencies.shift_remove(ready);
                    }
                    if dependencies.is_empty() {
                        service.depends_on = None;
                    }
                }
            }
        }
        let remaining = manifest.projects.iter().any(|other| {
            other.name != project.name
                && other
                    .resources
                    .as_ref()
                    .is_some_and(|resources| match wanted {
                        Infrastructure::Redis => resources.cache.as_deref() == Some("redis"),
                        Infrastructure::S3 => resources.object_store.as_deref() == Some("s3"),
                    })
        });
        if !adding && wanted == Infrastructure::S3 {
            release_minio_bucket(compose, &manifest.app_name, &bucket);
        }
        if !adding && !remaining {
            release_backend(&mut edit, compose, &generated_compose, backend)?;
        }
        let referenced = compose.services.values().any(|service| {
            service
                .environment
                .as_ref()
                .is_some_and(|env| env.values().any(|value| value.contains("${ENCRYPTION_KEY")))
        });
        if !adding && !referenced {
            eprintln!(
                "note: kept ENCRYPTION_KEY in .env, so values already encrypted under it stay readable if a cache or object store is added again"
            );
        }
        let header: String = compose_text
            .as_deref()
            .unwrap_or_default()
            .split_inclusive('\n')
            .take_while(|line| line.starts_with('#') || line.trim().is_empty())
            .collect();
        let updated = format!("{header}{}", serde_yml::to_string(compose)?);
        if compose_text.as_deref() != Some(updated.as_str()) {
            edit.write(compose_path, updated);
        }
    }
    let updated_cargo = if adding {
        crate::init::rust_project::dependencies::render_for_path(&cargo_path, &cargo.to_string())?
    } else {
        cargo.to_string()
    };
    if updated_cargo != cargo_text {
        edit.write(cargo_path, updated_cargo);
    }
    edit.write(lib_path, lib);
    let manifest_path = root.join(".forklaunch/manifest.toml");
    let manifest_text = edit.read(&manifest_path)?.context("Manifest is missing")?;
    edit.write(
        manifest_path,
        in_place::set_manifest_resource(
            &manifest_text,
            &project.name,
            resource,
            adding.then_some(id.as_str()),
        )?,
    );
    if matches.get_flag("dryrun") {
        for (path, writes) in edit.changed_paths() {
            println!(
                "{} {}",
                if writes { "write " } else { "delete" },
                path.strip_prefix(root).unwrap_or(&path).display()
            );
        }
        return Ok(());
    }
    if adding {
        crate::init::rust_project::ensure_native_infrastructure_sources(root)?;
    }
    edit.commit()?;
    refresh_lockfile(root);
    println!(
        "{} {id} {} {}",
        if adding { "Added" } else { "Removed" },
        if adding { "to" } else { "from" },
        project.name
    );
    if adding {
        let usage = match wanted {
            Infrastructure::Redis => "connect(), encrypted_cache() or encrypted_queue(name)",
            Infrastructure::S3 => {
                "object_store(), configured_object_store() or encrypted_object_store()"
            }
        };
        println!(
            "  {}: call crate::{id}_infrastructure::{usage} from the project's code",
            source_path
                .strip_prefix(root)
                .unwrap_or(&source_path)
                .display()
        );
    }
    Ok(())
}

/// Bring the workspace `Cargo.lock` in line with the edited manifests: the
/// generated Rust image builds `--locked`. Resolution adds only the new
/// dependencies' entries and prunes the ones a removal left unused.
pub(crate) fn refresh_lockfile(root: &Path) {
    if !root.join("Cargo.lock").is_file() || !root.join("Cargo.toml").is_file() {
        return;
    }
    let refreshed = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !refreshed {
        eprintln!(
            "note: Cargo.lock was not updated; run `cargo update --workspace` before `docker compose up --build`, which builds --locked"
        );
    }
}

/// Undo [`ensure_minio_buckets`] for `bucket` once no compose service uses
/// it; `minio-init` goes with its last bucket.
fn release_minio_bucket(compose: &mut DockerCompose, app_name: &str, bucket: &str) {
    let used = compose.services.iter().any(|(name, service)| {
        name != MINIO_INIT
            && service
                .environment
                .as_ref()
                .and_then(|env| env.get("S3_BUCKET"))
                .is_some_and(|value| value == bucket)
    });
    let buckets = minio_buckets(compose);
    if used || !buckets.iter().any(|existing| existing == bucket) {
        return;
    }
    let remaining: Vec<&str> = buckets
        .iter()
        .map(String::as_str)
        .filter(|existing| *existing != bucket)
        .collect();
    if remaining.is_empty() {
        compose.services.shift_remove(MINIO_INIT);
        for service in compose.services.values_mut() {
            if let Some(dependencies) = service.depends_on.as_mut() {
                dependencies.shift_remove(MINIO_INIT);
            }
        }
        return;
    }
    // Rewritten from the remaining buckets, as add wrote it.
    let mut scratch = DockerCompose::default();
    ensure_minio_buckets(&mut scratch, app_name, &remaining);
    let entrypoint = scratch.services[MINIO_INIT].entrypoint.clone();
    if let Some(service) = compose.services.get_mut(MINIO_INIT) {
        service.entrypoint = entrypoint;
    }
}

/// The shared compose backend (`redis`, `minio`) `infra add` creates, removed
/// once no project uses it: only while nothing else references it and it is
/// still the generated service.
fn release_backend(
    edit: &mut CapabilityEdit,
    compose: &mut DockerCompose,
    generated: &DockerCompose,
    backend: &str,
) -> Result<()> {
    let Some(current) = compose.services.get(backend) else {
        return Ok(());
    };
    if crate::core::docker::service_still_referenced(compose, backend) {
        return Ok(());
    }
    // `init --infrastructure` and `infra add` serialize the same backend with
    // and without empty fields (`environment: {}`); neither is a user edit.
    if without_empty(serde_json::to_value(current)?)
        == without_empty(serde_json::to_value(generated.services.get(backend))?)
    {
        compose.services.shift_remove(backend);
        // And the named volumes it brought, once no other service mounts them.
        for (name, volume) in &generated.volumes {
            let mounted = compose.services.values().any(|service| {
                service.volumes.iter().flatten().any(|mount| {
                    mount.split_once(':').map_or(mount.as_str(), |(source, _)| source) == name
                })
            });
            if !mounted
                && compose.volumes.get(name).map(serde_json::to_value).transpose()?
                    == Some(serde_json::to_value(volume)?)
            {
                compose.volumes.shift_remove(name);
            }
        }
    } else {
        let compose_path = edit.app_root.join("docker-compose.yaml");
        edit.note_kept_setting(&compose_path, &format!("service {backend}"));
    }
    Ok(())
}

/// A compose value without null, empty-object and empty-array fields.
fn without_empty(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, without_empty(value)))
                .filter(|(_, value)| match value {
                    serde_json::Value::Null => false,
                    serde_json::Value::Object(map) => !map.is_empty(),
                    serde_json::Value::Array(items) => !items.is_empty(),
                    _ => true,
                })
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(without_empty).collect())
        }
        other => other,
    }
}

/// Insert where rustfmt's module ordering expects it, so `cargo fmt --check`
/// passes on the generated library without reformatting user code.
pub(crate) fn insert_module_export(lib: &mut String, export: &str, name: &str) {
    let lines: Vec<&str> = lib.split_inclusive('\n').collect();
    let module = |line: &str| {
        line.trim_end()
            .strip_prefix("pub mod ")
            .and_then(|rest| rest.strip_suffix(';'))
            .map(str::to_owned)
    };
    let declarations: Vec<(usize, String)> =
        lines.iter().enumerate().filter_map(|(index, line)| Some((index, module(line)?))).collect();
    let attributes_before = |mut index: usize| {
        while index > 0 && lines[index - 1].starts_with("#[") {
            index -= 1;
        }
        index
    };
    let at = match declarations.iter().find(|(_, existing)| existing.as_str() > name) {
        Some((index, _)) => attributes_before(*index),
        None => match declarations.last() {
            Some((index, _)) => index + 1,
            None => {
                lib.push_str(if lib.is_empty() || lib.ends_with('\n') { "" } else { "\n" });
                lib.push_str(export);
                return;
            }
        },
    };
    let offset: usize = lines[..at].iter().map(|line| line.len()).sum();
    lib.insert_str(offset, export);
}

/// After `delete service`: release the shared compose backends the deleted
/// service was the last user of (see [`release_backend`]).
pub(crate) fn release_deleted_service(root: &Path) -> Result<()> {
    let compose_path = root.join("docker-compose.yaml");
    if !compose_path.exists() {
        return Ok(());
    }
    let manifest: ApplicationManifestData =
        toml::from_str(&std::fs::read_to_string(root.join(".forklaunch/manifest.toml"))?)?;
    let text = std::fs::read_to_string(&compose_path)?;
    let mut compose: DockerCompose = serde_yml::from_str(&text)?;
    let before = serde_yml::to_string(&compose)?;
    let mut edit = CapabilityEdit::new(root, &manifest.app_name, "", root, vec![], vec![]);
    for wanted in [Infrastructure::Redis, Infrastructure::S3] {
        let id = wanted.to_string();
        let remaining = manifest.projects.iter().any(|project| {
            project.resources.as_ref().is_some_and(|resources| match wanted {
                Infrastructure::Redis => resources.cache.as_deref() == Some(id.as_str()),
                Infrastructure::S3 => resources.object_store.as_deref() == Some(id.as_str()),
            })
        });
        if remaining {
            continue;
        }
        let mut generated = DockerCompose::default();
        let mut environment = IndexMap::new();
        let backend = match wanted {
            Infrastructure::Redis => {
                add_redis_to_docker_compose(&manifest.app_name, &mut generated, &mut environment, 0)?;
                "redis"
            }
            Infrastructure::S3 => {
                add_s3_to_docker_compose(&manifest.app_name, "", &mut generated, &mut environment)?;
                "minio"
            }
        };
        release_backend(&mut edit, &mut compose, &generated, backend)?;
    }
    let after = serde_yml::to_string(&compose)?;
    if after != before {
        let header: String = text
            .split_inclusive('\n')
            .take_while(|line| line.starts_with('#') || line.trim().is_empty())
            .collect();
        std::fs::write(compose_path, format!("{header}{after}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
