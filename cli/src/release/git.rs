use std::process::Command;

use anyhow::{Context, Result};

/// Get the current git commit SHA
pub(crate) fn get_git_commit() -> Result<String> {
    let output = Command::new("git")
        .args(&["rev-parse", "HEAD"])
        .output()
        .with_context(|| "Failed to execute git command. Is git installed?")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to get git commit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let commit = String::from_utf8(output.stdout)
        .with_context(|| "Invalid UTF-8 in git output")?
        .trim()
        .to_string();

    Ok(commit)
}

/// Get the current git branch name.
///
/// Honors `FORKLAUNCH_GIT_BRANCH` env var first — useful when HEAD is
/// detached (e.g. the autorelease worker checks out a specific commit)
/// and `git` can't infer the branch on its own.
pub(crate) fn get_git_branch() -> Result<String> {
    if let Ok(b) = std::env::var("FORKLAUNCH_GIT_BRANCH") {
        let trimmed = b.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let output = Command::new("git")
        .args(&["branch", "--show-current"])
        .output()
        .with_context(|| "Failed to execute git command")?;

    if output.status.success() {
        let branch = String::from_utf8(output.stdout)
            .with_context(|| "Invalid UTF-8 in git output")?
            .trim()
            .to_string();
        if !branch.is_empty() {
            return Ok(branch);
        }
        // Detached HEAD — try to find a branch pointing at HEAD.
        if let Ok(out) = Command::new("git")
            .args(&[
                "for-each-ref",
                "--format=%(refname:short)",
                "--points-at",
                "HEAD",
                "refs/heads/",
            ])
            .output()
        {
            let candidate = String::from_utf8_lossy(&out.stdout);
            if let Some(first) = candidate.lines().next() {
                let trimmed = first.trim();
                if !trimmed.is_empty() {
                    return Ok(trimmed.to_string());
                }
            }
        }
    }

    let output = Command::new("git")
        .args(&["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .with_context(|| "Failed to get git branch")?;

    if !output.status.success() {
        return Ok("unknown".to_string());
    }

    let branch = String::from_utf8(output.stdout)
        .with_context(|| "Invalid UTF-8 in git output")?
        .trim()
        .to_string();

    Ok(branch)
}

pub(crate) fn is_git_repo() -> bool {
    Command::new("git")
        .args(&["rev-parse", "--git-dir"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
