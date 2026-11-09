//! Export TypeScript contracts using the service image's installed runtime and dependencies.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::contracts::{quote, resolved_output};
use crate::core::{
    application_output::output_path,
    artifact_paths::{self, Artifact},
};

fn artifact(root: &Path, project: &toml::Value, field: &str, default: String) -> Result<PathBuf> {
    let configured = project
        .get("metadata")
        .and_then(|metadata| {
            metadata.get(field).or_else(|| {
                metadata.get(if field == "openapiPath" {
                    "openapi_path"
                } else {
                    "contract_artifact_path"
                })
            })
        })
        .and_then(toml::Value::as_str)
        .unwrap_or(&default);
    let path = resolved_output(&output_path(root, Path::new(configured))?)?;
    if path.exists() && !path.is_file() {
        bail!("Development artifact must be a file");
    }
    Ok(path)
}

fn default_artifact(project: &toml::Value, name: &str, artifact: Artifact) -> String {
    artifact_paths::default_path(
        name,
        project.get("metadata").and_then(toml::Value::as_table),
        artifact,
    )
}

fn script(scratch: &Path, output: &Path, runtime: &str) -> Result<String> {
    let scratch = quote(scratch.to_str().context("Invalid scratch path")?);
    let parent = quote(
        output
            .parent()
            .context("Artifact parent missing")?
            .to_str()
            .context("Invalid artifact path")?,
    );
    let output = quote(output.to_str().context("Invalid artifact path")?);
    let export = if runtime == "bun" {
        "bun server.ts"
    } else {
        "node ./node_modules/tsx/dist/cli.mjs --tsconfig tsconfig.json server.ts"
    };
    Ok(format!(
        r#"mkdir -p {scratch} && export_dir=$(mktemp -d {scratch}/dev-ts-export-XXXXXX) && trap 'rm -rf "$export_dir"' EXIT HUP INT TERM && FORKLAUNCH_MODE=openapi FORKLAUNCH_OPENAPI_OUTPUT="$export_dir/openapi.json" {export} && test -s "$export_dir/openapi.json" && mkdir -p {parent} && (cmp -s "$export_dir/openapi.json" {output} || mv -f "$export_dir/openapi.json" {output}) && rm -f "$export_dir/openapi.json" && rmdir "$export_dir" && trap - EXIT HUP INT TERM && exec "$@""#
    ))
}

pub(super) fn enable(
    root: &Path,
    manifest: &toml::Value,
    active: &BTreeSet<String>,
    compose: &mut Value,
    native: &Value,
) -> Result<Value> {
    let root = root.canonicalize()?;
    let projects = manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .context("Projects missing")?;
    let modules = manifest
        .get("modules_path")
        .and_then(toml::Value::as_str)
        .unwrap_or(".");
    let modules = if modules == "." {
        root.clone()
    } else {
        output_path(&root, Path::new(modules))?
    };
    let runtime = manifest
        .get("runtime")
        .and_then(toml::Value::as_str)
        .unwrap_or("node");
    let mount = Path::new("/forklaunch-dev-artifacts");
    let mut owners = BTreeMap::new();
    for change in native
        .as_object()
        .context("Native changes missing")?
        .values()
    {
        let name = change["command"][0]
            .as_str()
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str())
            .context("Native binary missing")?;
        let project = projects
            .iter()
            .find(|project| project.get("name").and_then(toml::Value::as_str) == Some(name))
            .context("Native project missing")?;
        for (field, default) in [
            (
                "openapiPath",
                default_artifact(project, name, Artifact::OpenApi),
            ),
            (
                "contractArtifactPath",
                default_artifact(project, name, Artifact::Contract),
            ),
        ] {
            owners.insert(artifact(&root, project, field, default)?, name.to_owned());
        }
    }
    anyhow::ensure!(
        crate::core::forklaunch_state::file_stamped(&root.join("Dockerfile.typescript")) != Some(false),
        "Modified TypeScript image requires an explicit development profile"
    );
    // Generated production processes, recognized by their Compose commands.
    let production = if crate::core::forklaunch_state::file_stamped(&root.join("Dockerfile.typescript")).is_some() {
        crate::init::rust_project::typescript_planned_processes(&root)
    } else {
        Default::default()
    };
    let mut plans = Vec::new();
    let mut changes = serde_json::Map::new();
    for project in projects {
        let Some(name) = project.get("name").and_then(toml::Value::as_str) else {
            continue;
        };
        if !active.contains(name)
            || native.get(name).is_some()
            || !matches!(
                project.get("type").and_then(toml::Value::as_str),
                Some("Service" | "Worker")
            )
            || crate::core::openapi_export::OBSERVABILITY_SERVICES.contains(&name)
        {
            continue;
        }
        if project
            .get("metadata")
            .and_then(|metadata| metadata.get("language"))
            .and_then(toml::Value::as_str)
            == Some("rust")
        {
            continue;
        }
        let directory = output_path(&root, &modules.strip_prefix(&root)?.join(name))?;
        if !directory.join("server.ts").is_file() {
            bail!("TypeScript development export requires {name}/server.ts");
        }
        let destination = artifact(
            &root,
            project,
            "openapiPath",
            default_artifact(project, name, Artifact::OpenApi),
        )?;
        if let Some(previous) = owners.insert(destination.clone(), name.to_owned()) {
            bail!("Development artifact collision between {previous} and {name}");
        }
        let mut service = compose["services"][name].clone();
        if let Some((_, entry)) = production.get(name).filter(|(_, entry)| entry != "migrate") {
            anyhow::ensure!(
                matches!(runtime, "node" | "bun"),
                "Generated production profile requires Node or Bun development tooling"
            );
            let package: Value =
                serde_json::from_str(&fs::read_to_string(directory.join("package.json"))?)?;
            let script = if entry == "worker" {
                "dev:worker"
            } else if package["scripts"].get("dev:server").is_some() {
                "dev:server"
            } else {
                "dev"
            };
            anyhow::ensure!(
                package["scripts"][script].is_string(),
                "Development process script missing"
            );
            // The `<name>-migrate` job migrates as the database owner; the
            // service's restricted role cannot, so it skips the script's own step.
            let migrated = compose["services"].get(format!("{name}-migrate")).is_some();
            service["entrypoint"] = match package["scripts"][script]
                .as_str()
                .and_then(without_migrations)
                .filter(|_| migrated)
            {
                Some(command) => json!(["sh", "-c", command]),
                None => json!([if runtime == "bun" { "bun" } else { "pnpm" }, "run", script]),
            };
            service["command"] = json!([]);
            service["working_dir"] = json!(format!(
                "/app/{}/{}",
                modules.strip_prefix(&root)?.display(),
                name
            ));
            let mut build =
                json!({"context":root,"dockerfile":"Dockerfile.typescript","target":"builder"});
            // The builder stage still needs the production build's named
            // contexts and arguments (a `--framework-path` checkout, ...).
            // An image-only service takes them from its image's builder.
            let image = &compose["services"][name]["image"];
            let production = compose["services"][name]["build"]
                .as_object()
                .or_else(|| {
                    compose["services"].as_object()?.values().find_map(|candidate| {
                        (!image.is_null() && candidate["image"] == *image)
                            .then(|| candidate["build"].as_object())
                            .flatten()
                    })
                });
            for key in ["additional_contexts", "args"] {
                if let Some(value) = production.and_then(|build| build.get(key)) {
                    build[key] = value.clone();
                }
            }
            service["build"] = build;
            // The builder stage has no curl; probe health with the runtime.
            if let Some(url) = service["healthcheck"]["test"]
                .as_array()
                .filter(|test| test.len() == 4 && test[..3] == [json!("CMD"), json!("curl"), json!("-f")])
                .and_then(|test| test[3].as_str())
            {
                let probe = format!(
                    "fetch({}).then((r) => process.exit(r.ok ? 0 : 1), () => process.exit(1))",
                    serde_json::to_string(url)?
                );
                service["healthcheck"]["test"] =
                    json!(["CMD", if runtime == "bun" { "bun" } else { "node" }, "-e", probe]);
            }
            service["image"] = json!(format!(
                "{}-{name}-dev:local",
                compose["services"][name]["image"]
                    .as_str()
                    .unwrap_or("forklaunch-typescript")
                    .split(':')
                    .next()
                    .unwrap()
            ));
            if !service["volumes"].is_array() {
                service["volumes"] = json!([]);
            }
        }

        let workdir = PathBuf::from(
            service["working_dir"]
                .as_str()
                .context("TypeScript working directory missing")?,
        );
        anyhow::ensure!(
            workdir.is_absolute()
                && workdir.file_name().and_then(|part| part.to_str()) == Some(name),
            "TypeScript export requires a project working directory ending in {name}"
        );
        let workspace = workdir
            .parent()
            .context("TypeScript workspace directory missing")?;
        anyhow::ensure!(
            workspace != Path::new("/")
                && !workspace.starts_with(mount)
                && !mount.starts_with(workspace),
            "TypeScript workspace conflicts with development artifact mount"
        );
        let mut arguments = service["entrypoint"]
            .as_array()
            .filter(|values| !values.is_empty())
            .context("TypeScript development requires an explicit array entrypoint")?
            .clone();
        if let Some(command) = service.get("command").filter(|value| !value.is_null()) {
            arguments.extend(
                command
                    .as_array()
                    .context("TypeScript development command must be an array")?
                    .iter()
                    .cloned(),
            );
        }
        anyhow::ensure!(
            arguments.iter().all(Value::is_string),
            "TypeScript launch arguments must be strings"
        );
        let volumes = service["volumes"]
            .as_array_mut()
            .context("TypeScript service volumes missing")?;
        for (source, target) in [(&root, mount), (&modules, workspace)] {
            if let Some(existing) = volumes
                .iter()
                .find(|volume| volume["target"].as_str() == target.to_str())
            {
                anyhow::ensure!(
                    existing["type"] == "bind"
                        && existing["source"].as_str().map(Path::new) == Some(source.as_path()),
                    "TypeScript development mount conflicts with existing volume"
                );
            } else {
                volumes.push(json!({"type":"bind","source":source,"target":target}));
            }
        }
        // Keep dependencies and generated runtime code private to each container.
        // Multiple services can rebuild the same source workspace concurrently.
        let mut private_paths = vec![workspace.join("node_modules")];
        for entry in fs::read_dir(&modules)? {
            let entry = entry?;
            if entry.path().join("package.json").is_file() {
                for directory in ["node_modules", "lib", "dist"] {
                    private_paths.push(workspace.join(entry.file_name()).join(directory));
                }
            }
        }
        for target in private_paths {
            if !volumes
                .iter()
                .any(|volume| volume["target"].as_str() == target.to_str())
            {
                volumes.push(json!({"type":"volume","target":target}));
            }
        }
        let output = mount.join(destination.strip_prefix(&root)?);
        let scratch = resolved_output(&output_path(&root, Path::new(".forklaunch"))?)?;
        let build = if runtime == "bun" {
            "bun run tsc -b --force"
        } else {
            "pnpm exec tsc -b --force"
        };
        let startup = format!(
            "(cd {} && {build}) && {}",
            quote(workspace.to_str().context("Invalid workspace path")?),
            script(&mount.join(scratch.strip_prefix(&root)?), &output, runtime)?
        )
        .replace('$', "$$");
        let mut entrypoint = vec![json!("sh"), json!("-c"), json!(startup), json!("--")];
        entrypoint.extend(arguments);
        service["entrypoint"] = json!(entrypoint);
        service["command"] = json!([]);
        // Workspace binding makes shared source changes visible to exporters and servers.
        let develop = service
            .as_object_mut()
            .context("TypeScript service must be an object")?
            .entry("develop")
            .or_insert(json!({}));
        let watches = develop
            .as_object_mut()
            .context("TypeScript development settings must be an object")?
            .entry("watch")
            .or_insert(json!([]))
            .as_array_mut()
            .context("TypeScript development watches must be an array")?;
        let mut ignores: Vec<String> = [
            "node_modules/",
            "**/node_modules/",
            "dist/",
            "**/dist/",
            "lib/",
            "**/lib/",
            "target/",
            "**/target/",
            ".git/",
            "**/.git/",
            ".forklaunch/",
            "sdks/",
            "**/*.tsbuildinfo",
            "production/",
            "**/production/",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        ignores.extend(super::metadata_watch_ignores(&root, manifest, &modules)?);
        watches.push(json!({"action":"restart","path":modules,"ignore":ignores}));
        changes.insert(name.to_owned(), json!({"entrypoint":service["entrypoint"],"command":service["command"],"develop":service["develop"],"volumes":service["volumes"]}));
        plans.push((name.to_owned(), service));
    }
    for (name, service) in plans {
        compose["services"][name] = service;
    }
    Ok(Value::Object(changes))
}

/// A development script without its `migrate:up` steps, when it has any.
fn without_migrations(script: &str) -> Option<String> {
    let steps: Vec<&str> = script.split("&&").map(str::trim).collect();
    let kept: Vec<&str> = steps
        .iter()
        .copied()
        .filter(|step| !step.split_whitespace().any(|word| word == "migrate:up"))
        .collect();
    (kept.len() < steps.len() && !kept.is_empty()).then(|| kept.join(" && "))
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn development_scripts_drop_their_migration_step() {
        assert_eq!(
            without_migrations("DOTENV_FILE_PATH=.env.local pnpm run migrate:up && DOTENV_FILE_PATH=.env.local pnpm tsx watch server.ts").as_deref(),
            Some("DOTENV_FILE_PATH=.env.local pnpm tsx watch server.ts")
        );
        assert_eq!(without_migrations("pnpm migrate:up && pnpm tsx watch server.ts").as_deref(), Some("pnpm tsx watch server.ts"));
        assert_eq!(without_migrations("pnpm tsx watch server.ts"), None);
        assert_eq!(without_migrations("pnpm migrate:up"), None);
    }

    #[test]
    fn migrated_services_start_without_their_own_migration_step() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("modules/api")).unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        fs::write(root.path().join("modules/api/server.ts"), "").unwrap();
        fs::write(
            root.path().join("modules/api/package.json"),
            r#"{"scripts":{"dev":"pnpm migrate:up && pnpm tsx watch server.ts"}}"#,
        )
        .unwrap();
        let plan = json!({"services": {"api": {"directory": "api", "entry": "server"}, "api-migrate": {"directory": "api", "entry": "migrate"}}});
        fs::write(
            root.path().join("Dockerfile.typescript"),
            crate::core::forklaunch_state::stamp(&format!("COPY <<\"FORKLAUNCH_EOF\" /forklaunch/production.json\n{plan}\nFORKLAUNCH_EOF\n")),
        )
        .unwrap();
        let manifest: toml::Value =
            toml::from_str("modules_path='modules'\nruntime='node'\n[[projects]]\nname='api'\ntype='Service'\n").unwrap();
        let mut compose = json!({"services":{"api":{"image":"demo-typescript:local","working_dir":"/app/api","command":["node","/app/api/production/server.mjs"],"volumes":[],"build":{"context":".","dockerfile":"Dockerfile.typescript"}},"api-migrate":{"image":"demo-typescript:local","command":["node","/app/api/production/migrate.mjs"]}}});
        enable(root.path(), &manifest, &BTreeSet::from(["api".into()]), &mut compose, &json!({})).unwrap();
        let entrypoint = compose["services"]["api"]["entrypoint"].as_array().unwrap();
        assert_eq!(entrypoint[entrypoint.len() - 3..], [json!("sh"), json!("-c"), json!("pnpm tsx watch server.ts")]);
    }

    #[test]
    fn owned_production_process_uses_isolated_builder_image_for_development() {
        for (runtime, manager) in [("node", "pnpm"), ("bun", "bun")] {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir_all(root.path().join("modules/api")).unwrap();
            fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
            fs::write(root.path().join("modules/api/server.ts"), "").unwrap();
            fs::write(
                root.path().join("modules/api/package.json"),
                format!(r#"{{"scripts":{{"dev":"{manager} run server.ts"}}}}"#),
            )
            .unwrap();
            let plan = json!({"services": {"api": {"directory": "api", "entry": "server"}}});
            fs::write(
                root.path().join("Dockerfile.typescript"),
                crate::core::forklaunch_state::stamp(&format!("COPY <<\"FORKLAUNCH_EOF\" /forklaunch/production.json\n{plan}\nFORKLAUNCH_EOF\n")),
            )
            .unwrap();
            let manifest: toml::Value = toml::from_str(
            &format!("modules_path='modules'\nruntime='{runtime}'\n[[projects]]\nname='api'\ntype='Service'\n"),
        )
        .unwrap();
            let mut compose = json!({"services":{"api":{"image":"demo-typescript:local","working_dir":"/app/api","healthcheck":{"test":["CMD","curl","-f","http://localhost:8000/health"],"interval":"30s"},"command":["node","--enable-source-maps","/app/api/production/server.mjs"],"volumes":[]},"api-migrate":{"image":"demo-typescript:local","command":["node","/app/api/production/migrate.mjs"],"build":{"context":".","dockerfile":"Dockerfile.typescript","additional_contexts":{"framework":"/checkout/framework-rs"},"args":{"FRAMEWORK_ROOT":"/checkout"}}}}});
            enable(
                root.path(),
                &manifest,
                &BTreeSet::from(["api".into()]),
                &mut compose,
                &json!({}),
            )
            .unwrap();
            assert_eq!(compose["services"]["api"]["build"]["target"], "builder");
            // The builder image has no curl; health is probed with the runtime.
            let test = &compose["services"]["api"]["healthcheck"]["test"];
            assert_eq!(test[1], if runtime == "bun" { "bun" } else { "node" });
            assert!(test[3].as_str().unwrap().starts_with("fetch(\"http://localhost:8000/health\")"));
            assert_eq!(compose["services"]["api"]["healthcheck"]["interval"], "30s");
            // Named contexts and arguments of the image's build carry over.
            assert_eq!(compose["services"]["api"]["build"]["additional_contexts"]["framework"], "/checkout/framework-rs");
            assert_eq!(compose["services"]["api"]["build"]["args"]["FRAMEWORK_ROOT"], "/checkout");
            assert_eq!(
                compose["services"]["api"]["image"],
                "demo-typescript-api-dev:local"
            );
            assert_eq!(
                compose["services"]["api"]["working_dir"],
                "/app/modules/api"
            );
            assert_eq!(compose["services"]["api"]["entrypoint"][4], manager);
            assert!(
                compose["services"]["api"]["entrypoint"][2]
                    .as_str()
                    .unwrap()
                    .contains(if runtime == "bun" {
                        "bun server.ts"
                    } else {
                        "node ./node_modules/tsx"
                    })
            );
            assert_eq!(
                compose["services"]["api-migrate"]["image"],
                "demo-typescript:local"
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn failed_typescript_exports_preserve_artifacts_and_do_not_start_service() {
        use std::{os::unix::fs::PermissionsExt, process::Command};
        let root = tempfile::tempdir().unwrap();
        let runtime = root.path().join("bun");
        fs::write(&runtime, "#!/bin/sh\nprintf '%s' \"$DOCUMENT\" > \"$FORKLAUNCH_OPENAPI_OUTPUT\"\nexit \"$EXPORT_STATUS\"\n").unwrap();
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o755)).unwrap();
        let output = root.path().join("quoted ' artifacts/openapi.json");
        let scratch = root.path().join("scratch");
        fs::create_dir_all(output.parent().unwrap()).unwrap();
        fs::write(&output, "original").unwrap();
        let modified = fs::metadata(&output).unwrap().modified().unwrap();
        let startup = script(&scratch, &output, "bun").unwrap();
        let run = |document: &str, status: &str| {
            Command::new("sh")
                .args(["-c", &startup, "--", "printf", "started"])
                .env(
                    "PATH",
                    format!(
                        "{}:{}",
                        root.path().display(),
                        std::env::var("PATH").unwrap()
                    ),
                )
                .env("DOCUMENT", document)
                .env("EXPORT_STATUS", status)
                .output()
                .unwrap()
        };
        for (document, status) in [("partial", "1"), ("", "0")] {
            let result = run(document, status);
            assert!(!result.status.success());
            assert!(result.stdout.is_empty());
            assert_eq!(fs::read_to_string(&output).unwrap(), "original");
            assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
        }
        assert!(run("original", "0").status.success());
        assert_eq!(fs::metadata(&output).unwrap().modified().unwrap(), modified);
        let result = run("updated", "0");
        assert!(result.status.success());
        assert_eq!(result.stdout, b"started");
        assert_eq!(fs::read_to_string(&output).unwrap(), "updated");
        assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
    }

    #[test]
    fn plans_contained_mixed_exports_without_writing_and_rejects_collisions() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("modules/api")).unwrap();
        fs::write(root.path().join("modules/api/server.ts"), "").unwrap();
        fs::write(root.path().join("modules/api/package.json"), "{}").unwrap();
        let mut manifest: toml::Value = toml::from_str("modules_path='modules'\nruntime='bun'\n[[projects]]\nname='api'\ntype='Service'\n[projects.metadata]\nopenapiPath='custom/api.json'\n").unwrap();
        let base = json!({"services":{"api":{"working_dir":"/test/api","entrypoint":["bun","run","dev"],"volumes":[]}}});
        let mut planned = base.clone();
        let changes = enable(
            root.path(),
            &manifest,
            &BTreeSet::from(["api".into()]),
            &mut planned,
            &json!({}),
        )
        .unwrap();
        assert!(
            changes["api"]["entrypoint"][2]
                .as_str()
                .unwrap()
                .contains("bun server.ts")
        );
        assert!(
            changes["api"]["entrypoint"][2]
                .as_str()
                .unwrap()
                .contains("$$export_dir")
        );
        assert_eq!(changes["api"]["entrypoint"][4], "bun");
        let volumes = changes["api"]["volumes"].as_array().unwrap();
        for target in ["/test/node_modules", "/test/api/lib", "/test/api/dist"] {
            assert!(
                volumes
                    .iter()
                    .any(|volume| volume["type"] == "volume" && volume["target"] == target)
            );
        }

        assert!(!root.path().join("custom").exists());
        let mut collision = manifest.clone();
        collision["projects"].as_array_mut().unwrap().push(toml::from_str("name='native-api'\ntype='Service'\n[metadata]\nlanguage='rust'\nopenapiPath='custom/api.json'\n").unwrap());
        let mut untouched = base.clone();
        assert!(
            enable(
                root.path(),
                &collision,
                &BTreeSet::from(["api".into()]),
                &mut untouched,
                &json!({"native-api":{"command":["/app/target/debug/native-api"]}})
            )
            .is_err()
        );
        assert_eq!(untouched, base);
        manifest["projects"][0]["metadata"]["openapiPath"] =
            toml::Value::String("../outside".into());
        let mut planned = base.clone();
        assert!(
            enable(
                root.path(),
                &manifest,
                &BTreeSet::from(["api".into()]),
                &mut planned,
                &json!({})
            )
            .is_err()
        );
        assert_eq!(planned, base);
    }
}
