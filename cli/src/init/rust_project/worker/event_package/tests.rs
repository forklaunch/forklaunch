use super::*;
use crate::init::rust_project::{RustProjectKind, native_layout, worker};
use serde_json::json;
use std::fs;

fn worker_templates(
    root: &Path,
    directory: &Path,
    backend: worker::Backend,
    apalis: bool,
    producer: bool,
) -> Vec<RenderedTemplate> {
    let files = if apalis {
        worker::apalis_templates(root, directory, "insights", "Insights worker").unwrap()
    } else {
        native_layout::flatten(
            directory,
            "insights",
            RustProjectKind::Worker,
            worker::templates(root, directory, "insights", "Insights worker", backend).unwrap(),
        )
    };
    if producer {
        worker::service_templates(
            directory,
            "insights",
            "Insights worker",
            files,
            apalis,
            backend,
        )
        .unwrap()
    } else {
        worker::consumer_contracts(directory, "insights", files).unwrap()
    }
}

#[test]
fn worker_event_packages_exist_before_install_for_each_native_profile() {
    let root = tempfile::tempdir().unwrap();
    let compiled = root.path().join("compiler-output");
    let schema = json!({
        "type":"object", "additionalProperties":false,
        "required":["resourceId", "revision"],
        "properties":{"resourceId":{"type":"string"}, "revision":{"type":"integer"}}
    });
    // The actual public compiler must produce precisely the dependencies declared
    // by init, so a package-manager install never needs a hidden second pass.
    forklaunch_core::build::worker_events("insights", &schema, &schema, "", &compiled).unwrap();
    for modules in ["src", "src/modules", "domains"] {
        let directory = root.path().join(modules).join("insights");
        for (backend, apalis) in [
            (worker::Backend::Postgres, false),
            (worker::Backend::Redis, false),
            (worker::Backend::Kafka, false),
            (worker::Backend::Postgres, true),
        ] {
            for producer in [false, true] {
                let files = worker_templates(root.path(), &directory, backend, apalis, producer);
                let event_files = files
                    .iter()
                    .filter(|file| file.path.starts_with(directory.join("events")))
                    .collect::<Vec<_>>();
                assert_eq!(
                    event_files.len(),
                    2,
                    "only dependency metadata precedes compilation"
                );
                for filename in ["package.json", "tsconfig.json"] {
                    let file = event_files
                        .iter()
                        .find(|file| {
                            file.path == directory.join("events/typescript").join(filename)
                        })
                        .unwrap();
                    assert_eq!(
                        file.content,
                        fs::read_to_string(compiled.join("typescript").join(filename)).unwrap()
                    );
                }
                assert!(!directory.exists(), "dry-run planning must not write");
            }
        }
    }
}

#[test]
fn worker_event_package_reinit_preserves_authored_metadata_and_fills_missing_config() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("domains/insights");
    let destination = directory.join("events/typescript");
    fs::create_dir_all(&destination).unwrap();
    let package = r#"{"name":"custom-events","dependencies":{"ajv":"8.20.0","custom":"1"},"scripts":{"build":"custom-build"}}"#;
    fs::write(destination.join("package.json"), package).unwrap();
    let files = worker_templates(root.path(), &directory, worker::Backend::Redis, false, true);
    assert!(
        !files
            .iter()
            .any(|file| file.path == destination.join("package.json"))
    );
    let config = files
        .iter()
        .find(|file| file.path == destination.join("tsconfig.json"))
        .unwrap();
    assert_eq!(config.content, EVENT_TSCONFIG);
    assert_eq!(
        fs::read_to_string(destination.join("package.json")).unwrap(),
        package
    );
    assert!(!destination.join("tsconfig.json").exists());
    let authored = r#"{"compilerOptions":{"strict":true},"exclude":["scratch"]}"#;
    fs::write(destination.join("tsconfig.json"), authored).unwrap();
    assert!(templates(&directory, "insights").unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(destination.join("package.json")).unwrap(),
        package
    );
    assert_eq!(
        fs::read_to_string(destination.join("tsconfig.json")).unwrap(),
        authored
    );
    assert!(templates(&directory, "../invalid").is_err());
}
