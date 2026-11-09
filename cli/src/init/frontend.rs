//! CLI-owned frontend foundations. No network calls, Git writes, or edits to existing apps.
use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use convert_case::{Case, Casing};
use include_dir::Dir;
use serde_json::json;

use crate::{
    CliCommand,
    core::{command::command, rendered_template::TEMPLATES_DIR},
};

#[derive(Debug)]
pub(crate) struct FrontendCommand;
impl FrontendCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}
impl CliCommand for FrontendCommand {
    fn command(&self) -> Command {
        command(
            "frontend",
            "Add a web or mobile app without changing existing code",
        )
        .arg(
            Arg::new("target")
                .long("target")
                .value_parser(["web", "mobile", "both"])
                .required(true),
        )
        .arg(
            Arg::new("name")
                .long("name")
                .required(true)
                .help("The app name people will see"),
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .required(true)
                .help("Existing application root"),
        )
        .arg(
            Arg::new("assistant")
                .long("assistant")
                .action(ArgAction::SetTrue),
        )
        .arg(Arg::new("dryrun").long("dryrun").action(ArgAction::SetTrue))
    }
    fn handler(&self, args: &ArgMatches) -> Result<()> {
        let root = Path::new(args.get_one::<String>("base_path").unwrap());
        let target = args.get_one::<String>("target").unwrap();
        let name = args.get_one::<String>("name").unwrap();
        generate(
            root,
            target,
            name,
            args.get_flag("dryrun"),
            args.get_flag("assistant"),
        )?;
        println!(
            "{} {} frontend foundation. Install its packages before starting it.",
            if args.get_flag("dryrun") {
                "Would create"
            } else {
                "Created"
            },
            target
        );
        Ok(())
    }
}
fn absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => bail!(
            "{} already exists. Your existing files were kept.",
            path.display()
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn copy_template(dir: &Dir<'_>, base: &Path, output: &Path) -> Result<()> {
    for file in dir.files() {
        let path = output.join(file.path().strip_prefix(base)?);
        fs::create_dir_all(path.parent().context("Missing template parent")?)?;
        fs::write(path, file.contents())?;
    }
    for child in dir.dirs() {
        copy_template(child, base, output)?;
    }
    Ok(())
}
fn generate(root: &Path, target: &str, name: &str, dryrun: bool, assistant: bool) -> Result<()> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        bail!("Choose an app name between 1 and 80 characters, on one line.");
    }
    let slug = name.to_case(Case::Kebab);
    if slug.is_empty()
        || !slug
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        bail!("For now, choose a name using English letters, numbers, spaces or hyphens.");
    }
    let targets: &[&str] = match target {
        "web" => &["web"],
        "mobile" => &["mobile"],
        "both" => &["web", "mobile"],
        _ => bail!("Choose web, mobile or both."),
    };
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
        bail!("Choose an application root with an app name.");
    }
    let apps = root.join("apps");
    if let Ok(meta) = fs::symlink_metadata(&apps) {
        if !meta.is_dir() || meta.file_type().is_symlink() {
            bail!("The apps folder must be a regular folder inside this application.");
        }
    }
    for target in targets {
        absent(&apps.join(target))?;
    }
    if targets.contains(&"web") {
        absent(&apps.join("client"))?;
    }
    if dryrun {
        return Ok(());
    }
    // Stage both targets before exposing either, so template failures cannot leave a half-written app.
    fs::create_dir_all(&apps)?;
    let staging = tempfile::Builder::new()
        .prefix(".frontend-")
        .tempdir_in(&apps)?;
    for target in targets {
        let kind = if *target == "web" {
            "frontend"
        } else {
            "mobile"
        };
        let prefix = format!("project/{kind}");
        let template = TEMPLATES_DIR
            .get_dir(&prefix)
            .context("Frontend template is missing from this CLI")?;
        let out = staging.path().join(target);
        copy_template(template, Path::new(&prefix), &out)?;
        if !assistant {
            fs::remove_dir_all(out.join("src/assistant"))?;
        } else if *target == "web" {
            let entry = out.join("src/main.tsx");
            let source = fs::read_to_string(&entry)?
                .replace(
                    "import './styles.css';",
                    "import './styles.css';\nimport { Assistant } from './assistant/Assistant';",
                )
                .replace(
                    "</nav>",
                    "<NavLink to=\"/assistant\">Assistant</NavLink></nav>",
                )
                .replace(
                    "<Routes>",
                    "<Routes><Route path=\"/assistant\" element={<Assistant/>}/>",
                );
            fs::write(entry, source)?;
        } else {
            fs::write(
                out.join("app/assistant.tsx"),
                "export { Assistant as default } from '../src/assistant/Assistant';\n",
            )?;
            let entry = out.join("app/index.tsx");
            let source = fs::read_to_string(&entry)?.replace(
                "</View>",
                "<Link href=\"/assistant\" style={styles.link}>Assistant</Link></View>",
            );
            fs::write(entry, source)?;
        }

        let mut package: serde_json::Value =
            serde_json::from_slice(&fs::read(out.join("package.json"))?)?;
        package["name"] = json!(format!("{slug}-{target}"));
        package["forklaunchFrontend"] = json!({ "schemaVersion": 1, "starterVersion": 1, "cliVersion": env!("CARGO_PKG_VERSION"), "target": target, "assistant": assistant, "expoSdk": if *target == "mobile" { Some(58) } else { None } });
        fs::write(
            out.join("package.json"),
            serde_json::to_string_pretty(&package)? + "\n",
        )?;
        fs::create_dir_all(out.join("src"))?;
        let scheme = format!("fl-{slug}");
        fs::write(
            out.join("src/product.ts"),
            format!(
                "export const productName = {};\nexport const appScheme = {};\n",
                serde_json::to_string(name)?,
                serde_json::to_string(&scheme)?
            ),
        )?;
        if *target == "mobile" {
            let config = json!({ "expo": { "name": name, "slug": slug, "scheme": scheme, "version": "1.0.0", "plugins": ["expo-router", "expo-secure-store"], "experiments": { "typedRoutes": true } } });
            fs::write(
                out.join("app.json"),
                serde_json::to_string_pretty(&config)? + "\n",
            )?;
        }
    }
    let mut installed = Vec::new();
    for target in targets {
        let destination = apps.join(target);
        // Recheck immediately before publication. Never replace existing files.
        if let Err(error) = absent(&destination)
            .and_then(|_| fs::rename(staging.path().join(target), &destination).map_err(Into::into))
        {
            for previous in installed {
                fs::rename(apps.join(previous), staging.path().join(previous))?;
            }
            return Err(error);
        }
        installed.push(*target);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".forklaunch")).unwrap();
        fs::write(
            dir.path().join(".forklaunch/manifest.toml"),
            "app_name = 'test'\n",
        )
        .unwrap();
        dir
    }
    #[test]
    fn generates_both_and_keeps_existing_edits() {
        let dir = app();
        generate(dir.path(), "both", "Labwork Buddy", false, true).unwrap();
        assert!(dir.path().join("apps/web/src/main.tsx").is_file());
        assert!(dir.path().join("apps/mobile/app/account.tsx").is_file());
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join("apps/mobile/app.json")).unwrap())
                .unwrap();
        assert_eq!(config["expo"]["name"], "Labwork Buddy");
        fs::write(dir.path().join("apps/web/src/main.tsx"), "user edits").unwrap();
        assert!(generate(dir.path(), "both", "Other", false, false).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("apps/web/src/main.tsx")).unwrap(),
            "user edits"
        );
    }
    #[test]
    fn dry_run_and_legacy_frontends_are_preserved() {
        let dir = app();
        generate(dir.path(), "both", "Example", true, false).unwrap();
        assert!(!dir.path().join("apps").exists());
        fs::create_dir_all(dir.path().join("apps/client")).unwrap();
        assert!(generate(dir.path(), "both", "Example", false, false).is_err());
        assert!(!dir.path().join("apps/mobile").exists());
    }
    #[test]
    fn assistant_is_optional_and_product_name_is_escaped() {
        let dir = app();
        generate(dir.path(), "web", "My App", false, false).unwrap();
        assert!(!dir.path().join("apps/web/src/assistant").exists());
        assert!(
            fs::read_to_string(dir.path().join("apps/web/src/product.ts"))
                .unwrap()
                .contains("\"My App\"")
        );
        assert!(!dir.path().join("apps/mobile").exists());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_redirected_apps_folder() {
        let dir = app();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("apps")).unwrap();
        assert!(generate(dir.path(), "web", "Example", false, false).is_err());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}
