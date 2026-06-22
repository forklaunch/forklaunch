use std::{
    env::current_dir,
    io::IsTerminal,
    path::{MAIN_SEPARATOR, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::ArgMatches;
use rustyline::{Editor, history::DefaultHistory};
use termcolor::StandardStream;
use walkdir::WalkDir;

use crate::{
    constants::{ERROR_FAILED_TO_GET_CWD, ERROR_MANIFEST_NOT_FOUND},
    core::manifest::ManifestData,
    prompt::{ArrayCompleter, prompt_with_validation},
};

pub(crate) fn check_base_path(base_path: &PathBuf, parent_count: usize) -> bool {
    let mut base_path_to_test = base_path.to_path_buf();
    for _ in 0..parent_count {
        base_path_to_test = base_path_to_test.parent().unwrap().to_path_buf();
    }

    base_path_to_test
        .join(".forklaunch")
        .join("manifest.toml")
        .exists()
}

pub(crate) fn prompt_base_path(
    app_root_path: &PathBuf,
    manifest_data: &ManifestData,
    project_name: &Option<String>,
    line_editor: &mut Editor<ArrayCompleter, DefaultHistory>,
    stdout: &mut StandardStream,
    matches: &ArgMatches,
    module_path_offset: usize,
) -> Result<PathBuf> {
    let modules_path = match manifest_data {
        ManifestData::Application(application_manifest_data) => {
            application_manifest_data.modules_path.clone()
        }
        ManifestData::Router(router_manifest_data) => router_manifest_data.modules_path.clone(),
        ManifestData::Service(service_manifest_data) => service_manifest_data.modules_path.clone(),
        ManifestData::Library(library_manifest_data) => library_manifest_data.modules_path.clone(),
        ManifestData::Worker(worker_manifest_data) => worker_manifest_data.modules_path.clone(),
    };
    let path_count = modules_path.split(MAIN_SEPARATOR).count() + module_path_offset;

    let mut base_path = match project_name {
        Some(project_name) => app_root_path
            .join(modules_path.clone())
            .join(project_name.clone()),
        None => app_root_path.join(modules_path.clone()),
    };

    // On a non-interactive stdin (studio/CI pass -p explicitly), the re-prompt loop below can
    // never make progress: prompt_with_validation reads EOF, the path still doesn't exist, and it
    // spins at 100% CPU forever. This is the `forklaunch init router -p src/modules/<missing>`
    // hang — confirmed via live /proc snapshot (state=R, no output). Fail fast with a clear,
    // actionable error instead of looping.
    if (!base_path.exists() || !check_base_path(&base_path, path_count)) && !std::io::stdin().is_terminal() {
        bail!(
            "base path '{}' does not exist (or has no forklaunch manifest above it) and stdin is \
             not a TTY to prompt for a correction. Create the target module first, e.g. \
             `forklaunch init service <name>`.",
            base_path.display()
        );
    }

    while !base_path.exists() || !check_base_path(&base_path, path_count) {
        base_path = PathBuf::from(prompt_with_validation(
            line_editor,
            stdout,
            "base_path",
            matches,
            "base path",
            None,
            |input: &str| {
                let input_path = &Path::new(input).to_path_buf();
                check_base_path(&input_path, path_count)
            },
            |_| "Base path is not correct. Please try again".to_string(),
        )?);
    }

    Ok(base_path)
}

pub(crate) fn find_nearest_manifest_from(start: &Path) -> Option<PathBuf> {
    let mut base_path = start.canonicalize().ok()?;
    loop {
        let manifest = base_path.join(".forklaunch").join("manifest.toml");
        if manifest.exists() {
            return Some(base_path.clone());
        }
        match base_path.parent() {
            Some(parent) => base_path = parent.to_path_buf(),
            None => break,
        }
    }
    None
}

pub(crate) fn find_nearest_manifest_root_unbounded() -> Option<PathBuf> {
    let mut base_path = current_dir().ok()?;
    loop {
        let manifest = base_path.join(".forklaunch").join("manifest.toml");
        if manifest.exists() {
            return Some(base_path);
        }
        match base_path.parent() {
            Some(parent) => base_path = parent.to_path_buf(),
            None => break,
        }
    }
    None
}

fn find_in_modules_dirs(start_dir: &Path, file_name: &str) -> Option<PathBuf> {
    for entry in WalkDir::new(start_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_dir() {
            let entry_path = entry.path();

            let module_path = entry_path.join("modules").join(file_name);
            if module_path.exists() {
                return Some(module_path);
            }
        }
    }
    None
}

pub(crate) enum RequiredLocation {
    Application,
    Project,
}

pub(crate) fn find_app_root_path(
    matches: &ArgMatches,
    required_location: RequiredLocation,
) -> Result<(PathBuf, Option<String>)> {
    let current_dir = current_dir().with_context(|| ERROR_FAILED_TO_GET_CWD)?;
    let relative_path = matches.get_one::<String>("base_path");

    let start_path = match relative_path {
        Some(relative_path) => {
            if current_dir.join(relative_path.clone()).exists() {
                current_dir.join(relative_path.clone())
            } else {
                let file_name = Path::new(relative_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(relative_path);

                find_in_modules_dirs(&current_dir, file_name).unwrap_or(current_dir.clone())
            }
        }
        None => current_dir.clone(),
    };

    let project_name = match required_location {
        RequiredLocation::Application => None,
        RequiredLocation::Project => Some(
            start_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        ),
    };

    let manifest_path = find_nearest_manifest_from(&start_path);

    match manifest_path {
        Some(manifest) => Ok((manifest, project_name)),
        None => bail!(ERROR_MANIFEST_NOT_FOUND),
    }
}
