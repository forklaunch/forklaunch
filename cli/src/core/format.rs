use std::path::Path;

use crate::{constants::Runtime, core::openapi_export::resolve_command};

pub(crate) fn format_code(base_path: &Path, runtime: &Runtime) {
    let command = match runtime {
        Runtime::Node => "pnpm",
        Runtime::Bun => "bun",
    };
    let resolved = resolve_command(command);
    let _ = std::process::Command::new(&resolved)
        .arg("format")
        .current_dir(base_path)
        .output();
}
