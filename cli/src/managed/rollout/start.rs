use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::json;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, post_json, print_dryrun, require_managed_mode, resolve_managed_auth},
        rollout::print_rollout,
        types::FleetRollout,
    },
};

#[derive(Debug)]
pub(super) struct StartCommand;

impl StartCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

/// Parses `--waves 10,50,100` into cumulative percentages and refuses the shapes the
/// control plane would reject, so a typo fails here instead of half-way through.
fn parse_waves(raw: &str) -> Result<Vec<u64>> {
    let mut waves = Vec::new();
    for part in raw.split(|c: char| c == ',' || c.is_whitespace()) {
        if part.is_empty() {
            continue;
        }
        let value: u64 = part
            .parse()
            .with_context(|| format!("'{}' is not a whole-number percentage", part))?;
        if value == 0 || value > 100 {
            bail!("wave percentages must be between 1 and 100 (got {})", value);
        }
        if let Some(previous) = waves.last() {
            if value <= *previous {
                bail!(
                    "waves are cumulative and must increase (got {} after {})",
                    value,
                    previous
                );
            }
        }
        waves.push(value);
    }
    if waves.is_empty() {
        bail!("--waves needs at least one percentage, for example 10,100");
    }
    if *waves.last().unwrap() != 100 {
        bail!("the last wave must be 100 (the whole fleet)");
    }
    Ok(waves)
}

impl CliCommand for StartCommand {
    fn command(&self) -> Command {
        command(
            "start",
            "Start rolling a published version out to a template's fleet",
        )
        .long_about(
            "Start rolling a published version out to a template's fleet.\n\n\
                 Every instance of --template whose update policy is auto is assigned to a\n\
                 wave; instances already on the target version are left alone. The default\n\
                 waves are 10,100: a tenth of the fleet as a canary, then everyone. Give\n\
                 --waves as cumulative percentages ending in 100, e.g. 10,50,100.\n\n\
                 The rollout advances on its own while each wave stays under --halt-above\n\
                 percent failures; above that it halts, and `rollout advance` resumes it.",
        )
        .arg(
            Arg::new("template")
                .long("template")
                .required(true)
                .help("Slug of the template whose fleet to update"),
        )
        .arg(
            Arg::new("version")
                .long("version")
                .required(true)
                .help("Published semver to roll out (see `template versions`)"),
        )
        .arg(
            Arg::new("waves")
                .long("waves")
                .help("Cumulative fleet coverage per wave, e.g. 10,50,100 (default 10,100)"),
        )
        .arg(
            Arg::new("halt_above")
                .long("halt-above")
                .value_parser(clap::value_parser!(u8).range(0..=100))
                .help("Halt when a wave's failures exceed this percent (default 10)"),
        )
        .arg(
            Arg::new("dryrun")
                .long("dryrun")
                .help("Print the request that would be sent without sending it")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let template = matches
            .get_one::<String>("template")
            .context("--template is required")?;
        let version = matches
            .get_one::<String>("version")
            .context("--version is required")?;

        let mut body = serde_json::Map::new();
        body.insert("templateSlug".into(), json!(template));
        body.insert("targetSemver".into(), json!(version));
        if let Some(raw) = matches.get_one::<String>("waves") {
            body.insert("wavePercents".into(), json!(parse_waves(raw)?));
        }
        if let Some(threshold) = matches.get_one::<u8>("halt_above") {
            body.insert("failureThresholdPercent".into(), json!(threshold));
        }
        let body = serde_json::Value::Object(body);

        if matches.get_flag("dryrun") {
            return print_dryrun("POST", "/rollouts", Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let rollout: FleetRollout = post_json(
            &auth_mode,
            "/rollouts",
            body,
            Missing::Resource(format!(
                "published version '{}' of template '{}'",
                version, template
            )),
        )?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&rollout)?);
            return Ok(());
        }

        log_ok!(
            stdout,
            "Rollout started: {} → {} in {} wave(s)",
            template,
            version,
            rollout.wave_percents.len()
        );
        print_rollout(&mut stdout, &rollout)?;
        if let Some(id) = rollout.id.as_deref() {
            log_info!(
                stdout,
                "Follow it with `forklaunch managed rollout get --id {}`; it advances on its own and halts above the failure threshold.",
                id
            );
        }
        writeln!(stdout)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_waves;

    #[test]
    fn default_shape_parses() {
        assert_eq!(parse_waves("10,100").unwrap(), vec![10, 100]);
        assert_eq!(parse_waves("10, 50 ,100").unwrap(), vec![10, 50, 100]);
    }

    #[test]
    fn waves_must_increase_and_end_at_the_whole_fleet() {
        assert!(parse_waves("50,10,100").is_err());
        assert!(parse_waves("10,50").is_err());
        assert!(parse_waves("0,100").is_err());
        assert!(parse_waves("").is_err());
        assert!(parse_waves("ten,100").is_err());
    }
}
