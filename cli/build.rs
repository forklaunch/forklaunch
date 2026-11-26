//! Make cargo aware that the generated templates are an input.
//!
//! `src/templates` is baked into the binary with `include_dir!`, and several of
//! its module directories are symlinks into `blueprint/` so a module's template
//! and its reference implementation cannot drift. Cargo does not follow those
//! symlinks when deciding whether a rebuild is needed, so editing a blueprint
//! file leaves `cargo build` reporting success while the binary still carries
//! the previous copy of that module.
//!
//! The failure is quiet and expensive: `forklaunch init` keeps scaffolding the
//! old module, the generated project compiles cleanly, and nothing anywhere
//! says the template is stale. It cost a full verification round — a scaffold
//! was built, run, and paid through before the missing changes were noticed.
//!
//! Walking the tree and emitting `rerun-if-changed` for every file, resolving
//! symlinks as we go, makes the dependency explicit.

use std::path::Path;

fn main() {
    if let Ok(projects) = std::fs::read_dir("src/templates/project") {
        for project in projects.flatten() {
            let name = project.file_name();
            let owner = match name.to_str() {
                Some("billing-stripe") => "billing-base",
                Some("messaging-twilio") => "messaging-base",
                Some(name) => name,
                None => continue,
            };
            let source = Path::new("../blueprint").join(owner).join(".gitignore");
            let embedded = project.path().join(".gitignore");
            if source.is_file() && embedded.exists() {
                verify_embedded_source(&source, &embedded);
            }
        }
    }
    emit_rerun_for_tree(Path::new("src/templates"));
    // The blueprint is where the symlinked template directories actually live.
    emit_rerun_for_tree(Path::new("../blueprint"));
}

fn verify_embedded_source(source: &Path, embedded: &Path) {
    let matches = if source
        .file_name()
        .is_some_and(|name| name == ".gitignore" || name == ".gitattributes")
    {
        // Git refuses symlink ignore/attribute files; use exact source copies.
        !embedded.is_symlink()
            && std::fs::read(source)
                .ok()
                .zip(std::fs::read(embedded).ok())
                .is_some_and(|(source, embedded)| source == embedded)
    } else {
        embedded.canonicalize().ok() == source.canonicalize().ok()
    };
    assert!(
        matches,
        "Rust template inventory is stale at {}. Run `python3 scripts/sync_rust_embedded_sources.py` from the repository root before rebuilding the CLI.",
        embedded.display()
    );
}

fn emit_rerun_for_tree(dir: &Path) {
    // canonicalize resolves the symlink, so the paths handed to cargo are the
    // real files rather than the links pointing at them.
    let Ok(resolved) = dir.canonicalize() else {
        return;
    };
    println!("cargo:rerun-if-changed={}", resolved.display());

    let Ok(entries) = std::fs::read_dir(&resolved) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // node_modules and build output dwarf the template tree and never
        // affect what gets baked in; walking them would make every build slow
        // for no benefit.
        if path.file_name().is_some_and(|name| {
            matches!(
                name.to_str(),
                Some("node_modules") | Some("dist") | Some("lib") | Some(".git") | Some("target")
            )
        }) {
            continue;
        }
        if path.is_dir() {
            emit_rerun_for_tree(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
