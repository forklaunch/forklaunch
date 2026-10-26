//! Bounded, manifest-directed consumption of Rust storage classifications.
//! Parsing declarations never proves runtime encryption, tenant policy or lifecycle.
use crate::{
    CliCommand,
    core::{
        command::command,
        manifest::{ProjectEntry, ProjectType},
        validate::require_manifest,
    },
};
use anyhow::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Component, Path},
};
const MAX_BYTES: u64 = 1_048_576;
/// Build output written by each crate's build script, beside its private contract host.
pub(crate) const ARTIFACT: &str = ".forklaunch/compliance.json";
/// Location used before the artifact moved under the crate's generated directory.
const LEGACY_ARTIFACT: &str = "compliance.json";

#[derive(Debug)]
pub(crate) struct ValidateArtifactsCommand;
impl CliCommand for ValidateArtifactsCommand {
    fn command(&self) -> Command {
        command(
            "validate-artifacts",
            "Validate generated Rust declarations; does not certify runtime compliance",
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("Application path"),
        )
        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let (root, manifest) = require_manifest(matches)?;
        let report = scan(&root, &manifest.modules_path, &manifest.projects);
        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            for project in &report.projects {
                println!(
                    "{}: {} ({})",
                    project.project, project.status, project.artifact
                );
            }
            println!(
                "Declarations only: encryption, tenant isolation, lifecycle and source freshness are unverified."
            );
        }
        anyhow::ensure!(
            report
                .projects
                .iter()
                .all(|p| p.status == "valid-declarations"),
            "Rust classification artifacts are missing or invalid; build the affected crate and review the diagnostics"
        );
        Ok(())
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RustDeclarations {
    pub(crate) projects: Vec<ProjectDeclarations>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectDeclarations {
    pub(crate) project: String,
    pub(crate) artifact: String,
    pub(crate) status: &'static str,
    pub(crate) diagnostic: Option<&'static str>,
    pub(crate) source_freshness: &'static str,
    pub(crate) encryption_verification: &'static str,
    pub(crate) tenancy_verification: &'static str,
    pub(crate) lifecycle_verification: &'static str,
    pub(crate) declarations: Option<Artifact>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Artifact {
    schema_version: u32,
    coverage: String,
    runtime_enforcement: String,
    lifecycle_coverage: String,
    pub(crate) entities: Vec<Entity>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entity {
    entity: String,
    storage: String,
    kind: String,
    /// Absent in artifacts written before protected-field bindings were exported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    binding: Option<Binding>,
    pub(crate) fields: Vec<Field>,
}
/// Declared purpose bindings from `ProtectedField` models. A binding names the
/// members a typed model protects; it is not proof that values were encrypted.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Binding {
    DeclarationsOnly,
    ProtectedModel {
        #[serde(rename = "protectedMembers")]
        protected_members: Vec<ProtectedMember>,
    },
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProtectedMember {
    member: String,
    purpose: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Field {
    member: String,
    column: String,
    classification: String,
    nullable: bool,
}
fn valid_name(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
fn validate(artifact: &Artifact) -> Result<(), &'static str> {
    if artifact.schema_version != 1 {
        return Err("Unsupported classification artifact schema version");
    }
    if artifact.coverage != "declared-storage-members-only"
        || artifact.runtime_enforcement != "requires-independent-verification"
        || artifact.lifecycle_coverage != "unknown"
    {
        return Err(
            "Artifact must explicitly preserve declared-only and unverified-runtime coverage",
        );
    }
    if artifact.entities.len() > 256 {
        return Err("Classification artifact exceeds entity limit");
    }
    let mut entities = BTreeSet::new();
    for entity in &artifact.entities {
        if ![&entity.entity, &entity.storage, &entity.kind]
            .iter()
            .all(|v| valid_name(v))
            || !entities.insert((&entity.entity, &entity.storage))
        {
            return Err("Invalid or duplicate storage entity declaration");
        }
        if entity.fields.is_empty() || entity.fields.len() > 1024 {
            return Err("Invalid classification field count");
        }
        let mut members = BTreeSet::new();
        let mut columns = BTreeSet::new();
        for field in &entity.fields {
            if !valid_name(&field.member)
                || !valid_name(&field.column)
                || !members.insert(&field.member)
                || !columns.insert(&field.column)
            {
                return Err("Invalid or duplicate member/storage mapping");
            }
            if !matches!(
                field.classification.as_str(),
                "none" | "pii" | "phi" | "pci"
            ) {
                return Err("Unsupported member classification");
            }
        }
        if let Some(Binding::ProtectedModel { protected_members }) = &entity.binding {
            let mut bound = BTreeSet::new();
            if protected_members.is_empty()
                || !protected_members.iter().all(|protected| {
                    valid_name(&protected.purpose)
                        && members.contains(&protected.member)
                        && bound.insert(&protected.member)
                })
            {
                return Err("Protected bindings must name distinct declared members");
            }
        }
    }
    Ok(())
}
fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

pub(crate) fn scan(root: &Path, modules: &str, projects: &[ProjectEntry]) -> RustDeclarations {
    let mut output = RustDeclarations::default();
    for project in projects {
        if !matches!(project.r#type, ProjectType::Service | ProjectType::Worker) {
            continue;
        }
        let metadata = project.metadata.as_ref();
        let explicit = metadata.and_then(|m| m.language.as_deref());
        let crate_path = metadata
            .and_then(|m| m.crate_path.clone())
            .unwrap_or_else(|| {
                Path::new(modules)
                    .join(&project.name)
                    .to_string_lossy()
                    .into_owned()
            });
        // Old manifests may omit language; a project Cargo manifest is evidence.
        if explicit != Some("rust")
            && (explicit.is_some() || !root.join(&crate_path).join("Cargo.toml").is_file())
        {
            continue;
        }
        let mut row = ProjectDeclarations {
            project: project.name.clone(),
            artifact: format!("{crate_path}/{ARTIFACT}"),
            status: "invalid",
            diagnostic: None,
            source_freshness: "unverified",
            encryption_verification: "unknown",
            tenancy_verification: "unknown",
            lifecycle_verification: "unknown",
            declarations: None,
        };
        let result = (|| -> Result<Artifact, &'static str> {
            if !relative(Path::new(&crate_path)) {
                return Err("Rust crate path must stay within the application");
            }
            let root = root
                .canonicalize()
                .map_err(|_| "Application root cannot be read")?;
            let directory = root
                .join(&crate_path)
                .canonicalize()
                .map_err(|_| "Rust crate directory is missing")?;
            if !directory.starts_with(&root) {
                return Err("Rust crate resolves outside the application");
            }
            let file = [ARTIFACT, LEGACY_ARTIFACT]
                .into_iter()
                .find(|artifact| directory.join(artifact).exists())
                .map(|artifact| {
                    row.artifact = format!("{crate_path}/{artifact}");
                    directory.join(artifact)
                });
            let Some(file) = file else {
                row.status = "missing";
                return Err(
                    "Missing .forklaunch/compliance.json; compile this crate to generate its declarations",
                );
            };
            row.status = "invalid";
            let file = file
                .canonicalize()
                .map_err(|_| "Classification artifact cannot be read")?;
            if !file.starts_with(&directory) {
                return Err("Classification artifact resolves outside its crate");
            }
            let metadata =
                fs::metadata(&file).map_err(|_| "Classification artifact cannot be read")?;
            if !metadata.is_file() || metadata.len() > MAX_BYTES {
                return Err("Classification artifact must be a file no larger than one MiB");
            }
            let mut bytes = Vec::new();
            fs::File::open(file)
                .map_err(|_| "Classification artifact cannot be read")?
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Classification artifact cannot be read")?;
            if bytes.len() > MAX_BYTES as usize {
                return Err("Classification artifact exceeds size limit");
            }
            let artifact: Artifact = serde_json::from_slice(&bytes)
                .map_err(|_| "Malformed classification artifact JSON/schema")?;
            validate(&artifact)?;
            Ok(artifact)
        })();
        match result {
            Ok(artifact) => {
                row.status = "valid-declarations";
                row.declarations = Some(artifact);
            }
            Err(message) => row.diagnostic = Some(message),
        }
        output.projects.push(row);
    }
    output.projects.sort_by(|a, b| a.project.cmp(&b.project));
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::ProjectMetadata;
    use serde_json::json;
    fn project(name: &str, path: &str) -> ProjectEntry {
        ProjectEntry {
            name: name.into(),
            r#type: ProjectType::Service,
            description: String::new(),
            variant: None,
            resources: None,
            routers: None,
            serves: None,
            metadata: Some(ProjectMetadata {
                language: Some("rust".into()),
                crate_path: Some(path.into()),
                ..Default::default()
            }),
        }
    }
    fn artifact() -> serde_json::Value {
        json!({"schemaVersion":1,"coverage":"declared-storage-members-only","runtimeEnforcement":"requires-independent-verification","lifecycleCoverage":"unknown","entities":[{"entity":"Contact","storage":"contacts","kind":"postgres","fields":[{"member":"email","column":"email_ciphertext","classification":"pii","nullable":true}]}]})
    }
    fn write(root: &Path, value: &serde_json::Value) {
        fs::create_dir_all(root.join("src/notes")).unwrap();
        fs::write(
            root.join("src/notes/Cargo.toml"),
            "[package]\nname='notes'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("src/notes/.forklaunch")).unwrap();
        fs::write(root.join("src/notes").join(ARTIFACT), value.to_string()).unwrap();
    }
    #[test]
    fn manifest_crate_path_is_authoritative_and_declarations_never_become_encryption_proof() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &artifact());
        let mut ts = project("ts", "src/typescript");
        ts.metadata.as_mut().unwrap().language = Some("typescript".into());
        let report = scan(
            dir.path(),
            "wrong/modules",
            &[project("notes", "src/notes"), ts],
        );
        assert_eq!(report.projects.len(), 1);
        let row = &report.projects[0];
        assert_eq!(row.status, "valid-declarations");
        assert_eq!(row.encryption_verification, "unknown");
        assert_eq!(row.tenancy_verification, "unknown");
        assert_eq!(row.lifecycle_verification, "unknown");
        assert_eq!(row.source_freshness, "unverified");
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(
            json["projects"][0]["declarations"]["entities"][0]["fields"][0]["column"],
            "email_ciphertext"
        );
        assert!(!json.to_string().contains("\"encrypted\":true"));
    }
    #[test]
    fn missing_invalid_and_overclaiming_artifacts_remain_visible() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/notes")).unwrap();
        assert_eq!(
            scan(dir.path(), "src", &[project("notes", "src/notes")]).projects[0].status,
            "missing"
        );
        let mut bad = artifact();
        bad["runtimeEnforcement"] = json!("certified");
        write(dir.path(), &bad);
        assert_eq!(
            scan(dir.path(), "src", &[project("notes", "src/notes")]).projects[0].status,
            "invalid"
        );
        let mut bad = artifact();
        bad["entities"][0]["fields"][0]["classification"] = json!("personal");
        write(dir.path(), &bad);
        assert_eq!(
            scan(dir.path(), "src", &[project("notes", "src/notes")]).projects[0].status,
            "invalid"
        );
        let mut bad = artifact();
        bad["entities"][0]["fields"]
            .as_array_mut()
            .unwrap()
            .push(artifact()["entities"][0]["fields"][0].clone());
        write(dir.path(), &bad);
        assert_eq!(
            scan(dir.path(), "src", &[project("notes", "src/notes")]).projects[0].status,
            "invalid"
        );
        fs::write(
            dir.path().join("src/notes").join(ARTIFACT),
            "private invalid source content",
        )
        .unwrap();
        let report = scan(dir.path(), "src", &[project("notes", "src/notes")]);
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("private invalid source content")
        );
    }
    #[test]
    fn published_framework_bindings_validate_and_must_name_declared_members() {
        let dir = tempfile::tempdir().unwrap();
        let mut value = artifact();
        value["entities"][0]["binding"] = json!({"kind":"protected-model","protectedMembers":[{"member":"email","purpose":"contact-email-v1"}]});
        write(dir.path(), &value);
        let report = scan(dir.path(), "src", &[project("notes", "src/notes")]);
        assert_eq!(report.projects[0].status, "valid-declarations");
        assert_eq!(report.projects[0].encryption_verification, "unknown", "A binding is a declaration, not encryption proof");
        value["entities"][0]["binding"] = json!({"kind":"declarations-only"});
        write(dir.path(), &value);
        assert_eq!(scan(dir.path(), "src", &[project("notes", "src/notes")]).projects[0].status, "valid-declarations");
        for binding in [
            json!({"kind":"protected-model","protectedMembers":[{"member":"phone","purpose":"p"}]}),
            json!({"kind":"protected-model","protectedMembers":[]}),
            json!({"kind":"certified-encrypted"}),
        ] {
            value["entities"][0]["binding"] = binding;
            write(dir.path(), &value);
            assert_eq!(scan(dir.path(), "src", &[project("notes", "src/notes")]).projects[0].status, "invalid");
        }
    }
    #[test]
    fn crate_local_build_output_takes_precedence_over_the_legacy_crate_root_artifact() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &artifact());
        let legacy = dir.path().join("src/notes").join(LEGACY_ARTIFACT);
        fs::write(&legacy, "stale legacy output").unwrap();
        let row = &scan(dir.path(), "src", &[project("notes", "src/notes")]).projects[0];
        assert_eq!((row.status, row.artifact.as_str()), ("valid-declarations", "src/notes/.forklaunch/compliance.json"));
        fs::remove_file(dir.path().join("src/notes").join(ARTIFACT)).unwrap();
        fs::write(&legacy, artifact().to_string()).unwrap();
        let row = &scan(dir.path(), "src", &[project("notes", "src/notes")]).projects[0];
        assert_eq!((row.status, row.artifact.as_str()), ("valid-declarations", "src/notes/compliance.json"),
            "Applications built before the move remain discoverable");
    }
    #[test]
    fn parser_rejects_oversize_versions_and_path_escape() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &artifact());
        fs::write(
            dir.path().join("src/notes").join(ARTIFACT),
            vec![b' '; MAX_BYTES as usize + 1],
        )
        .unwrap();
        assert_eq!(
            scan(dir.path(), "src", &[project("notes", "src/notes")]).projects[0].status,
            "invalid"
        );
        for path in ["../outside", "/private/tmp"] {
            assert_ne!(
                scan(dir.path(), "src", &[project("notes", path)]).projects[0].status,
                "valid-declarations"
            );
        }
        let mut bad = artifact();
        bad["schemaVersion"] = json!(2);
        write(dir.path(), &bad);
        assert_eq!(
            scan(dir.path(), "src", &[project("notes", "src/notes")]).projects[0].status,
            "invalid"
        );
    }
    #[cfg(unix)]
    #[test]
    fn symlink_cannot_read_declarations_outside_the_crate() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        write(other.path(), &artifact());
        fs::create_dir_all(dir.path().join("src/notes/.forklaunch")).unwrap();
        std::os::unix::fs::symlink(
            other.path().join("src/notes").join(ARTIFACT),
            dir.path().join("src/notes").join(ARTIFACT),
        )
        .unwrap();
        let report = scan(dir.path(), "src", &[project("notes", "src/notes")]);
        assert_eq!(report.projects[0].status, "invalid");
        assert!(report.projects[0].declarations.is_none());
    }
}
