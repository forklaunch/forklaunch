//! Opt-in compiler evidence. Never infer runtime/security guarantees from a build.
mod assembly;
mod provenance;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::Serialize;

use crate::core::{
    manifest::{ProjectEntry, ProjectType},
    static_analysis::workspace_diagnostics::{self, Analysis, Diagnostic},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Verification {
    mode: &'static str,
    configuration: &'static str,
    checks: Vec<Check>,
    required_not_checked: Vec<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Check {
    kind: &'static str,
    project: String,
    command: Vec<String>,
    status: &'static str,
    exit_code: Option<i32>,
    message: Option<String>,
}

fn execute(
    root: &Path,
    kind: &'static str,
    project: &str,
    program: &str,
    args: &[String],
) -> Check {
    eprintln!("Verifying {kind} for {project}");
    let mut command = vec![program.to_owned()];
    command.extend_from_slice(args);
    // Child output must not corrupt the enclosing JSON report. Inherit stderr
    // for compiler diagnostics without retaining potentially unbounded output.
    let result = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(std::io::stderr()))
        .stderr(Stdio::inherit())
        .status();
    match result {
        Ok(status) => Check {
            kind,
            project: project.into(),
            command,
            status: if status.success() { "passed" } else { "failed" },
            exit_code: status.code(),
            message: None,
        },
        Err(error) => Check {
            kind,
            project: project.into(),
            command,
            status: "not_checked",
            exit_code: None,
            message: Some(error.to_string()),
        },
    }
}

fn issue(analysis: &mut Analysis, root: &Path, code: &'static str, project: &str, message: String) {
    analysis.diagnostics.push(Diagnostic {
        code,
        severity: "error",
        project: project.into(),
        path: root.to_string_lossy().into_owned(),
        message,
    });
    analysis.status = "errors";
}

fn required_not_checked(analysis: &Analysis) -> Vec<&'static str> {
    // These require live infrastructure or application-specific policy and are
    // deliberately outside the compile/static verification promise.
    analysis
        .not_checked
        .iter()
        .copied()
        .filter(|name| {
            !matches!(
                *name,
                "runtime_auth_tenancy_encryption_and_audit"
                    | "retention_and_migration_policy"
                    | "docker_execution_and_shell_commands"
            )
        })
        .collect()
}

fn workspace_typescript(root: &Path, config: &Path) -> Option<PathBuf> {
    config
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .take_while(|directory| directory.starts_with(root))
        .map(|directory| directory.join("node_modules/typescript/bin/tsc"))
        .find(|compiler| compiler.is_file())
}

fn local_sdk_config(directory: &Path) -> Option<PathBuf> {
    let config = directory.join("sdk/typescript/tsconfig.json");
    config.is_file().then_some(config)
}

fn event_sdk_config(directory: &Path) -> Option<PathBuf> {
    (directory.join("event-contract.json").is_file()
        || directory.join("typescript/package.json").is_file())
    .then(|| directory.join("typescript/tsconfig.json"))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn inspect(
    root: &Path,
    modules: &str,
    projects: &[ProjectEntry],
    filter: Option<&str>,
    verify: bool,
    strict: bool,
    analysis: &mut Analysis,
) -> Option<Verification> {
    if !verify && !strict {
        return None;
    }
    let mut report = Verification {
        mode: if verify { "compiler" } else { "read_only" },
        configuration: "workspace defaults; Cargo default features and configured target; declared TypeScript tsconfigs; no feature matrix inferred",
        checks: vec![],
        required_not_checked: vec![],
    };
    // A structurally invalid inventory is not a trustworthy execution plan.
    if verify && !analysis.has_errors() {
        let selected: Vec<_> = projects
            .iter()
            .filter(|p| filter.is_none_or(|name| p.name == name))
            .collect();
        let rust: Vec<_> = selected
            .iter()
            .filter(|p| p.metadata.as_ref().and_then(|m| m.language.as_deref()) == Some("rust"))
            .collect();
        if !rust.is_empty() {
            let mut args = vec!["check".into(), "--all-targets".into()];
            if filter.is_none() {
                args.push("--workspace".into());
            } else {
                for project in &rust {
                    args.extend([
                        "--package".into(),
                        project
                            .metadata
                            .as_ref()
                            .and_then(|m| m.crate_name.clone())
                            .unwrap_or_else(|| project.name.clone()),
                    ]);
                }
            }
            let check = execute(
                root,
                "rust_typechecking",
                filter.unwrap_or("workspace"),
                "cargo",
                &args,
            );
            if check.status == "passed" {
                analysis
                    .not_checked
                    .retain(|name| *name != "rust_macro_expansion_and_typechecking");
            }
            report.checks.push(check);
        } else {
            analysis
                .not_checked
                .retain(|name| *name != "rust_macro_expansion_and_typechecking");
        }
        let mut configs = BTreeSet::new();
        let mut missing_ts = false;
        for project in selected
            .iter()
            .filter(|p| p.metadata.as_ref().and_then(|m| m.language.as_deref()) != Some("rust"))
        {
            if let Some(dir) = workspace_diagnostics::project_dir(root, modules, project) {
                let config = dir.join("tsconfig.json");
                if config.is_file() {
                    configs.insert(config);
                } else {
                    missing_ts = true;
                    report.checks.push(Check {
                        kind: "typescript_typechecking",
                        project: project.name.clone(),
                        command: vec![],
                        status: "not_checked",
                        exit_code: None,
                        message: Some(format!("No declared tsconfig at {}", config.display())),
                    });
                }
            }
        }
        // Generated SDKs are checked directly, without executing app-defined scripts.
        let sdk_config = root
            .join(modules)
            .join("client-sdk/typescript/tsconfig.json");
        if sdk_config.is_file() {
            configs.insert(sdk_config);
        }
        for project in &selected {
            if let Some(directory) = workspace_diagnostics::project_dir(root, modules, project) {
                if let Some(config) = local_sdk_config(&directory) {
                    configs.insert(config);
                }
            }
        }
        let mut event_directories = BTreeSet::from([root.join(modules).join("client-sdk/events")]);
        for project in &selected {
            if let Some(directory) = workspace_diagnostics::project_dir(root, modules, project) {
                event_directories.insert(directory.join("events"));
            }
        }
        for directory in event_directories {
            if let Some(config) = event_sdk_config(&directory) {
                if config.is_file() {
                    configs.insert(config);
                } else {
                    missing_ts = true;
                    report.checks.push(Check {
                        kind: "typescript_typechecking",
                        project: directory.to_string_lossy().into_owned(),
                        command: vec![],
                        status: "not_checked",
                        exit_code: None,
                        message: Some(format!(
                            "Generated worker SDK has no tsconfig at {}",
                            config.display()
                        )),
                    });
                }
            }
        }
        let mut ts_passed = !missing_ts;
        for config in configs {
            let compiler = workspace_typescript(root, &config);
            let Some(compiler) = compiler else {
                ts_passed = false;
                report.checks.push(Check {
                    kind: "typescript_typechecking", project: config.to_string_lossy().into_owned(),
                    command: vec![], status: "not_checked", exit_code: None,
                    message: Some("Install this workspace's declared TypeScript development dependency before verification; global compiler fallback is not used".into()),
                });
                continue;
            };
            let check = execute(
                config.parent().unwrap_or(root),
                "typescript_typechecking",
                &config.to_string_lossy(),
                "node",
                &[
                    compiler.to_string_lossy().into_owned(),
                    "--noEmit".into(),
                    "--project".into(),
                    config.to_string_lossy().into_owned(),
                ],
            );
            ts_passed &= check.status == "passed";
            report.checks.push(check);
        }
        if ts_passed {
            analysis
                .not_checked
                .retain(|name| *name != "typescript_typechecking");
        }
        let has_http_exports = selected.iter().any(|project| {
            matches!(project.r#type, ProjectType::Service | ProjectType::Worker)
                && project.metadata.as_ref().and_then(|m| m.contract_export) != Some(false)
        });
        if let Ok(cli) = std::env::current_exe()
            && has_http_exports
        {
            let mut args: Vec<String> = [
                "sdk", "generate", "--check", "--target", "all", "--from", "all", "--path",
            ]
            .into_iter()
            .map(String::from)
            .collect();
            args.push(root.to_string_lossy().into_owned());
            if let Some(name) = filter {
                args.extend(["--service".into(), name.into()]);
            }
            report.checks.push(execute(
                root,
                "sdk_freshness",
                filter.unwrap_or("workspace"),
                &cli.to_string_lossy(),
                &args,
            ));
            // SDK byte/semantic equality alone does not attest source fingerprints,
            // graph hashes, target/features, or binding/security manifest coverage.
        }
        for project in &rust {
            if matches!(project.r#type, ProjectType::Service | ProjectType::Worker)
                && project.metadata.as_ref().and_then(|m| m.contract_export) != Some(false)
            {
                report
                    .checks
                    .push(provenance::inspect(root, modules, project));
                report
                    .checks
                    .push(provenance::inspect_bindings(root, modules, project));
            }
        }
        if let Some(check) = provenance::inspect_channels(root, modules, projects, filter) {
            report.checks.push(check);
        }
        report.checks.extend(provenance::inspect_local_channels(
            root, modules, projects, filter,
        ));
        let exported: Vec<_> = selected
            .iter()
            .filter(|project| {
                matches!(project.r#type, ProjectType::Service | ProjectType::Worker)
                    && project
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.contract_export)
                        != Some(false)
            })
            .collect();
        let passed = |kind: &str, project: &str| {
            report.checks.iter().any(|check| {
                check.kind == kind && check.project == project && check.status == "passed"
            })
        };
        let all_native = exported.iter().all(|project| {
            project
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.language.as_deref())
                == Some("rust")
        });
        let source_assets = all_native
            && exported
                .iter()
                .all(|project| passed("http_provenance_declared_scope", &project.name))
            && report
                .checks
                .iter()
                .filter(|check| check.kind == "channel_provenance_declared_scope")
                .all(|check| check.status == "passed");
        let compiler_passed = !analysis
            .not_checked
            .contains(&"rust_macro_expansion_and_typechecking")
            && !analysis.not_checked.contains(&"typescript_typechecking");
        if source_assets
            && compiler_passed
            && (!has_http_exports || passed("sdk_freshness", filter.unwrap_or("workspace")))
        {
            analysis
                .not_checked
                .retain(|name| *name != "generated_artifact_freshness_and_digests");
            if exported
                .iter()
                .all(|project| passed("managed_http_syntactic_assembly", &project.name))
            {
                analysis
                    .not_checked
                    .retain(|name| *name != "canonical_contract_semantics");
            }
        }
        for check in &report.checks {
            if check.status == "failed" {
                issue(
                    analysis,
                    root,
                    "FLR-VERIFY-001",
                    &check.project,
                    format!(
                        "{} failed (exit {:?}); compiler diagnostics are on stderr",
                        check.kind, check.exit_code
                    ),
                );
            }
        }
    }
    report.required_not_checked = required_not_checked(analysis);
    if strict && !report.required_not_checked.is_empty() {
        issue(
            analysis,
            root,
            "FLR-ANALYSIS-004",
            filter.unwrap_or("workspace"),
            format!(
                "Required verification coverage remains incomplete: {}",
                report.required_not_checked.join(", ")
            ),
        );
    }
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_project_local_sdk_is_in_typecheck_inventory() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("src/notes");
        let config = directory.join("sdk/typescript/tsconfig.json");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(&config, "{}").unwrap();
        assert_eq!(local_sdk_config(&directory), Some(config));
        assert_eq!(local_sdk_config(root.path()), None);
    }

    #[test]
    fn worker_sdk_coverage_does_not_disappear_when_its_config_is_missing() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("events");
        assert_eq!(event_sdk_config(&directory), None);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("event-contract.json"), "{}").unwrap();
        assert_eq!(
            event_sdk_config(&directory),
            Some(directory.join("typescript/tsconfig.json"))
        );
        std::fs::remove_file(directory.join("event-contract.json")).unwrap();
        std::fs::create_dir_all(directory.join("typescript")).unwrap();
        std::fs::write(directory.join("typescript/package.json"), "{}").unwrap();
        assert_eq!(
            event_sdk_config(&directory),
            Some(directory.join("typescript/tsconfig.json"))
        );
    }

    #[test]
    fn typescript_resolution_stays_in_workspace_and_prefers_project_install() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("app");
        let project = root.join("src/notes");
        let config = project.join("tsconfig.json");
        let installed = |directory: &Path| {
            let compiler = directory.join("node_modules/typescript/bin/tsc");
            std::fs::create_dir_all(compiler.parent().unwrap()).unwrap();
            std::fs::write(&compiler, "fixture").unwrap();
            compiler
        };
        installed(temporary.path());
        assert_eq!(workspace_typescript(&root, &config), None);
        let workspace_compiler = installed(&root);
        assert_eq!(
            workspace_typescript(&root, &config),
            Some(workspace_compiler)
        );
        let project_compiler = installed(&project);
        assert_eq!(workspace_typescript(&root, &config), Some(project_compiler));
    }

    #[test]
    fn strict_read_only_reports_unknowns_without_executing_anything() {
        let root = tempfile::tempdir().unwrap();
        let mut analysis =
            workspace_diagnostics::inspect(root.path(), "src", &[], Some("rust"), None);
        let report = inspect(root.path(), "src", &[], None, false, true, &mut analysis).unwrap();
        assert!(report.checks.is_empty());
        assert!(analysis.has_errors());
        assert!(
            report
                .required_not_checked
                .contains(&"canonical_contract_semantics")
        );
        assert!(
            !report
                .required_not_checked
                .contains(&"runtime_auth_tenancy_encryption_and_audit")
        );
        assert!(serde_json::to_value(report).is_ok());
    }

    #[test]
    fn ordinary_inspection_never_enters_verification() {
        let root = tempfile::tempdir().unwrap();
        let mut analysis = workspace_diagnostics::inspect(root.path(), "src", &[], None, None);
        assert!(inspect(root.path(), "src", &[], None, false, false, &mut analysis).is_none());
    }

    #[test]
    fn unavailable_tool_is_unknown_and_nonzero_exit_is_failure() {
        let root = tempfile::tempdir().unwrap();
        let unavailable = execute(
            root.path(),
            "fixture",
            "fixture",
            "/no-such-forklaunch-tool",
            &[],
        );
        assert_eq!(unavailable.status, "not_checked");
        assert!(unavailable.message.is_some());
        #[cfg(unix)]
        {
            let failed = execute(
                root.path(),
                "fixture",
                "fixture",
                "/bin/sh",
                &["-c".into(), "exit 7".into()],
            );
            assert_eq!(failed.status, "failed");
            assert_eq!(failed.exit_code, Some(7));
        }
    }
}
