use std::path::Path;

use anyhow::Result;

use super::rendered_template::RenderedTemplate;

pub(crate) fn generate_gitignore(path_dir: &Path) -> Result<Option<RenderedTemplate>> {
    let path = path_dir.join(".gitignore");

    if path.exists() {
        return Ok(None);
    }

    Ok(Some(RenderedTemplate {
        path,
        // Keep generic modules and application scaffolds on the same policy.
        content: include_str!("../templates/application/.gitignore").to_owned(),
        context: None,
    }))
}
