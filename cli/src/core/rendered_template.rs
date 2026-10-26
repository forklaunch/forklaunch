use std::{
    collections::{HashMap, hash_map::Drain},
    fs::{create_dir_all, write},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use fs_extra::file::read_to_string;
use include_dir::{Dir, include_dir};
use termcolor::StandardStream;

use super::watermark::apply_watermark;
use crate::constants::ERROR_FAILED_TO_CREATE_DIR;

pub(crate) static TEMPLATES_DIR: Dir = include_dir!("src/templates");

#[derive(Debug, Clone)]
pub(crate) struct RenderedTemplate {
    pub(crate) path: PathBuf,
    pub(crate) content: String,
    pub(crate) context: Option<String>,
}

pub(crate) fn create_forklaunch_dir(path_dir: &String, dryrun: bool) -> Result<()> {
    if !dryrun {
        let forklaunch_path = Path::new(path_dir).join(".forklaunch");
        create_dir_all(&forklaunch_path).with_context(|| ERROR_FAILED_TO_CREATE_DIR)?;
    }
    Ok(())
}

impl RenderedTemplate {
    /// For a package.json the CLI reserialized: keep the file's own key
    /// order (and its bytes when nothing changed). See
    /// [`super::package_json::preserve_json_layout`].
    pub(crate) fn preserving_json_layout(mut self) -> Self {
        if let Ok(original) = std::fs::read_to_string(&self.path) {
            self.content = super::package_json::preserve_json_layout(&original, &self.content);
        }
        self
    }
}

pub(crate) fn write_rendered_templates(
    rendered_templates: &Vec<RenderedTemplate>,
    dryrun: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    for rendered_template in rendered_templates {
        if !dryrun {
            create_dir_all(&rendered_template.path.parent().unwrap()).with_context(|| {
                format!(
                    "Failed to create parent directory for {}",
                    rendered_template.path.display()
                )
            })?;

            let mut content = apply_watermark(&rendered_template)?;
            if super::manifest::is_manifest_path(&rendered_template.path) {
                content = super::manifest::strip_retired_manifest_keys(&content);
                if let Ok(original) = std::fs::read_to_string(&rendered_template.path) {
                    content = super::manifest::preserve_manifest_layout(&original, &content);
                }
            }
            write(&rendered_template.path, content)
            .with_context(|| match &rendered_template.context {
                Some(context) => context.clone(),
                None => format!(
                    "Failed to write {}. Please check your target directory is writable",
                    rendered_template.path.display()
                ),
            })?;
        } else {
            writeln!(stdout, "Would write {}", rendered_template.path.display())?;
        }
    }
    Ok(())
}

/// The dry-run plan of a native change that edits files in place: the same
/// "Would write" lines rendered templates print.
pub(crate) fn log_dryrun_plan<S: AsRef<str>>(stdout: &mut StandardStream, paths: &[S]) -> Result<()> {
    for path in paths {
        match path.as_ref().strip_suffix(" (removed)") {
            Some(removed) => writeln!(stdout, "Would remove {removed}")?,
            None => writeln!(stdout, "Would write {}", path.as_ref())?,
        }
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct RenderedTemplatesCache {
    internal_cache: HashMap<String, RenderedTemplate>,
}

impl RenderedTemplatesCache {
    pub(crate) fn new() -> Self {
        Self {
            internal_cache: HashMap::new(),
        }
    }

    pub(crate) fn get<P: AsRef<Path>>(&self, key: P) -> Result<Option<RenderedTemplate>> {
        if self
            .internal_cache
            .contains_key(&key.as_ref().to_string_lossy().to_string())
        {
            Ok(Some(
                self.internal_cache
                    .get(&key.as_ref().to_string_lossy().to_string())
                    .unwrap()
                    .clone(),
            ))
        } else {
            read_to_string(&key).map_or(Ok(None), |content| {
                let template = RenderedTemplate {
                    path: key.as_ref().to_path_buf(),
                    content,
                    context: None,
                };
                Ok(Some(template))
            })
        }
    }

    pub(crate) fn insert<K: ToString>(&mut self, key: K, value: RenderedTemplate) {
        self.internal_cache.insert(key.to_string(), value);
    }

    pub(crate) fn drain(&mut self) -> Drain<'_, std::string::String, RenderedTemplate> {
        self.internal_cache.drain()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::TEMPLATES_DIR;
    use crate::constants::Module;

    // ecommerce-stripe was registered in the Module enum without the rest of
    // the pipeline: no cli/src/templates/project/ecommerce-stripe farm (panics
    // at scaffold time), no ProjectDependencies fields, no version consts, no
    // is_ecommerce manifest flag, and blueprint/ecommerce-stripe ships no
    // __test__. Completing it is its own change; do not add to this list.
    const KNOWN_MISSING_TEMPLATE_DIRS: &[&str] = &["ecommerce-stripe"];

    #[test]
    fn every_module_variant_has_an_embedded_template_dir() {
        for variant in Module::VARIANTS {
            let module = Module::from_str(variant).unwrap();
            let Some(exclusive_files) = module.metadata().exclusive_files else {
                continue;
            };
            for dir in exclusive_files {
                if KNOWN_MISSING_TEMPLATE_DIRS.contains(dir) {
                    continue;
                }
                assert!(
                    TEMPLATES_DIR
                        .get_dir(std::path::Path::new("project").join(dir))
                        .is_some(),
                    "Module '{variant}' declares template dir '{dir}' but src/templates/project/{dir} is not embedded — `forklaunch init module -m {variant}` would panic"
                );
            }
        }
    }
}
