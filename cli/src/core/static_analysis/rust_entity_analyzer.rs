//! Data classifications declared by a Rust service, read from its source.
//!
//! A ForkLaunch Rust crate records classifications on the stored type itself:
//!
//! ```ignore
//! #[derive(ComplianceMetadata)]
//! #[compliance(entity = "Note", persisted)]
//! pub struct StoredNote {
//!     #[compliance(level = "none")]
//!     pub id: Uuid,
//!     #[compliance(level = "pii", purpose = "note-title-v1")]
//!     pub title_ciphertext: ProtectedField,
//! }
//! ```
//!
//! The derive requires a level on every field. `persisted` makes the struct a protected
//! model: each classified member must then be a `ProtectedField` (ciphertext only) with an
//! explicit purpose, or the crate does not compile. Without `persisted` the struct only
//! declares classifications and nothing ties its members to ciphertext.
//!
//! The crate's build script writes the same declarations to `.forklaunch/compliance.json`.
//! That file is build output and can lag the source, so the source is read first and the
//! artifact only when the source declares nothing (a hand-written `ComplianceMetadata`
//! implementation, for example). Nothing is compiled or run to read either.
use std::{collections::HashMap, fs, path::Path};

use syn::{
    Attribute, ItemStruct,
    visit::{self, Visit},
};

use super::rust_route_analyzer::{has_cfg_test, read_rust_sources};
use crate::core::ast::infrastructure::compliance::EntityComplianceInfo;

/// Where a crate's build script writes its declarations, newest location first.
const ARTIFACTS: &[&str] = &[".forklaunch/compliance.json", "compliance.json"];
const MAX_ARTIFACT_BYTES: u64 = 1_048_576;

/// One classified storage type of a Rust service.
#[derive(Debug, Clone)]
pub(crate) struct RustEntity {
    /// The same shape the TypeScript entity scan returns. Rust entities declare no
    /// retention policy, so `retention` is always `None`.
    pub(crate) info: EntityComplianceInfo,
    /// Members classified `pii`, `phi` or `pci`, in declaration order.
    pub(crate) classified: Vec<String>,
    /// Whether the classified members are bound to protected (ciphertext-only) storage:
    /// `#[compliance(persisted)]` in source, a `protected-model` binding in the artifact.
    pub(crate) protected: bool,
    /// Source file (or artifact) relative to the project, and the line of the declaration.
    pub(crate) file: String,
    pub(crate) line: usize,
}

fn derives_compliance_metadata(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        if !attribute.path().is_ident("derive") {
            return false;
        }
        let mut found = false;
        let _ = attribute.parse_nested_meta(|meta| {
            if meta
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "ComplianceMetadata")
            {
                found = true;
            }
            Ok(())
        });
        found
    })
}

/// The options of every `#[compliance(..)]` on an item: `(name, string value if any)`.
fn compliance_options(attributes: &[Attribute]) -> Vec<(String, Option<String>)> {
    let mut options = Vec::new();
    for attribute in attributes {
        if !attribute.path().is_ident("compliance") {
            continue;
        }
        let _ = attribute.parse_nested_meta(|meta| {
            let Some(name) = meta.path.get_ident().map(ToString::to_string) else {
                return Ok(());
            };
            let value = match meta.value() {
                Ok(value) => value.parse::<syn::LitStr>().ok().map(|text| text.value()),
                Err(_) => None,
            };
            options.push((name, value));
            Ok(())
        });
    }
    options
}

struct Reader<'a> {
    file: &'a str,
    entities: Vec<RustEntity>,
}

impl<'ast> Visit<'ast> for Reader<'_> {
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if !has_cfg_test(&module.attrs) {
            visit::visit_item_mod(self, module);
        }
    }

    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        if !derives_compliance_metadata(&item.attrs) {
            return;
        }
        let options = compliance_options(&item.attrs);
        let entity_name = options
            .iter()
            .find(|(name, _)| name == "entity")
            .and_then(|(_, value)| value.clone())
            .unwrap_or_else(|| item.ident.to_string().trim_start_matches("r#").to_string());
        let protected = options.iter().any(|(name, _)| name == "persisted");
        let mut field_classifications = HashMap::new();
        let mut classified = Vec::new();
        for field in &item.fields {
            let Some(member) = &field.ident else {
                continue;
            };
            let member = member.to_string().trim_start_matches("r#").to_string();
            let level = compliance_options(&field.attrs)
                .into_iter()
                .find(|(name, _)| name == "level")
                .and_then(|(_, value)| value);
            // A field with no level does not compile; it is left out rather than guessed.
            let Some(level) = level else {
                continue;
            };
            if level != "none" {
                classified.push(member.clone());
            }
            field_classifications.insert(member, level);
        }
        self.entities.push(RustEntity {
            info: EntityComplianceInfo {
                entity_name,
                field_classifications,
                retention: None,
            },
            classified,
            protected,
            file: self.file.to_string(),
            line: item.ident.span().start().line,
        });
    }
}

/// The entities declared in `(relative path, source)` pairs.
#[cfg(test)]
pub(crate) fn rust_entities_in_sources(files: &[(&str, &str)]) -> Vec<RustEntity> {
    entities_in(&super::rust_route_analyzer::parse_sources(
        files
            .iter()
            .map(|(rel, text)| (rel.to_string(), text.to_string()))
            .collect(),
    ))
}

fn entities_in(sources: &super::rust_route_analyzer::RustSources) -> Vec<RustEntity> {
    let mut entities = Vec::new();
    for file in &sources.files {
        let mut reader = Reader {
            file: &file.rel,
            entities: Vec::new(),
        };
        reader.visit_file(&file.ast);
        entities.extend(reader.entities);
    }
    entities
}

/// The declarations a crate's build script exported, when the artifact is present and
/// well-formed. Anything unexpected reads as no declarations.
fn entities_in_artifact(project_path: &Path) -> Vec<RustEntity> {
    let Some((name, text)) = ARTIFACTS.iter().find_map(|name| {
        let path = project_path.join(name);
        let small = fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_ARTIFACT_BYTES);
        small
            .then(|| fs::read_to_string(&path).ok())
            .flatten()
            .map(|text| (*name, text))
    }) else {
        return Vec::new();
    };
    let Ok(artifact) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut entities = Vec::new();
    for entity in artifact["entities"].as_array().into_iter().flatten() {
        let Some(entity_name) = entity["entity"].as_str() else {
            continue;
        };
        let mut field_classifications = HashMap::new();
        let mut classified = Vec::new();
        for field in entity["fields"].as_array().into_iter().flatten() {
            let (Some(member), Some(level)) =
                (field["member"].as_str(), field["classification"].as_str())
            else {
                continue;
            };
            if level != "none" {
                classified.push(member.to_string());
            }
            field_classifications.insert(member.to_string(), level.to_string());
        }
        entities.push(RustEntity {
            info: EntityComplianceInfo {
                entity_name: entity_name.to_string(),
                field_classifications,
                retention: None,
            },
            classified,
            protected: entity["binding"]["kind"].as_str() == Some("protected-model"),
            file: name.to_string(),
            line: 1,
        });
    }
    entities
}

/// Every classified storage type a Rust project declares.
pub(crate) fn scan_rust_entity_compliance(project_path: &Path) -> Vec<RustEntity> {
    let entities = entities_in(&read_rust_sources(project_path));
    if entities.is_empty() {
        entities_in_artifact(project_path)
    } else {
        entities
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTITY: &str = r#"
use forklaunch_core::persistence::{compliance::ComplianceMetadata, protected::ProtectedField};

#[derive(Debug, ComplianceMetadata)]
#[compliance(entity = "Note", persisted)]
pub struct StoredNote {
    #[compliance(level = "none")]
    pub id: Uuid,
    #[compliance(level = "none")]
    pub tenant_id: String,
    #[compliance(level = "pii", purpose = "note-title-v1")]
    pub title_ciphertext: ProtectedField,
    #[compliance(level = "phi", purpose = "note-body-v1", nullable, name = "body")]
    pub body_ciphertext: Option<ProtectedField>,
}

#[derive(forklaunch_core::persistence::compliance::ComplianceMetadata, Clone)]
pub struct Contact {
    #[compliance(level = "none")]
    id: String,
    #[compliance(level = "pii", nullable, name = "email_address")]
    email: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NotAnEntity { pub email: String }

#[cfg(test)]
mod tests {
    #[derive(ComplianceMetadata)]
    struct Fixture { #[compliance(level = "pci")] card: String }
}
"#;

    #[test]
    fn reads_classifications_from_the_derive() {
        let entities = rust_entities_in_sources(&[("persistence/entities/stored_note.rs", ENTITY)]);
        assert_eq!(
            entities.len(),
            2,
            "plain structs and test fixtures are not entities"
        );
        let note = &entities[0];
        assert_eq!(note.info.entity_name, "Note");
        assert!(note.protected);
        assert_eq!(note.classified, vec!["title_ciphertext", "body_ciphertext"]);
        assert_eq!(note.info.field_classifications.len(), 4);
        assert_eq!(note.info.field_classifications["id"], "none");
        assert_eq!(note.info.field_classifications["title_ciphertext"], "pii");
        assert_eq!(note.info.field_classifications["body_ciphertext"], "phi");
        assert!(note.info.retention.is_none());
        assert_eq!(
            (note.file.as_str(), note.line),
            ("persistence/entities/stored_note.rs", 6)
        );

        let contact = &entities[1];
        assert_eq!(
            contact.info.entity_name, "Contact",
            "defaults to the struct name"
        );
        assert!(
            !contact.protected,
            "declarations only: no persisted binding"
        );
        assert_eq!(contact.classified, vec!["email"]);
    }

    #[test]
    fn falls_back_to_the_build_artifact_only_when_the_source_declares_nothing() {
        let dir = std::env::temp_dir().join("fl-rust-entity-artifact");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".forklaunch")).unwrap();
        fs::write(dir.join("lib.rs"), "pub mod persistence;\n").unwrap();
        fs::write(
            dir.join(".forklaunch/compliance.json"),
            r#"{"schemaVersion":1,"entities":[
                {"entity":"Record","storage":"records","kind":"postgres-table",
                 "binding":{"kind":"protected-model","protectedMembers":[{"member":"name_ciphertext","purpose":"p"}]},
                 "fields":[{"member":"id","column":"id","classification":"none","nullable":false},
                           {"member":"name_ciphertext","column":"name_ciphertext","classification":"pii","nullable":false}]},
                {"entity":"Audit","storage":"audit","kind":"postgres-table","binding":{"kind":"declarations-only"},
                 "fields":[{"member":"actor","column":"actor","classification":"pii","nullable":false}]}]}"#,
        )
        .unwrap();
        let from_artifact = scan_rust_entity_compliance(&dir);
        assert_eq!(from_artifact.len(), 2);
        assert_eq!(from_artifact[0].info.entity_name, "Record");
        assert!(from_artifact[0].protected);
        assert_eq!(from_artifact[0].classified, vec!["name_ciphertext"]);
        assert!(!from_artifact[1].protected);
        assert_eq!(from_artifact[0].file, ".forklaunch/compliance.json");

        // Once the source declares entities, the (possibly stale) artifact is not consulted.
        fs::write(dir.join("entity.rs"), ENTITY).unwrap();
        let from_source = scan_rust_entity_compliance(&dir);
        assert_eq!(from_source.len(), 2);
        assert_eq!(from_source[0].info.entity_name, "Note");
        assert_eq!(from_source[0].file, "entity.rs");

        fs::write(dir.join(".forklaunch/compliance.json"), "{ not json").unwrap();
        fs::remove_file(dir.join("entity.rs")).unwrap();
        assert!(scan_rust_entity_compliance(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
