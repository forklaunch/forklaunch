//! Declare worker event dependencies before the first workspace installation.
use super::{Path, RenderedTemplate, Result};

// The framework renders the same package for services; published framework
// releases before 0.2.0 do not export it, so the CLI keeps this copy and a test
// holds the two together.
const EVENT_TSCONFIG: &str = r#"{"compilerOptions":{"target":"ES2022","module":"NodeNext","moduleResolution":"NodeNext","lib":["ES2022","DOM"],"types":["node"],"strict":true,"noEmit":true},"include":["*.ts","*.mts"]}"#;

fn event_package(service: &str) -> Result<String> {
    anyhow::ensure!(
        !service.is_empty()
            && service.len() <= 64
            && service.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
        "Invalid event SDK service name"
    );
    Ok(serde_json::to_string_pretty(&serde_json::json!({"name":format!("@forklaunch-generated/{service}-events"),"version":"0.0.0","type":"module","private":true,"exports":{".":"./events.ts"},"scripts":{"build":"tsc --noEmit"},"dependencies":{"ajv":"8.20.0","ajv-formats":"3.0.1","@types/node":"22.10.0"},"devDependencies":{"typescript":"5.9.3"}}))?)
}

pub(super) fn templates(directory: &Path, name: &str) -> Result<Vec<RenderedTemplate>> {
    let mut templates = Vec::new();
    for (file, content) in [
        ("package.json", event_package(name)?),
        ("tsconfig.json", EVENT_TSCONFIG.to_owned()),
    ] {
        let path = directory.join("events/typescript").join(file);
        // Reinitialization may fill missing files, but must not reset application
        // dependency pins or compiler settings. Planning itself never writes.
        match std::fs::symlink_metadata(&path) {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        templates.push(RenderedTemplate {
            path,
            content,
            context: None,
        });
    }
    Ok(templates)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod framework_parity_tests {
    #[test]
    fn event_package_matches_the_framework_renderer() {
        assert_eq!(super::EVENT_TSCONFIG, forklaunch_core::build::service_package::EVENT_TSCONFIG);
        assert_eq!(
            super::event_package("jobs").unwrap(),
            forklaunch_core::build::service_package::event_package("jobs").unwrap()
        );
        assert!(super::event_package("bad name").is_err());
    }
}
