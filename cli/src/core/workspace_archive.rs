//! Pack a workspace into a zip for server-side analysis.
//!
//! `POST /analysis/zip` wants a base64 `.zip`, and the orchestrator extracts it
//! by shelling out to `unzip` — so it has to be a real zip archive, not a
//! tar.gz.
//!
//! # What gets excluded, and why it matters
//!
//! This uploads a copy of someone's source to a remote service, so what goes in
//! is a privacy decision rather than a size optimisation. Three layers:
//!
//! 1. `.gitignore` is honoured (via the `ignore` crate, the same walker
//!    ripgrep uses). Anything the repo already declines to track — `.env`,
//!    local certs, credential dumps — stays local.
//! 2. `ALWAYS_EXCLUDE` covers directories that are enormous, regenerable, or
//!    secret-bearing regardless of what `.gitignore` says. A repo that forgot
//!    to ignore `node_modules` should not silently upload it, and `.git` holds
//!    every secret ever committed and later removed.
//! 3. `MAX_FILE_BYTES` skips individual large files. Analysis reads source; a
//!    50MB fixture or binary contributes nothing and inflates the payload.
//!
//! Lockfiles are exempt from `MAX_FILE_BYTES` up to `MAX_LOCKFILE_BYTES`: the
//! dependency check proves pinning from them, and a large monorepo's lockfile
//! is routinely over 2 MB.
//!
//! `.git` never leaves the machine, so the server cannot ask git which files are
//! tracked (it uses that to credit only committed lockfiles). When the workspace
//! is a git checkout, the archive carries `TRACKED_FILES_LIST`: the paths
//! `git ls-files` reports, one per line — names only, no history or contents.
//!
//! The archive is held in memory because it then has to be base64'd into a JSON
//! body anyway. `MAX_ARCHIVE_BYTES` is what stops that being unbounded.

use std::io::{Cursor, Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use zip::write::SimpleFileOptions;

/// Excluded whatever `.gitignore` says. Regenerable, enormous, or secret-bearing.
const ALWAYS_EXCLUDE: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".venv",
    "__pycache__",
    ".pnpm-store",
    "coverage",
    ".DS_Store",
];

/// Per-file ceiling. Analysis reads source; anything this large is a fixture,
/// a binary, or a lockfile-shaped blob that adds payload without adding signal.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Lockfiles may be this large: the dependency check needs them.
const MAX_LOCKFILE_BYTES: u64 = 16 * 1024 * 1024;

const LOCKFILES: &[&str] = &[
    "pnpm-lock.yaml",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "Cargo.lock",
];

/// The tracked-file list the orchestrator reads when the upload has no `.git`
/// (studio-orchestrator repository-package-check.ts `TRACKED_FILES_LIST`).
pub(crate) const TRACKED_FILES_LIST: &str = ".forklaunch-tracked-files";

/// Whole-archive ceiling. Beyond this the base64 body becomes unreasonable and
/// the caller deserves a clear error rather than a timeout or a 413.
const MAX_ARCHIVE_BYTES: usize = 48 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct ArchiveSummary {
    pub(crate) bytes: usize,
    pub(crate) files: usize,
    pub(crate) skipped_large: usize,
}

fn is_lockfile(relative: &Path) -> bool {
    relative
        .file_name()
        .is_some_and(|name| LOCKFILES.contains(&name.to_string_lossy().as_ref()))
}

/// The files git tracks under `root`, relative to it; None outside a git checkout
/// or when git is unavailable.
fn tracked_files(root: &Path) -> Option<Vec<String>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--cached", "-z"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        output
            .stdout
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            // One path per line: a name with a newline cannot be listed.
            .filter(|p| !p.contains('\n') && p != TRACKED_FILES_LIST)
            .collect(),
    )
}

fn is_excluded(relative: &Path) -> bool {
    relative
        .components()
        .any(|c| ALWAYS_EXCLUDE.contains(&c.as_os_str().to_string_lossy().as_ref()))
}

/// Zip `root` into memory, returning the bytes and what went in.
pub(crate) fn pack_workspace(root: &Path) -> Result<(Vec<u8>, ArchiveSummary)> {
    let mut buffer = Vec::new();
    let mut files = 0usize;
    let mut skipped_large = 0usize;

    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        // `ignore` honours .gitignore/.ignore and skips hidden files by
        // default; hidden(false) lets through the dotfiles that carry real
        // configuration (.forklaunch, .github) without overriding .gitignore.
        let walker = WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .parents(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                // An unreadable path is not worth failing the whole analysis
                // over; the card is built from what could be read.
                Err(_) => continue,
            };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }

            let path = entry.path();
            let relative = match path.strip_prefix(root) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if is_excluded(relative) {
                continue;
            }

            if relative == Path::new(TRACKED_FILES_LIST) {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let limit = if is_lockfile(relative) {
                MAX_LOCKFILE_BYTES
            } else {
                MAX_FILE_BYTES
            };
            if size > limit {
                skipped_large += 1;
                continue;
            }

            let mut contents = Vec::new();
            if std::fs::File::open(path)
                .and_then(|mut f| f.read_to_end(&mut contents))
                .is_err()
            {
                continue;
            }

            writer
                .start_file(relative.to_string_lossy().replace('\\', "/"), options)
                .with_context(|| format!("failed to add {} to the archive", relative.display()))?;
            writer.write_all(&contents)?;
            files += 1;
        }

        if let Some(tracked) = tracked_files(root) {
            writer
                .start_file(TRACKED_FILES_LIST, options)
                .context("failed to add the tracked-file list to the archive")?;
            writer.write_all(tracked.join("\n").as_bytes())?;
        }

        writer.finish().context("failed to finalise the archive")?;
    }

    if buffer.len() > MAX_ARCHIVE_BYTES {
        anyhow::bail!(
            "workspace archive is {:.1} MB, over the {} MB limit — exclude large directories \
             or analyse a narrower path with --path",
            buffer.len() as f64 / (1024.0 * 1024.0),
            MAX_ARCHIVE_BYTES / (1024 * 1024)
        );
    }

    if files == 0 {
        anyhow::bail!(
            "nothing to analyse under {} — every file was ignored or excluded",
            root.display()
        );
    }

    let bytes = buffer.len();
    Ok((
        buffer,
        ArchiveSummary {
            bytes,
            files,
            skipped_large,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("index.ts"), "export const a = 1;").unwrap();
        fs::write(root.join(".gitignore"), ".env\nsecrets/\n").unwrap();
        fs::write(root.join(".env"), "API_KEY=super-secret").unwrap();
        fs::create_dir_all(root.join("secrets")).unwrap();
        fs::write(root.join("secrets/key.pem"), "-----BEGIN PRIVATE KEY-----").unwrap();
        fs::create_dir_all(root.join("node_modules/left-pad")).unwrap();
        fs::write(root.join("node_modules/left-pad/index.js"), "module.exports=1").unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), "[remote]").unwrap();
        dir
    }

    fn entries(bytes: &[u8]) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().unwrap().to_string())
            .collect()
    }

    #[test]
    fn includes_source() {
        let dir = workspace();
        let (bytes, summary) = pack_workspace(dir.path()).unwrap();
        assert!(entries(&bytes).contains(&"index.ts".to_string()));
        assert!(summary.files >= 1);
    }

    #[test]
    fn honours_gitignore_so_secrets_stay_local() {
        // The whole reason .gitignore is consulted: this uploads source to a
        // remote service, and a repo that declines to track a file has already
        // said it should not leave the machine.
        let dir = workspace();
        let (bytes, _) = pack_workspace(dir.path()).unwrap();
        let names = entries(&bytes);
        assert!(!names.iter().any(|n| n == ".env"), "{names:?}");
        assert!(
            !names.iter().any(|n| n.starts_with("secrets/")),
            "{names:?}"
        );
    }

    #[test]
    fn excludes_git_and_node_modules_even_if_not_gitignored() {
        // Neither is in this fixture's .gitignore. `.git` carries every secret
        // ever committed and later removed; node_modules is enormous and
        // regenerable. Both must go regardless.
        let dir = workspace();
        let (bytes, _) = pack_workspace(dir.path()).unwrap();
        let names = entries(&bytes);
        assert!(!names.iter().any(|n| n.starts_with(".git/")), "{names:?}");
        assert!(
            !names.iter().any(|n| n.starts_with("node_modules/")),
            "{names:?}"
        );
    }

    #[test]
    fn skips_oversized_files_and_reports_how_many() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("small.ts"), "export const a = 1;").unwrap();
        fs::write(dir.path().join("huge.bin"), vec![0u8; (MAX_FILE_BYTES + 1) as usize]).unwrap();

        let (bytes, summary) = pack_workspace(dir.path()).unwrap();
        assert_eq!(summary.skipped_large, 1);
        assert!(!entries(&bytes).contains(&"huge.bin".to_string()));
    }

    #[test]
    fn keeps_a_large_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("small.ts"), "export const a = 1;").unwrap();
        fs::write(
            dir.path().join("pnpm-lock.yaml"),
            vec![b'a'; (MAX_FILE_BYTES + 1) as usize],
        )
        .unwrap();
        let (bytes, summary) = pack_workspace(dir.path()).unwrap();
        assert_eq!(summary.skipped_large, 0);
        assert!(entries(&bytes).contains(&"pnpm-lock.yaml".to_string()));
    }

    #[test]
    fn lists_tracked_files_for_a_git_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(root)
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        };
        git(&["init", "--quiet"]);
        fs::write(root.join("index.ts"), "export const a = 1;").unwrap();
        fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'").unwrap();
        git(&["add", "index.ts"]);
        let (bytes, _) = pack_workspace(root).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut listed = String::new();
        archive
            .by_name(TRACKED_FILES_LIST)
            .unwrap()
            .read_to_string(&mut listed)
            .unwrap();
        assert_eq!(listed, "index.ts");
    }

    #[test]
    fn no_tracked_list_outside_git() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("index.ts"), "export const a = 1;").unwrap();
        let (bytes, _) = pack_workspace(dir.path()).unwrap();
        assert!(!entries(&bytes).contains(&TRACKED_FILES_LIST.to_string()));
    }

    #[test]
    fn an_empty_workspace_is_an_error_not_an_empty_upload() {
        // Uploading an empty archive would produce a confident, meaningless
        // score. Better to say nothing was found.
        let dir = tempfile::tempdir().unwrap();
        let err = pack_workspace(dir.path()).unwrap_err().to_string();
        assert!(err.contains("nothing to analyse"), "{err}");
    }
}
