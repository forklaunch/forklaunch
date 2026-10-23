use std::path::Path;

use anyhow::Result;
use ramhorns::Template;

use super::rendered_template::{RenderedTemplate, TEMPLATES_DIR};
use crate::{constants::License, core::manifest::application::ApplicationManifestData};

pub(crate) fn generate_license(
    app_path: &Path,
    data: &ApplicationManifestData,
) -> Result<Option<RenderedTemplate>> {
    let license_file = match data.license.parse::<License>()?.metadata().exclusive_files {
        Some(files) => files.first().unwrap(),
        None => return Ok(None),
    };

    // The manifest has no year to render, so `{{year}}` came out empty
    // ("Copyright (c)  app"); the copyright year is the year it is written.
    let source = TEMPLATES_DIR
        .get_file(Path::new("licenses").join(license_file))
        .unwrap()
        .contents_utf8()
        .unwrap()
        .replace("{{year}}", &chrono::Utc::now().format("%Y").to_string());
    let license_template = Template::new(source.as_str())?;

    Ok(Some(RenderedTemplate {
        path: app_path.join("LICENSE"),
        content: license_template.render(data),
        context: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mit_license_names_the_year_and_the_application() {
        let manifest: ApplicationManifestData = toml::from_str("id='x'\ncli_version='1.0.0'\napp_name='zebra'\nmodules_path='src'\nruntime='node'\nformatter='prettier'\nlinter='oxlint'\nvalidator='zod'\nhttp_framework='express'\ntest_framework='vitest'\napp_description='d'\nauthor='a'\nlicense='MIT'\nprojects=[]\nproject_peer_topology={}\n").unwrap();
        let license = generate_license(Path::new("."), &manifest).unwrap().unwrap().content;
        let year = chrono::Utc::now().format("%Y").to_string();
        assert!(license.contains(&format!("Copyright (c) {year} zebra\n")), "{license}");
    }
}
