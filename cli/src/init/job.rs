//! CLI-owned manual Rust job foundation. Generation does not execute or schedule work.
use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::json;

use crate::{CliCommand, core::command::command};

#[derive(Debug)]
pub(crate) struct JobCommand;
impl JobCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}
impl CliCommand for JobCommand {
    fn command(&self) -> Command {
        command("job", "Add a bounded manual job to an existing application")
            .arg(
                Arg::new("name")
                    .long("name")
                    .required(true)
                    .help("Unique lowercase name, for example weekly-report"),
            )
            .arg(Arg::new("base_path").short('p').long("path").required(true))
            .arg(Arg::new("dryrun").long("dryrun").action(ArgAction::SetTrue))
    }
    fn handler(&self, args: &ArgMatches) -> Result<()> {
        generate(
            Path::new(args.get_one::<String>("base_path").unwrap()),
            args.get_one::<String>("name").unwrap(),
            args.get_flag("dryrun"),
        )?;
        println!(
            "{} manual Rust job foundation. It has not been run or scheduled.",
            if args.get_flag("dryrun") {
                "Would create"
            } else {
                "Created"
            }
        );
        Ok(())
    }
}
fn generate(root: &Path, name: &str, dryrun: bool) -> Result<()> {
    if name.is_empty()
        || name.len() > 63
        || !name.as_bytes()[0].is_ascii_lowercase()
        || !name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        bail!(
            "Use a name starting with a lowercase letter, with at most 63 lowercase letters, digits, or hyphens."
        );
    }
    let root = root
        .canonicalize()
        .context("The application folder could not be opened")?;
    let manifest: toml::Value = toml::from_str(
        &fs::read_to_string(root.join(".forklaunch/manifest.toml"))
            .context("Choose an existing ForkLaunch application")?,
    )?;
    if manifest
        .get("app_name")
        .and_then(toml::Value::as_str)
        .is_none()
    {
        bail!("Choose an application with a saved app name.");
    }
    let jobs = root.join("jobs");
    match fs::symlink_metadata(&jobs) {
        Ok(m) if !m.is_dir() || m.file_type().is_symlink() => {
            bail!("The jobs folder must be a regular folder inside the application.")
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    let destination = jobs.join(name);
    if fs::symlink_metadata(&destination).is_ok() {
        bail!("This job already exists. Its files were kept.");
    }
    if dryrun {
        return Ok(());
    }
    fs::create_dir_all(&jobs)?;
    // Reserve without replacement, including under simultaneous CLI invocations.
    fs::create_dir(&destination).context("This job could not be reserved; it may already exist")?;
    let write = (|| -> Result<()> {
        fs::create_dir(destination.join("src"))?;
        fs::write(
            destination.join("src/main.rs"),
            include_str!("../templates/project/job/src/main.rs"),
        )?;
        fs::write(
            destination.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n\n[dependencies]\nserde_json = \"1.0.151\"\n"
            ),
        )?;
        let definition = json!({"schemaVersion":1,"id":name,"language":"rust","trigger":{"kind":"manual"},"input":"json-stdin","output":"json-stdout","limits":{"timeoutSeconds":60,"maxInputBytes":1048576,"maxOutputBytes":1048576},"externalActions":"disabled","implementation":"sample"});
        fs::write(
            destination.join("job.json"),
            serde_json::to_string_pretty(&definition)? + "\n",
        )?;
        fs::write(
            destination.join("evals.json"),
            serde_json::to_string_pretty(
                &json!({"schemaVersion":1,"cases":[{"id":"word-count","input":{"text":"one two three"},"expected":{"wordCount":3}}]}),
            )? + "\n",
        )?;
        fs::write(
            destination.join("README.md"),
            "# Manual job foundation\n\nThis sample counts words; replace `execute` in `src/main.rs` with your assignment. It does not call AI, send data, or schedule anything. `evals.json` contains a sample behavior check.\n\nRun and test it inside your private workspace. The execution service must enforce `job.json` limits outside this program; the JSON file alone is not a security boundary. Keep credentials out of source and input. Review and approve any future external actions separately.\n",
        )?;
        Ok(())
    })();
    if write.is_err() {
        let _ = fs::remove_dir_all(&destination);
    }
    write
}
#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".forklaunch")).unwrap();
        fs::write(
            root.path().join(".forklaunch/manifest.toml"),
            "app_name = \"sample\"\n",
        )
        .unwrap();
        root
    }
    #[test]
    fn job_is_manual_and_existing_code_is_preserved() {
        let root = app();
        generate(root.path(), "weekly-report", false).unwrap();
        let definition: serde_json::Value = serde_json::from_slice(
            &fs::read(root.path().join("jobs/weekly-report/job.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(definition["trigger"]["kind"], "manual");
        assert_eq!(definition["externalActions"], "disabled");
        assert!(generate(root.path(), "weekly-report", false).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join(".forklaunch/manifest.toml")).unwrap(),
            "app_name = \"sample\"\n"
        );
    }
    #[test]
    fn dry_run_and_invalid_names_write_nothing() {
        let root = app();
        generate(root.path(), "weekly-report", true).unwrap();
        for name in ["../escape", "", "-name", "123", "hello/world"] {
            assert!(generate(root.path(), name, false).is_err());
        }
        assert!(!root.path().join("jobs").exists());
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_job_folder() {
        let root = app();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("jobs")).unwrap();
        assert!(generate(root.path(), "report", false).is_err());
        assert!(!outside.path().join("report").exists());
    }
}
