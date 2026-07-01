//! `forklaunch context` — drops the ForkLaunch skill/context pack into the right location for a
//! given coding agent, so an external agent (Claude Code, Cursor, Windsurf, Codex, …) builds
//! ForkLaunch apps following framework conventions.
//!
//! The skill content is the canonical `.claude/skills` pack, embedded into the binary at build
//! time (`include_dir!`), so this works offline and the agent always gets the conventions that
//! match this CLI version. The studio's "copy plan for a coding agent" output points here.

use std::{
    fs::{create_dir_all, write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Arg, ArgMatches, Command};
use include_dir::{Dir, DirEntry, File, include_dir};

use crate::{CliCommand, core::command::command};

/// The ForkLaunch skill/context pack, vendored from `.claude/skills` and embedded in the binary.
static SKILLS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/assets/forklaunch-skills");

pub(crate) struct ContextCommand;

impl ContextCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for ContextCommand {
    fn command(&self) -> Command {
        command(
            "context",
            "Drop the ForkLaunch skill/context pack for a coding agent (Claude Code, Cursor, …).",
        )
        .alias("skills")
        .arg(
            Arg::new("agent")
                .short('a')
                .long("agent")
                .help("Target coding agent — determines where the context files are written")
                .value_parser(["claude", "cursor", "windsurf", "codex", "generic"])
                .default_value("claude"),
        )
        .arg(
            Arg::new("path")
                .short('p')
                .long("path")
                .help("Output directory (defaults to the current directory)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let agent = matches
            .get_one::<String>("agent")
            .map(String::as_str)
            .unwrap_or("claude");
        let out = match matches.get_one::<String>("path") {
            Some(p) => PathBuf::from(p),
            None => std::env::current_dir().context("could not resolve the current directory")?,
        };

        let files = collect_md_files(&SKILLS);

        match agent {
            // Claude Code consumes the skills natively — preserve the directory structure.
            "claude" => {
                for f in &files {
                    let dest = out.join(".claude").join("skills").join(f.path());
                    if let Some(parent) = dest.parent() {
                        create_dir_all(parent)?;
                    }
                    write(&dest, f.contents())?;
                }
                eprintln!(
                    "✓ Wrote {} ForkLaunch skill files to .claude/skills/",
                    files.len()
                );
            }
            // Cursor reads `.cursor/rules/*.mdc` — one always-applied rule bundle.
            "cursor" => {
                let dest = out.join(".cursor").join("rules").join("forklaunch.mdc");
                write_bundle(&dest, &files, true)?;
                eprintln!("✓ Wrote ForkLaunch rules to .cursor/rules/forklaunch.mdc");
            }
            // Windsurf reads `.windsurf/rules`.
            "windsurf" => {
                let dest = out.join(".windsurf").join("rules").join("forklaunch.md");
                write_bundle(&dest, &files, false)?;
                eprintln!("✓ Wrote ForkLaunch rules to .windsurf/rules/forklaunch.md");
            }
            // codex / generic — the cross-agent `AGENTS.md` convention.
            _ => {
                let dest = out.join("AGENTS.md");
                write_bundle(&dest, &files, false)?;
                eprintln!("✓ Wrote ForkLaunch context to AGENTS.md");
            }
        }
        Ok(())
    }
}

/// Recursively collect every embedded `.md` skill file.
fn collect_md_files<'a>(dir: &'a Dir<'a>) -> Vec<&'a File<'a>> {
    let mut out = Vec::new();
    collect_into(dir, &mut out);
    out.sort_by_key(|f| f.path().to_path_buf());
    out
}

fn collect_into<'a>(dir: &'a Dir<'a>, out: &mut Vec<&'a File<'a>>) {
    for entry in dir.entries() {
        match entry {
            DirEntry::File(f) => {
                if f.path().extension().and_then(|e| e.to_str()) == Some("md") {
                    out.push(f);
                }
            }
            DirEntry::Dir(d) => collect_into(d, out),
        }
    }
}

/// Write a single bundled context file (all skills concatenated), optionally with Cursor `.mdc`
/// frontmatter so the rule is always applied.
fn write_bundle(dest: &Path, files: &[&File], mdc: bool) -> Result<()> {
    if let Some(parent) = dest.parent() {
        create_dir_all(parent)?;
    }
    let mut body = String::new();
    if mdc {
        body.push_str("---\ndescription: ForkLaunch framework conventions and skills\nalwaysApply: true\n---\n\n");
    }
    body.push_str("# ForkLaunch Skill Pack\n\nFollow these conventions when building this ForkLaunch app.\n");
    for f in files {
        body.push_str(&format!(
            "\n\n<!-- ===== {} ===== -->\n\n",
            f.path().display()
        ));
        body.push_str(f.contents_utf8().unwrap_or(""));
    }
    write(dest, body)?;
    Ok(())
}
