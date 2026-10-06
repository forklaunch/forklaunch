#[cfg(unix)]
use std::os::unix::prelude::PermissionsExt;
use std::{
    env::{args, current_dir},
    fs::{File, create_dir_all, metadata, read_to_string, rename, set_permissions},
    io::{Write, copy},
    path::PathBuf,
    process::{Command as OsCommand, Stdio, exit},
};

use anyhow::{Context, Result};
use clap::ArgMatches;
use reqwest::blocking::Client;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::base_path::{find_nearest_manifest_from, find_nearest_manifest_root_unbounded};
use crate::prompt::{ArrayCompleter, prompt_for_confirmation};

#[derive(Debug)]
pub(crate) enum VersionCheckOutcome {
    SkipNoManifest,
    SkipWhitelisted,
    Ok,
    ReexecNotSupported,
}

fn current_cli_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn parse_required_cli_version(manifest_root: &PathBuf) -> Option<String> {
    let manifest_path = manifest_root.join(".forklaunch").join("manifest.toml");
    let content = read_to_string(&manifest_path).ok()?;
    let value: toml::Value = toml::from_str(&content).ok()?;
    value
        .get("cli_version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn platform_triple() -> Result<&'static str> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return Ok("darwin-aarch64");
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        return Ok("darwin-x86_64");
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return Ok("linux-aarch64");
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return Ok("linux-x86_64");
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return Ok("windows-x86_64");
    }
    #[allow(unreachable_code)]
    Err(anyhow::anyhow!("Unsupported platform"))
}

/// Where a project-pinned CLI version lives on this machine.
///
/// Each version has its own file. It used to be installed over
/// `~/.forklaunch/bin/forklaunch` itself, which meant that running one
/// command in a project with an old pin silently downgraded the CLI for
/// every other project, and took the account keyring away with it.
fn pinned_binary_path(required_version: &str) -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .map(|p| p.join(".forklaunch").join("bin"))
        .context("Failed to resolve install directory")?;
    let name = if platform_triple()?.starts_with("windows") {
        format!("forklaunch-{required_version}.exe")
    } else {
        format!("forklaunch-{required_version}")
    };
    Ok(dir.join(name))
}

fn download_binary(required_version: &str) -> Result<PathBuf> {
    let platform = platform_triple()?;
    let artifact_name = if platform.starts_with("windows") {
        format!("forklaunch-{}.exe", platform)
    } else {
        format!("forklaunch-{}", platform)
    };
    let url = format!(
        "https://github.com/forklaunch/forklaunch-js/releases/download/cli-v{}/{}",
        required_version, artifact_name
    );

    let binary_path = pinned_binary_path(required_version)?;
    if let Some(dir) = binary_path.parent() {
        create_dir_all(dir).ok();
    }

    let client = Client::builder().build()?;
    let mut resp = client.get(&url).send()?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "Failed to download forklaunch v{} ({}): HTTP {}",
            required_version,
            platform,
            resp.status()
        );
    }
    // Write to a temp file first, then atomically rename into place, so a
    // concurrent command never executes a half-written binary.
    let tmp_path = binary_path.with_extension(format!("tmp{}", std::process::id()));
    let mut file = File::create(&tmp_path)?;
    copy(&mut resp, &mut file)?;
    drop(file);
    #[cfg(unix)]
    {
        let mut perms = metadata(&tmp_path)?.permissions();
        perms.set_mode(0o755);
        set_permissions(&tmp_path, perms)?;
    }
    rename(&tmp_path, &binary_path)?;

    Ok(binary_path)
}

/// The application root an invocation targets: `--base_path`/`--path` on the
/// deepest subcommand when given, else the nearest manifest above the
/// working directory.
pub(crate) fn invocation_manifest_root(matches: &ArgMatches) -> Option<PathBuf> {
    let mut current_matches = matches;

    while let Some((_, sub_matches)) = current_matches.subcommand() {
        current_matches = sub_matches;
    }

    let start_path: PathBuf = if current_matches.try_get_one::<String>("base_path").is_ok()
        && let Some(base_path) = current_matches.get_one::<String>("base_path")
    {
        PathBuf::from(base_path)
    } else if current_matches.try_get_one::<String>("path").is_ok()
        && let Some(path) = current_matches.get_one::<String>("path")
    {
        PathBuf::from(path)
    } else {
        current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    find_nearest_manifest_from(&start_path).or_else(|| find_nearest_manifest_root_unbounded())
}

/// Does this (older, project-pinned) binary know about the account keyring?
/// Asked of the binary itself, because the alternative is a version table
/// that has to be right about every release ever cut.
fn supports_accounts(binary_path: &PathBuf) -> bool {
    OsCommand::new(binary_path)
        .args(["account", "--help"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Drop `--account <name>` / `--account=<name>` from an argument list bound
/// for a binary that would reject the flag.
fn without_account_flag(args: impl Iterator<Item = String>) -> Vec<String> {
    let mut kept = Vec::new();
    let mut skip_value = false;
    for arg in args {
        if skip_value {
            skip_value = false;
        } else if arg == "--account" {
            skip_value = true;
        } else if !arg.starts_with("--account=") {
            kept.push(arg);
        }
    }
    kept
}

pub(crate) fn precheck_version(
    matches: &ArgMatches,
    subcommand: &str,
) -> Result<VersionCheckOutcome> {
    if subcommand == "init" {
        if let Some((child, _)) = matches.subcommand() {
            if child == "application" {
                return Ok(VersionCheckOutcome::SkipWhitelisted);
            }
        }
    }

    // The keyring belongs to the machine, not to the project the command
    // happens to be typed in. Handing these to a project-pinned older CLI
    // would give `account` to a binary that has never heard of it.
    if matches!(subcommand, "account" | "login" | "logout") {
        return Ok(VersionCheckOutcome::SkipWhitelisted);
    }

    let manifest_root = invocation_manifest_root(matches);
    let Some(manifest_root) = manifest_root else {
        return Ok(VersionCheckOutcome::SkipNoManifest);
    };
    let Some(required_version) = parse_required_cli_version(&manifest_root) else {
        return Ok(VersionCheckOutcome::Ok);
    };

    let current = current_cli_version();
    if current == required_version || current == "0.0.0" {
        return Ok(VersionCheckOutcome::Ok);
    }

    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    let cached = pinned_binary_path(&required_version)?;
    let binary_path = if cached.exists() {
        cached
    } else {
        log_warn!(
            stdout,
            "This project requires forklaunch CLI v{}, but you are running v{}.",
            required_version,
            current
        );

        let mut line_editor =
            rustyline::Editor::<ArrayCompleter, rustyline::history::DefaultHistory>::new()?;
        let confirm = prompt_for_confirmation(
            &mut line_editor,
            "Do you want to install the required version now? [y/n]: ",
        )?;
        if !confirm {
            anyhow::bail!("Version mismatch. Aborting as per user choice.");
        }

        let platform = platform_triple()?;
        log_info!(
            stdout,
            "Installing forklaunch CLI v{} for {}...",
            required_version,
            platform
        );

        let binary_path = download_binary(&required_version)?;

        log_ok!(
            stdout,
            "Installed forklaunch CLI v{} at {}",
            required_version,
            binary_path.display()
        );
        binary_path
    };

    log_info!(
        stdout,
        "Re-executing your command with forklaunch v{}...",
        required_version
    );

    // The pinned binary runs as the account this invocation resolved. One
    // that knows the keyring is simply told which; one that predates it only
    // ever reads `~/.forklaunch/token`, so that path is held on the account
    // until the command finishes.
    let selection = super::accounts::selection()?;
    let mut command = OsCommand::new(&binary_path);
    let handoff = if supports_accounts(&binary_path) {
        if selection.source != super::accounts::AccountSource::Default
            && let Some(name) = &selection.name
        {
            command.env(super::accounts::ACCOUNT_ENV, name);
        }
        command.args(args().skip(1));
        None
    } else {
        command.args(without_account_flag(args().skip(1)));
        command.env_remove(super::accounts::ACCOUNT_ENV);
        super::accounts::handoff(&super::accounts::forklaunch_dir()?, &selection)?
    };

    let mut child = command
        .current_dir(current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    let status = child.wait()?;
    // `exit` runs no destructors, and the hand-off must be released first.
    drop(handoff);

    match status.code() {
        Some(code) => {
            exit(code);
        }
        None => {
            log_warn!(
                stdout,
                "forklaunch v{} at {} was stopped by a signal. Please re-run your command.",
                required_version,
                binary_path.display()
            );
            Ok(VersionCheckOutcome::ReexecNotSupported)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(list: &[&str]) -> Vec<String> {
        without_account_flag(list.iter().map(|s| s.to_string()))
    }

    /// A pinned binary that predates the keyring rejects `--account`, so the
    /// flag (in either spelling) is dropped and nothing else is touched.
    #[test]
    fn the_account_flag_is_stripped_for_a_binary_that_would_reject_it() {
        assert_eq!(
            strip(&["--account", "pact", "release", "create", "--yes"]),
            ["release", "create", "--yes"]
        );
        assert_eq!(
            strip(&["deploy", "create", "--account=pact", "--yes"]),
            ["deploy", "create", "--yes"]
        );
        assert_eq!(
            strip(&["cloud-account", "link", "--account-id", "1"]),
            ["cloud-account", "link", "--account-id", "1"]
        );
    }
}
