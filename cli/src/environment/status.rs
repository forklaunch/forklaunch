use std::{collections::HashMap, io::Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_platform_management_api_url,
    core::{
        ast::infrastructure::env::find_all_env_vars,
        command::command,
        env::{find_workspace_root, get_modules_path, is_env_var_defined},
        env_scope::{determine_env_var_scopes, is_pulumi_injected},
        http_client,
        manifest::application::ApplicationManifestData,
        rendered_template::RenderedTemplatesCache,
        validate::{require_integration, resolve_auth},
    },
};

/// Why a variable does or does not need a value from a person.
///
/// The order these are tested in is the order the deploy gate tests them, so a
/// variable reported here as needing nobody is one the gate also skips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Classification {
    /// The platform injects this at deploy time.
    PlatformManaged,
    /// A value is already present in a local .env file.
    Set,
    /// The application declares it `optional(...)`, so it may be left unset.
    DeclaredOptional,
    /// Nothing exempts it — a person has to supply this.
    NeedsValue,
}

impl Classification {
    /// Translate the platform's classification into the CLI's vocabulary.
    ///
    /// The platform distinguishes more reasons than the CLI can determine
    /// locally — an inter-service URL or a runtime-injected variable both mean
    /// "the platform supplies this", which is what `PlatformManaged` says here.
    fn from_platform(row: &PlatformStatus) -> Self {
        // A stored value settles it whatever the classification says. The
        // platform's `needsValue` describes the class of variable — whether one
        // of these ever requires a person — not whether this one is still
        // missing. Reading it alone reports a variable that is already set as
        // still needing someone.
        if row.state == "set" {
            return Self::Set;
        }

        match row.classification.as_str() {
            "NEEDS_VALUE" => Self::NeedsValue,
            "DECLARED_OPTIONAL" => Self::DeclaredOptional,
            _ => Self::PlatformManaged,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::PlatformManaged => "platform-managed",
            Self::Set => "set",
            Self::DeclaredOptional => "optional",
            Self::NeedsValue => "needs a value",
        }
    }

    /// The gate's question: does a person still have to supply this?
    pub(crate) fn needs_person(self) -> bool {
        matches!(self, Self::NeedsValue)
    }

    fn colour(self) -> Color {
        match self {
            Self::NeedsValue => Color::Yellow,
            Self::Set => Color::Green,
            _ => Color::Cyan,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct VariableStatus {
    pub key: String,
    pub project: String,
    pub classification: Classification,
    /// Declared optionality as the scanner read it. Absent when no sighting
    /// carried a declared type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
    pub needs_value: bool,
    /// True when the platform answered for this variable rather than the local
    /// scan guessing. Reported so a reader can tell the two apart.
    #[serde(default)]
    pub from_platform: bool,
    /// True when the platform resolved the value from a broader scope.
    #[serde(default)]
    pub inherited: bool,
}

/// Whether the report reflects platform state or only this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusSource {
    LocalOnly,
    Platform,
}

/// One row of the platform's answer.
///
/// The platform is authoritative: the deploy gate applies skip logic the CLI
/// does not share, and it knows values stored for an environment that never
/// appear on this machine. Where it has an opinion, it wins.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlatformStatus {
    pub key: String,
    pub state: String,
    pub classification: String,
    pub needs_value: bool,
    #[serde(default)]
    pub inherited: bool,
}

/// Ask the platform which variables still need a person for an environment.
///
/// Returns None when the application is not integrated or no environment was
/// named — the local scan is still useful on its own, and requiring a platform
/// round trip to run `env status` at all would make it useless offline.
fn fetch_platform_status(
    manifest: &ApplicationManifestData,
    environment: &str,
    region: &str,
) -> Result<Option<Vec<PlatformStatus>>> {
    let application_id = match require_integration(manifest) {
        Ok(id) => id,
        Err(_) => return Ok(None),
    };

    let auth_mode = resolve_auth()?;
    let url = format!(
        "{}/applications/{}/environments/{}/regions/{}/config/status",
        get_platform_management_api_url(),
        application_id,
        environment,
        region
    );

    let response = http_client::get_with_auth(&auth_mode, &url)
        .with_context(|| format!("Failed to reach the platform at {url}"))?;

    if !response.status().is_success() {
        bail!(
            "Platform returned {} for environment '{}' region '{}'",
            response.status(),
            environment,
            region
        );
    }

    Ok(Some(response.json::<Vec<PlatformStatus>>().with_context(
        || "Failed to parse the platform's status response",
    )?))
}

/// Decide what a variable's status is.
///
/// Kept separate from the handler so the rule can be tested without a
/// filesystem. The order mirrors the platform's: ownership first, then a stored
/// value, then declared optionality. Ownership leads because a variable the
/// platform injects is nobody's task whatever the application declared about
/// it — reporting it optional would imply someone could usefully set it.
pub(crate) fn classify(
    platform_injected: bool,
    has_value: bool,
    optional: Option<bool>,
) -> Classification {
    if platform_injected {
        Classification::PlatformManaged
    } else if has_value {
        Classification::Set
    } else if optional == Some(true) {
        Classification::DeclaredOptional
    } else {
        Classification::NeedsValue
    }
}

/// Which platform state to ask about, if any.
///
/// `--environment` and `--region` only mean something together: a variable is
/// stored for an environment *in* a region, so one flag without the other
/// names nothing the platform can answer for. Given just one, the earlier code
/// fell through to local state and carried on — reporting `set` from a local
/// file, and exiting zero, for a caller that believed it had checked
/// production. Partial input is refused rather than quietly downgraded, for the
/// same reason a failed platform lookup is warned about rather than swallowed:
/// this tool's exit code is a gate, and a gate must not open because the
/// question was malformed.
pub(crate) fn platform_selector<'a>(
    environment: Option<&'a String>,
    region: Option<&'a String>,
) -> Result<Option<(&'a str, &'a str)>> {
    match (environment, region) {
        (Some(env), Some(reg)) => Ok(Some((env.as_str(), reg.as_str()))),
        (None, None) => Ok(None),
        (Some(_), None) => bail!("--environment was given without --region; pass both to ask the platform, or neither for local state"),
        (None, Some(_)) => bail!("--region was given without --environment; pass both to ask the platform, or neither for local state"),
    }
}

#[derive(Debug)]
pub(crate) struct StatusCommand;

impl StatusCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for StatusCommand {
    fn command(&self) -> Command {
        command(
            "status",
            "Report which environment variables still need a value from a person",
        )
        .long_about(
            "Scans the workspace, classifies every environment variable it finds, and reports \
             which ones still need someone to supply a value.\n\n\
             Exits non-zero while anything needs input, so CI and agents can gate on it without \
             parsing the output.",
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("The application path to report status for"),
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .help("Environment to ask the platform about (e.g. production)"),
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .help("Region to ask the platform about (e.g. us-west-2)"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Emit machine-readable JSON instead of a table")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let json_output = matches.get_flag("json");

        let (app_root, manifest) = crate::core::validate::require_manifest(matches)?;
        let workspace_root = find_workspace_root(&app_root)?;
        let modules_path = get_modules_path(&workspace_root)?;

        let cache = RenderedTemplatesCache::new();
        let project_env_vars = find_all_env_vars(&modules_path, &cache)?;

        // Scoped vars are the right unit: optionality is already folded across
        // every project that uses a name, so a variable required anywhere is
        // reported required here.
        let scoped = determine_env_var_scopes(&project_env_vars, &manifest)?;

        // Every project the manifest declares, not just those the scan found
        // variables in. A project with no environment variables of its own is
        // absent from `project_env_vars`, and taking names from there would
        // stop another service's URL for it being recognised as
        // platform-injected — reporting it as needing a person, and exiting
        // non-zero, for a value the platform supplies.
        let project_names: Vec<String> = manifest
            .projects
            .iter()
            .map(|project| project.name.clone())
            .collect();

        let mut statuses: Vec<VariableStatus> = Vec::new();

        for var in &scoped {
            let owning_project = var
                .used_by
                .first()
                .cloned()
                .unwrap_or_else(|| "application".to_string());
            let project_path = modules_path.join(&owning_project);

            // Precedence mirrors the platform's: ownership first, then a stored
            // value, then declared optionality. A variable the platform injects
            // is nobody's task whatever the code declared about it.
            // An unreadable or malformed local env file is not the same as an
            // unset variable: swallowing the error reports `needs a value` for
            // something that may well be set, and hides the real fault.
            let stored_locally = match var.value.is_some() {
                true => true,
                false => is_env_var_defined(&project_path, &var.name)?,
            };

            let classification = classify(
                is_pulumi_injected(&var.name, &project_names),
                stored_locally,
                var.optional,
            );

            statuses.push(VariableStatus {
                key: var.name.clone(),
                project: owning_project,
                classification,
                optional: var.optional,
                needs_value: classification.needs_person(),
                from_platform: false,
                inherited: false,
            });
        }

        // Ask the platform, when we have somewhere to ask about. Its answer
        // replaces the local guess for any variable it knows: a value stored for
        // this environment never appears on this machine, and a local .env entry
        // says nothing about whether production has one. Reporting `set` from a
        // local file alone is how this tool would tell an agent an application is
        // ready to deploy when it is not.
        let environment = matches.get_one::<String>("environment");
        let region = matches.get_one::<String>("region");

        let platform = match platform_selector(environment, region)? {
            Some((env, reg)) => {
                match fetch_platform_status(&manifest, env, reg) {
                    Ok(rows) => rows,
                    Err(error) => {
                        // A failed lookup must not masquerade as "nothing needed".
                        log_warn!(
                            stdout,
                            "Could not read platform state ({error}); reporting local state only"
                        );
                        None
                    }
                }
            }
            None => None,
        };

        let mut source = StatusSource::LocalOnly;
        if let Some(rows) = platform {
            source = StatusSource::Platform;
            let authoritative: HashMap<String, PlatformStatus> =
                rows.into_iter().map(|r| (r.key.clone(), r)).collect();

            for status in statuses.iter_mut() {
                if let Some(row) = authoritative.get(&status.key) {
                    status.classification = Classification::from_platform(row);
                    // Outstanding means the class requires a person AND nothing
                    // usable is stored — the same combination the platform's own
                    // outstanding list applies.
                    status.needs_value =
                        row.needs_value && matches!(row.state.as_str(), "absent" | "blank");
                    status.inherited = row.inherited;
                    status.from_platform = true;
                }
            }
        }

        statuses.sort_by(|a, b| (&a.project, &a.key).cmp(&(&b.project, &b.key)));
        let outstanding = statuses.iter().filter(|s| s.needs_value).count();

        if json_output {
            writeln!(stdout, "{}", serde_json::to_string_pretty(&statuses)?)?;
        } else {
            render_table(&mut stdout, &statuses, source)?;
        }

        // Non-zero while anything needs input, so CI and agents can gate on the
        // exit code rather than parsing output.
        if outstanding > 0 {
            std::process::exit(1);
        }

        Ok(())
    }
}

fn render_table(
    stdout: &mut StandardStream,
    statuses: &[VariableStatus],
    source: StatusSource,
) -> Result<()> {
    if statuses.is_empty() {
        writeln!(stdout, "No environment variables found.")?;
        return Ok(());
    }

    let width = statuses
        .iter()
        .map(|s| s.key.len())
        .max()
        .unwrap_or(3)
        .max(3);

    writeln!(stdout, "\n{:<width$}  STATUS", "KEY", width = width)?;

    for status in statuses {
        write!(stdout, "{:<width$}  ", status.key, width = width)?;
        stdout
            .set_color(termcolor::ColorSpec::new().set_fg(Some(status.classification.colour())))?;
        write!(stdout, "{}", status.classification.label())?;
        stdout.reset()?;
        writeln!(stdout, "   ({})", status.project)?;
    }

    let outstanding = statuses.iter().filter(|s| s.needs_value).count();
    writeln!(
        stdout,
        "\n{} variables · {} need a value",
        statuses.len(),
        outstanding
    )?;

    // Say which state this reflects. Without it a reader cannot tell an answer
    // about their deployed environment from a guess made off local files, and
    // the two disagree precisely when it matters.
    match source {
        StatusSource::Platform => writeln!(
            stdout,
            "Reflecting platform state for the requested environment."
        )?,
        StatusSource::LocalOnly => writeln!(
            stdout,
            "Local state only — pass --environment and --region to ask the platform."
        )?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_without_env_vars_still_owns_its_url() {
        // The scan only yields projects that have environment-variable
        // sightings, so a project declaring none is absent from its keys.
        // Taking project names from there rather than from the manifest makes
        // that project's inter-service URL unrecognisable, and the command
        // reports a platform-supplied value as needing a person -- and exits
        // non-zero on it.
        let from_manifest = vec![
            "billing".to_string(),
            "monitoring".to_string(),
            "iam".to_string(),
        ];
        // `monitoring` declares no variables of its own, so a scan-derived
        // list would omit it.
        let from_scan = vec!["billing".to_string(), "iam".to_string()];

        assert!(
            is_pulumi_injected("MONITORING_URL", &from_manifest),
            "a manifest project's URL must be recognised as platform-injected"
        );
        assert!(
            !is_pulumi_injected("MONITORING_URL", &from_scan),
            "guard-the-guard: this is precisely what the scan-derived list \
             misses, so the assertion above is meaningful"
        );

        // And the classification that follows from each.
        assert_eq!(
            classify(is_pulumi_injected("MONITORING_URL", &from_manifest), false, None),
            Classification::PlatformManaged
        );
        assert_eq!(
            classify(is_pulumi_injected("MONITORING_URL", &from_scan), false, None),
            Classification::NeedsValue
        );
    }

    #[test]
    fn both_selector_flags_ask_the_platform() {
        let env = "production".to_string();
        let reg = "us-east-1".to_string();
        assert_eq!(
            platform_selector(Some(&env), Some(&reg)).unwrap(),
            Some(("production", "us-east-1"))
        );
    }

    #[test]
    fn neither_selector_flag_means_local_state() {
        assert_eq!(platform_selector(None, None).unwrap(), None);
    }

    #[test]
    fn a_partial_selector_is_refused_rather_than_downgraded() {
        // One flag without the other used to fall through to local state and
        // exit zero — a gate opening because the question was malformed. Both
        // partial shapes must error, and the error must name the missing flag.
        let env = "production".to_string();
        let reg = "us-east-1".to_string();

        let missing_region = platform_selector(Some(&env), None).unwrap_err();
        assert!(missing_region.to_string().contains("--region"));

        let missing_env = platform_selector(None, Some(&reg)).unwrap_err();
        assert!(missing_env.to_string().contains("--environment"));
    }

    #[test]
    fn platform_ownership_wins_over_everything() {
        // A variable the platform injects is nobody's task, whatever the
        // application declared or whether a local value happens to exist.
        for has_value in [true, false] {
            for optional in [None, Some(true), Some(false)] {
                assert_eq!(
                    classify(true, has_value, optional),
                    Classification::PlatformManaged
                );
            }
        }
    }

    #[test]
    fn a_stored_value_settles_it() {
        assert_eq!(classify(false, true, None), Classification::Set);
        assert_eq!(classify(false, true, Some(false)), Classification::Set);
    }

    #[test]
    fn declared_optional_needs_nobody_when_unset() {
        assert_eq!(
            classify(false, false, Some(true)),
            Classification::DeclaredOptional
        );
    }

    #[test]
    fn undeclared_and_unset_needs_a_person() {
        // Absent optionality means no sighting carried a declared type, which
        // this design refuses to read as optional.
        assert_eq!(classify(false, false, None), Classification::NeedsValue);
        assert_eq!(
            classify(false, false, Some(false)),
            Classification::NeedsValue
        );
    }

    fn platform_row(state: &str, classification: &str, needs_value: bool) -> PlatformStatus {
        PlatformStatus {
            key: "K".to_string(),
            state: state.to_string(),
            classification: classification.to_string(),
            needs_value,
            inherited: false,
        }
    }

    #[test]
    fn a_stored_platform_value_settles_it_whatever_the_class_says() {
        // The platform's `needsValue` describes the class of variable — whether
        // one of these ever requires a person — not whether this one is still
        // missing. Reading it alone reported a variable already set on the
        // platform as still needing someone.
        let row = platform_row("set", "NEEDS_VALUE", true);

        assert_eq!(Classification::from_platform(&row), Classification::Set);
    }

    #[test]
    fn platform_classifications_map_onto_the_cli_vocabulary() {
        assert_eq!(
            Classification::from_platform(&platform_row("absent", "NEEDS_VALUE", true)),
            Classification::NeedsValue
        );
        assert_eq!(
            Classification::from_platform(&platform_row("absent", "DECLARED_OPTIONAL", false)),
            Classification::DeclaredOptional
        );
        // The platform distinguishes reasons the CLI cannot determine locally.
        // All of them mean the same thing here: the platform supplies it.
        for reason in [
            "PLATFORM_MANAGED",
            "INTER_SERVICE_URL",
            "RUNTIME_INJECTED",
            "TEST_ONLY",
        ] {
            assert_eq!(
                Classification::from_platform(&platform_row("absent", reason, false)),
                Classification::PlatformManaged
            );
        }
    }

    #[test]
    fn a_blank_platform_value_still_needs_a_person() {
        // Blank is not set. A variable stored as an empty string is exactly the
        // case that would silently drop off an agent's list.
        let row = platform_row("blank", "NEEDS_VALUE", true);

        assert_eq!(
            Classification::from_platform(&row),
            Classification::NeedsValue
        );
    }

    #[test]
    fn only_needs_value_is_reported_as_needing_a_person() {
        // The exit code and the agent's work list both key off this, so it is
        // the one mapping that must not drift.
        assert!(!Classification::PlatformManaged.needs_person());
        assert!(!Classification::Set.needs_person());
        assert!(!Classification::DeclaredOptional.needs_person());
        assert!(Classification::NeedsValue.needs_person());
    }
}
