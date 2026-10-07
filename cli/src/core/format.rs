use std::path::Path;

use crate::{constants::Runtime, core::openapi_export::resolve_command};

pub(crate) fn format_code(base_path: &Path, runtime: &Runtime) {
    // Opt-out for callers that run their own format/lint pass afterward (e.g. the ForkLaunch
    // Studio orchestrator). The `format` script globs `**/*` and walks the pnpm-symlinked
    // node_modules tree, which can peg a core for minutes on a populated module — pure waste
    // when the caller reformats everything anyway. Default behavior is unchanged.
    if std::env::var_os("FORKLAUNCH_SKIP_FORMAT").is_some() {
        return;
    }
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
