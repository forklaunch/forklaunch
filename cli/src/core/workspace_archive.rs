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

/// Whole-archive ceiling. Beyond this the base64 body becomes unreasonable and
/// the caller deserves a clear error rather than a timeout or a 413.
const MAX_ARCHIVE_BYTES: usize = 48 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct ArchiveSummary {
    pub(crate) bytes: usize,
    pub(crate) files: usize,
    pub(crate) skipped_large: usize,
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

            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size > MAX_FILE_BYTES {
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
            .map(|i| archive.by_index(i).unwrap().name().to_string())
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
    fn an_empty_workspace_is_an_error_not_an_empty_upload() {
        // Uploading an empty archive would produce a confident, meaningless
        // score. Better to say nothing was found.
        let dir = tempfile::tempdir().unwrap();
        let err = pack_workspace(dir.path()).unwrap_err().to_string();
        assert!(err.contains("nothing to analyse"), "{err}");
    }
}
