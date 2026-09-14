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

/// The `origin` remote of the current checkout as an `https://` URL, or None
/// when there is no remote. Normalises the SSH and `.git` spellings so the
/// value matches what the platform stores for a connected repository:
///   git@github.com:acme/app.git      -> https://github.com/acme/app
///   ssh://git@github.com/acme/app.git -> https://github.com/acme/app
///   https://github.com/acme/app.git  -> https://github.com/acme/app
pub(crate) fn get_git_remote_url() -> Option<String> {
    let output = Command::new("git")
        .args(&["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8(output.stdout).ok()?;
    normalize_git_remote_url(raw.trim())
}

pub(crate) fn normalize_git_remote_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let without_scheme = if let Some(rest) = raw.strip_prefix("ssh://") {
        // ssh://git@host/owner/repo(.git)
        let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
        rest.to_string()
    } else if let Some(rest) = raw.strip_prefix("git@") {
        // git@host:owner/repo(.git)
        rest.replacen(':', "/", 1)
    } else if let Some(rest) = raw
        .strip_prefix("https://")
        .or_else(|| raw.strip_prefix("http://"))
    {
        // https://[user[:token]@]host/owner/repo(.git)
        rest.rsplit_once('@')
            .map(|(_, r)| r)
            .unwrap_or(rest)
            .to_string()
    } else {
        return None;
    };
    let trimmed = without_scheme
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/');
    if trimmed.split('/').count() < 3 {
        return None;
    }
    Some(format!("https://{}", trimmed))
}

#[cfg(test)]
mod remote_url_tests {
    use super::normalize_git_remote_url;

    #[test]
    fn normalizes_ssh_https_and_git_suffix() {
        for raw in [
            "git@github.com:acme/app.git",
            "ssh://git@github.com/acme/app.git",
            "https://github.com/acme/app.git",
            "https://github.com/acme/app",
            "https://x-access-token:tok@github.com/acme/app.git",
            "https://github.com/acme/app/",
        ] {
            assert_eq!(
                normalize_git_remote_url(raw).as_deref(),
                Some("https://github.com/acme/app"),
                "{raw}"
            );
        }
    }

    #[test]
    fn rejects_empty_and_local_paths() {
        assert_eq!(normalize_git_remote_url(""), None);
        assert_eq!(normalize_git_remote_url("/srv/git/app.git"), None);
        assert_eq!(normalize_git_remote_url("https://github.com/acme"), None);
    }
}
