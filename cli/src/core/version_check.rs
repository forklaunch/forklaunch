#[cfg(unix)]
use std::os::unix::{fs::symlink, prelude::PermissionsExt};
use std::{
    env::{args, current_dir},
    fs::{File, create_dir_all, metadata, read_to_string, remove_file, set_permissions},
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

    let dir = dirs::home_dir()
        .map(|p| p.join(".forklaunch").join("bin"))
        .context("Failed to resolve install directory")?;
    create_dir_all(&dir).ok();
    let binary_path = if platform.starts_with("windows") {
        dir.join("forklaunch.exe")
    } else {
        dir.join("forklaunch")
    };

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
    let mut file = File::create(&binary_path)?;
    copy(&mut resp, &mut file)?;
    #[cfg(unix)]
    {
        let mut perms = metadata(&binary_path)?.permissions();
        perms.set_mode(0o755);
        set_permissions(&binary_path, perms)?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let alias_path = dir.join("fl");
        if alias_path.exists() {
            let _ = remove_file(&alias_path);
        }
        #[cfg(unix)]
        symlink(&binary_path, &alias_path).ok();
    }

    Ok(binary_path)
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

    let manifest_root =
        find_nearest_manifest_from(&start_path).or_else(|| find_nearest_manifest_root_unbounded());
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
    log_warn!(stdout, "This project requires forklaunch CLI v{}, but you are running v{}.", required_version, current);

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
    log_info!(stdout, "Installing forklaunch CLI v{} for {}...", required_version, platform);

    let binary_path = download_binary(&required_version)?;

    log_ok!(stdout, "Installed forklaunch CLI v{} at {}", required_version, binary_path.display());

    log_info!(stdout, "Re-executing your command with forklaunch v{}...", required_version);

    let mut child = OsCommand::new(&binary_path)
        .args(args().skip(1))
        .current_dir(current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    let status = child.wait()?;

    match status.code() {
        Some(code) => {
            exit(code);
        }
        None => {
            log_warn!(stdout, "Installed forklaunch v{} at {}. Please re-run your command.", required_version, binary_path.display());
            Ok(VersionCheckOutcome::ReexecNotSupported)
        }
    }
}
