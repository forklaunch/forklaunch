//! Explicit native mapper selection. Storage authorization remains repository-owned.
use super::*;

pub(super) fn templates(
    dir: &Path,
    enabled: bool,
    mut files: Vec<RenderedTemplate>,
) -> Result<Vec<RenderedTemplate>> {
    let health = files
        .iter()
        .find(|file| file.path == dir.join("domain/services/health.rs"));
    let postgres = health.is_some_and(|file| file.content.contains("PostgresRepository"));
    let enabled = enabled && health.is_some();
    if !enabled {
        files.retain(|file| !file.path.starts_with(dir.join("domain/mappers")));
        for file in &mut files {
            if file.path == dir.join("domain/mod.rs") {
                file.content = file.content.replace("pub mod mappers;\n", "");
            }
        }
        return Ok(files);
    }
    for file in &mut files {
        let Ok(relative) = file.path.strip_prefix(dir) else {
            continue;
        };
        let relative = relative.to_string_lossy();
        match relative.as_ref() {
            "Cargo.toml" => {
                let mut cargo: toml_edit::DocumentMut = file.content.parse()?;
                let features = cargo["dependencies"]["forklaunch-core"]["features"].as_array_mut().context("core dependency features")?;
                if !features.iter().any(|value| value.as_str() == Some("mappers")) { features.push("mappers"); }
                file.content = cargo.to_string();
            }
            "domain/mappers/mod.rs" => file.content = "//! Explicit projections into validated public DTOs.\npub mod health;\npub use health::HealthMapper;\n".into(),
            "domain/services/health.rs" => {
                let repository = if postgres { "PostgresRepository" } else { "InMemoryRepository" };
                let count = if postgres { "self.repository.count().await?" } else { "self.repository.count()?" };
                file.content = format!("use crate::{{domain::{{schemas::HealthResponse, mappers::HealthMapper}}, persistence::{repository}, error::Result}};\n#[derive(Clone)]\npub struct HealthService {{ name: String, repository: {repository}, mapper: HealthMapper }}\nimpl HealthService {{\n    pub fn new(name: String, repository: {repository}) -> Result<Self> {{ Ok(Self {{ name, repository, mapper: HealthMapper::new()? }}) }}\n    pub async fn report(&self) -> Result<HealthResponse> {{\n        self.mapper.report(&self.name, {count}).await\n    }}\n}}\n");
            }
            "registrations.rs" => file.content = file.content.replace("HealthService::new(config.name.clone(), repository.clone())", "HealthService::new(config.name.clone(), repository.clone())?"),
            "api/controllers/health.rs" => file.content = file.content.replace("Ok(state.health.report())", "state.health.report().await.map_err(|_| EndpointError::Unavailable)"),
            "error.rs" => file.content = file.content.replace("    Storage(String),", "    Storage(String),\n    Mapping,").replace("            Self::Storage(message)", "            Self::Mapping => write!(formatter, \"invalid mapped response\"),\n            Self::Storage(message)"),
            _ => {}
        }
    }
    files.push(RenderedTemplate {
        path: dir.join("domain/mappers/health.rs"),
        content: render_static_rust_template(
            "rust/project/native-service/domain/mappers/health.rs",
        ),
        context: None,
    });
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapper_selection_wires_real_projection_for_memory_and_postgres() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("src/notes");
        for postgres in [false, true] {
            let base = native_layout::service_templates(&dir, "notes", "Notes");
            let base = if postgres {
                service_database::templates(&dir, "notes", base).unwrap()
            } else {
                base
            };
            let mapped = templates(&dir, true, base.clone()).unwrap();
            let source = |path: &str| {
                mapped
                    .iter()
                    .find(|file| file.path == dir.join(path))
                    .unwrap()
                    .content
                    .as_str()
            };
            assert!(source("domain/mappers/health.rs").contains("CheckedMapper"));
            assert!(source("domain/services/health.rs").contains("self.mapper.report"));
            assert!(source("registrations.rs").contains("repository.clone())?"));
            assert!(source("api/controllers/health.rs").contains("state.health.report().await"));
            if postgres {
                assert!(source("Cargo.toml").contains("0.9"));
            }
            let plain = templates(&dir, false, base).unwrap();
            assert!(
                !plain
                    .iter()
                    .any(|file| file.path.starts_with(dir.join("domain/mappers")))
            );
            assert!(
                !plain
                    .iter()
                    .find(|file| file.path == dir.join("domain/mod.rs"))
                    .unwrap()
                    .content
                    .contains("mod mappers")
            );
        }
    }
}
