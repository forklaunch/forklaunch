use std::path::Path;

use anyhow::Result;
use ramhorns::Template;

use super::manifest::ManifestData;
use super::rendered_template::RenderedTemplate;
use super::template::get_file_contents;

/// Generates GitHub configuration files (.github/workflows/ci.yml, .github/dependabot.yml,
/// and .github/BRANCH_PROTECTION.md) for a project.
///
/// Can be called from `init`, `sync`, and `change` commands.
/// Skips files that already exist on disk.
pub(crate) fn ensure_github_configs(
    base_path: &Path,
    manifest_data: &ManifestData,
) -> Result<Vec<RenderedTemplate>> {
    let mut rendered_templates = Vec::new();

    let github_dir = base_path.join(".github");
    let workflows_dir = github_dir.join("workflows");

    // CI workflow
    let ci_path = workflows_dir.join("ci.yml");
    if !ci_path.exists() {
        let ci_template_content =
            get_file_contents(Path::new("github/ci.yml"))?;
        let tpl = Template::new(ci_template_content)?;
        let rendered = match manifest_data {
            ManifestData::Application(data) => tpl.render(data),
            ManifestData::Service(data) => tpl.render(data),
            ManifestData::Library(data) => tpl.render(data),
            ManifestData::Router(data) => tpl.render(data),
            ManifestData::Worker(data) => tpl.render(data),
        };
        rendered_templates.push(RenderedTemplate {
            path: ci_path,
            content: rendered,
            context: Some("Failed to generate CI workflow".to_string()),
        });
    }

    // Dependabot config
    let dependabot_path = github_dir.join("dependabot.yml");
    if !dependabot_path.exists() {
        let dependabot_content =
            get_file_contents(Path::new("github/dependabot.yml"))?;
        rendered_templates.push(RenderedTemplate {
            path: dependabot_path,
            content: dependabot_content,
            context: Some("Failed to generate Dependabot config".to_string()),
        });
    }

    // Branch protection guide
    let branch_protection_path = github_dir.join("BRANCH_PROTECTION.md");
    if !branch_protection_path.exists() {
        let branch_protection_content =
            get_file_contents(Path::new("github/BRANCH_PROTECTION.md"))?;
        rendered_templates.push(RenderedTemplate {
            path: branch_protection_path,
            content: branch_protection_content,
            context: Some("Failed to generate branch protection guide".to_string()),
        });
    }

    Ok(rendered_templates)
}
