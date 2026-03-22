use std::{fs::create_dir_all, path::Path};

use anyhow::{Context, Result};
use include_dir::{Dir, File};
use ramhorns::Template;
use regex::Regex;

use super::{
    manifest::ManifestData,
    rendered_template::{RenderedTemplate, TEMPLATES_DIR},
};
use crate::constants::{Module, error_failed_to_create_dir};

#[derive(Debug, Clone)]
pub(crate) struct PathIO {
    pub(crate) input_path: String,
    pub(crate) output_path: String,
    pub(crate) module_id: Option<Module>,
}

pub(crate) fn get_directory_filenames(path: &'_ PathIO) -> Result<Vec<&'_ File<'_>>> {
    Ok(TEMPLATES_DIR
        .get_dir(&path.input_path)
        .unwrap()
        .files()
        .collect())
}

pub(crate) fn get_directory_subdirectory_names(path: &'_ PathIO) -> Result<Vec<&'_ Dir<'_>>> {
    Ok(TEMPLATES_DIR
        .get_dir(&path.input_path)
        .unwrap()
        .dirs()
        .collect())
}

pub(crate) fn get_file_contents(filepath: &Path) -> Result<String> {
    Ok(TEMPLATES_DIR
        .get_file(filepath)
        .unwrap()
        .contents_utf8()
        .unwrap()
        .to_string())
}

fn database_replacements(database: &String, template: String) -> String {
    match database.as_str() {
        "mongodb" => {
            let re = Regex::new(r"([^a-zA-Z0-9_$]|^)sqlBaseProperties").unwrap();
            re.replace_all(&template, "${1}nosqlBaseProperties")
                .replace("id: uuid", "id: string")
        }
        _ => template,
    }
}

fn forklaunch_replacements(app_name: &String, template: String) -> String {
    template.replace("@forklaunch/blueprint-", format!("@{app_name}/").as_str())
}

pub(crate) fn generate_with_template(
    output_prefix: Option<&String>,
    template_dir: &PathIO,
    data: &ManifestData,
    ignore_files: &Vec<String>,
    ignore_dirs: &Vec<String>,
    preserve_files: &Vec<String>,
    dryrun: bool,
) -> Result<Vec<RenderedTemplate>> {
    let mut rendered_templates = Vec::new();

    let output_dir = match output_prefix {
        Some(output_prefix) => Path::new(output_prefix).join(&template_dir.output_path),
        None => Path::new(&template_dir.output_path).to_path_buf(),
    };

    for entry in get_directory_filenames(&template_dir)? {
        let output_path_template =
            Template::new(entry.path().file_name().unwrap().to_string_lossy())?;
        let output_path = output_dir.join(match data {
            ManifestData::Application(manifest_data) => output_path_template.render(manifest_data),
            ManifestData::Service(manifest_data) => output_path_template.render(manifest_data),
            ManifestData::Library(manifest_data) => output_path_template.render(manifest_data),
            ManifestData::Router(manifest_data) => output_path_template.render(manifest_data),
            ManifestData::Worker(manifest_data) => output_path_template.render(manifest_data),
        });

        if !output_path.exists() && !dryrun {
            create_dir_all(output_path.parent().unwrap())
                .with_context(|| error_failed_to_create_dir(&output_path.parent().unwrap()))?;
        }

        let file_contents = get_file_contents(&Path::new(&entry.path())).with_context(|| {
            format!(
                "Failed to parse template file {}",
                &entry.path().to_string_lossy()
            )
        })?;
        let tpl = Template::new(file_contents.clone())?;
        let rendered = match data {
            ManifestData::Application(manifest_data) => {
                forklaunch_replacements(&manifest_data.app_name, tpl.render(&manifest_data))
            }
            ManifestData::Service(manifest_data) => database_replacements(
                &manifest_data.database,
                forklaunch_replacements(&manifest_data.app_name, tpl.render(&manifest_data)),
            ),
            ManifestData::Library(manifest_data) => {
                forklaunch_replacements(&manifest_data.app_name, tpl.render(&manifest_data))
            }
            ManifestData::Router(manifest_data) => {
                forklaunch_replacements(&manifest_data.app_name, tpl.render(&manifest_data))
            }
            ManifestData::Worker(manifest_data) => {
                forklaunch_replacements(&manifest_data.app_name, tpl.render(&manifest_data))
            }
        };
        if !output_path.exists() {
            let output_path_str = output_path.file_name().unwrap().to_str().unwrap();
            let should_ignore = ignore_files
                .iter()
                .any(|ignore_file| output_path_str == ignore_file);
            let should_preserve = preserve_files
                .iter()
                .any(|preserve_file| output_path_str == preserve_file);

            if !should_ignore {
                if should_preserve {
                    rendered_templates.push(RenderedTemplate {
                        path: output_path,
                        content: file_contents,
                        context: None,
                    });
                } else {
                    rendered_templates.push(RenderedTemplate {
                        path: output_path,
                        content: rendered,
                        context: None,
                    });
                }
            }
        }
    }

    for subdirectory in get_directory_subdirectory_names(&template_dir)? {
        let should_ignore = ignore_dirs
            .iter()
            .any(|ignore_dir| subdirectory.path().to_string_lossy().contains(ignore_dir));

        if !should_ignore {
            rendered_templates.extend(
                generate_with_template(
                    output_prefix,
                    &PathIO {
                        input_path: Path::new(&subdirectory.path())
                            .to_string_lossy()
                            .to_string(),
                        output_path: Path::new(&template_dir.output_path)
                            .join(&subdirectory.path().file_name().unwrap())
                            .to_string_lossy()
                            .to_string(),
                        module_id: None,
                    },
                    data,
                    ignore_files,
                    ignore_dirs,
                    preserve_files,
                    dryrun,
                )
                .with_context(|| {
                    format!(
                        "Failed to create templates for {}",
                        &subdirectory.path().to_string_lossy()
                    )
                })?,
            );
        }
    }

    Ok(rendered_templates)
}

pub(crate) fn get_routers_from_standard_package(package: Module) -> Option<Vec<String>> {
    match package {
        Module::BaseBilling => Some(vec![
            String::from("billingPortal"),
            String::from("checkoutSession"),
            String::from("paymentLink"),
            String::from("plan"),
            String::from("subscription"),
        ]),
        Module::StripeBilling => Some(vec![
            String::from("billingPortal"),
            String::from("checkoutSession"),
            String::from("paymentLink"),
            String::from("plan"),
            String::from("subscription"),
            String::from("webhook"),
        ]),
        Module::BaseIam | Module::BetterAuthIam => Some(vec![
            String::from("organization"),
            String::from("permission"),
            String::from("role"),
            String::from("user"),
        ]),
    }
}
