use std::{
    env::current_dir,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::json;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{
        accounts::{
            self, ACCOUNT_ENV, AccountInfo, AccountSource, Identity, forklaunch_dir,
            manifest_application,
        },
        base_path::find_nearest_manifest_from,
        command::command,
    },
};

// ── Top-level command ─────────────────────────────────────────────────────────
// The keyring of platform logins on this machine. Which one a command uses
// is decided per invocation (see `core::accounts`); these subcommands are how
// a person, or an agent, inspects and arranges that.

#[derive(Debug)]
pub(crate) struct AccountCommand;

impl AccountCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

fn json_flag() -> Arg {
    Arg::new("json")
        .long("json")
        .help("Output raw JSON instead of formatted terminal output")
        .action(ArgAction::SetTrue)
}

fn name_arg(help: &'static str) -> Arg {
    Arg::new("name")
        .required(true)
        .value_name("NAME")
        .help(help)
}

fn path_arg() -> Arg {
    Arg::new("app_path")
        .long("app-path")
        .value_name("DIR")
        .help("The application directory (default: the application the current directory is in)")
}

impl CliCommand for AccountCommand {
    fn command(&self) -> Command {
        command(
            "account",
            "Keep several platform logins on this machine and choose which one a command uses",
        )
        .after_help(
            "A command uses, in order: --account <name>, FORKLAUNCH_ACCOUNT, the account bound \
             to the application it runs in, then the default login.",
        )
        .subcommand(command("list", "List the logins on this machine").arg(json_flag()))
        .subcommand(
            command(
                "current",
                "Show which login a command run here would use, and why",
            )
            .arg(json_flag()),
        )
        .subcommand(
            command("use", "Make an account the default login")
                .arg(name_arg("The account to make the default"))
                .arg(
                    Arg::new("force")
                        .long("force")
                        .help("Discard a default login that was never saved into the keyring")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            command(
                "bind",
                "Always use an account for the application in this directory",
            )
            .arg(name_arg("The account this application should use"))
            .arg(path_arg()),
        )
        .subcommand(
            command(
                "unbind",
                "Stop using a specific account for the application in this directory",
            )
            .arg(path_arg()),
        )
        .subcommand(
            command(
                "save",
                "Save the current default login into the keyring under a name",
            )
            .arg(name_arg("The name to save the login under"))
            .arg(
                Arg::new("from")
                    .long("from")
                    .value_name("FILE")
                    .help("Import a token file kept by hand instead of the default login"),
            )
            .arg(
                Arg::new("force")
                    .long("force")
                    .help("Replace an existing account of the same name")
                    .action(ArgAction::SetTrue),
            ),
        )
        .subcommand(
            command("remove", "Forget an account and its stored login")
                .arg(name_arg("The account to forget")),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let dir = forklaunch_dir()?;
        match matches.subcommand() {
            Some(("current", sub)) => current(&dir, sub.get_flag("json")),
            Some(("use", sub)) => use_account(&dir, name_of(sub), sub.get_flag("force")),
            Some(("bind", sub)) => bind(&dir, name_of(sub), sub),
            Some(("unbind", sub)) => unbind(&dir, sub),
            Some(("save", sub)) => save(
                &dir,
                name_of(sub),
                sub.get_one::<String>("from").map(PathBuf::from),
                sub.get_flag("force"),
            ),
            Some(("remove", sub)) => remove(&dir, name_of(sub)),
            Some(("list", sub)) => list(&dir, sub.get_flag("json")),
            _ => list(&dir, false),
        }
    }
}

fn name_of(matches: &ArgMatches) -> &str {
    matches
        .get_one::<String>("name")
        .map(String::as_str)
        .unwrap_or_default()
}

/// The application a `bind`/`unbind` is about.
fn application_for(matches: &ArgMatches) -> Result<(String, Option<String>)> {
    let start = match matches.get_one::<String>("app_path") {
        Some(path) => PathBuf::from(path),
        None => current_dir()?,
    };
    let Some(application) =
        find_nearest_manifest_from(&start).and_then(|root| manifest_application(&root))
    else {
        bail!(
            "No forklaunch application found at {} (looked for .forklaunch/manifest.toml with an \
             `id`). Run this inside the application, or pass --app-path.",
            start.display()
        );
    };
    Ok(application)
}

fn session_label(identity: &Identity) -> String {
    let now = chrono::Utc::now().timestamp();
    let remaining = identity.expires_at.map(|at| at - now);
    let validity = match remaining {
        Some(secs) if secs <= 0 => "token expired".to_string(),
        Some(secs) if secs < 3600 => format!("token valid {}m", secs / 60),
        Some(secs) if secs < 86_400 => format!("token valid {}h", secs / 3600),
        Some(secs) => format!("token valid {}d", secs / 86_400),
        None => "no expiry recorded".to_string(),
    };
    match identity.kind {
        Some("api-key") => "API key, renews itself".to_string(),
        Some("session") => format!("browser session, {validity}"),
        Some(_) => format!("pasted token, {validity}, cannot renew"),
        None => "no login stored".to_string(),
    }
}

fn label(name: &Option<String>) -> &str {
    name.as_deref().unwrap_or("(unsaved)")
}

fn applications_label(account: &AccountInfo) -> String {
    account
        .applications
        .iter()
        .map(|app| app.name.clone().unwrap_or_else(|| app.id.clone()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn list(dir: &Path, as_json: bool) -> Result<()> {
    let accounts = accounts::list(dir)?;
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);

    if as_json {
        writeln!(stdout, "{}", serde_json::to_string_pretty(&accounts)?)?;
        return Ok(());
    }

    if accounts.is_empty() {
        writeln!(
            stdout,
            "No logins on this machine. Run `forklaunch login --account <name>` to add one."
        )?;
        return Ok(());
    }

    let width = |column: fn(&AccountInfo) -> String, header: &str| {
        accounts
            .iter()
            .map(|account| column(account).len())
            .max()
            .unwrap_or(0)
            .max(header.len())
    };
    let name_col = |a: &AccountInfo| label(&a.name).to_string();
    let email_col = |a: &AccountInfo| a.identity.email.clone().unwrap_or_else(|| "-".into());
    let org_col = |a: &AccountInfo| {
        a.identity
            .organization_id
            .clone()
            .unwrap_or_else(|| "-".into())
    };
    let session_col = |a: &AccountInfo| session_label(&a.identity);
    let (name_w, email_w, org_w, session_w) = (
        width(name_col, "ACCOUNT"),
        width(email_col, "EMAIL"),
        width(org_col, "ORGANIZATION"),
        width(session_col, "SESSION"),
    );

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(
        stdout,
        "  {:<name_w$}  {:<email_w$}  {:<org_w$}  {:<session_w$}  APPLICATIONS",
        "ACCOUNT", "EMAIL", "ORGANIZATION", "SESSION"
    )?;
    stdout.reset()?;
    for account in &accounts {
        if account.is_default {
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
        }
        write!(stdout, "{} ", if account.is_default { "*" } else { " " })?;
        write!(stdout, "{:<name_w$}", name_col(account))?;
        stdout.reset()?;
        writeln!(
            stdout,
            "  {:<email_w$}  {:<org_w$}  {:<session_w$}  {}",
            email_col(account),
            org_col(account),
            session_col(account),
            applications_label(account)
        )?;
    }
    writeln!(stdout)?;
    writeln!(stdout, "  * default login")?;
    if accounts.iter().any(|account| account.name.is_none()) {
        writeln!(
            stdout,
            "  The unsaved login is not in the keyring yet: `forklaunch account save <name>` keeps it."
        )?;
    }
    Ok(())
}

fn current(dir: &Path, as_json: bool) -> Result<()> {
    let selection = accounts::selection()?;
    let identity = accounts::identity(&selection.token_path);
    let logged_in = selection.token_path.exists();
    let application = current_dir()
        .ok()
        .and_then(|cwd| find_nearest_manifest_from(&cwd))
        .and_then(|root| manifest_application(&root));
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);

    if as_json {
        let value = json!({
            "account": selection.name,
            "source": selection.source.key(),
            "loggedIn": logged_in,
            "default": selection.name.is_some()
                && selection.name == accounts::default_account_name(dir)
                || selection.source == AccountSource::Default,
            "email": identity.email,
            "organizationId": identity.organization_id,
            "expiresAt": identity.expires_at,
            "kind": identity.kind,
            "application": application.as_ref().map(|(id, name)| json!({ "id": id, "name": name })),
        });
        writeln!(stdout, "{}", serde_json::to_string_pretty(&value)?)?;
        return Ok(());
    }

    let mut row = |key: &str, value: String| -> Result<()> {
        stdout.set_color(ColorSpec::new().set_bold(true))?;
        write!(stdout, "  {key:<14}")?;
        stdout.reset()?;
        writeln!(stdout, "{value}")?;
        Ok(())
    };
    writeln!(std::io::stdout())?;
    row(
        "Account:",
        format!(
            "{} ({})",
            label(&selection.name),
            selection.source.describe()
        ),
    )?;
    if !logged_in {
        row("Login:", "none stored".to_string())?;
        writeln!(std::io::stdout())?;
        writeln!(
            std::io::stdout(),
            "  {}",
            accounts::missing_login_message(&selection)
        )?;
        return Ok(());
    }
    row(
        "Email:",
        identity.email.clone().unwrap_or_else(|| "-".into()),
    )?;
    row(
        "Organization:",
        identity
            .organization_id
            .clone()
            .unwrap_or_else(|| "-".into()),
    )?;
    row("Session:", session_label(&identity))?;
    if let Some((id, name)) = &application {
        row(
            "Application:",
            format!("{} ({id})", name.as_deref().unwrap_or("-")),
        )?;
    }
    Ok(())
}

fn use_account(dir: &Path, name: &str, force: bool) -> Result<()> {
    let aside = accounts::set_default(dir, name, force)?;
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    log_ok!(stdout, "'{}' is now the default login.", name);
    if let Some(path) = aside {
        log_info!(
            stdout,
            "The previous, unsaved login was moved to {}. To keep it: `forklaunch account save <name> --from {}`.",
            path.display(),
            path.display()
        );
    }
    if let Ok(selected) = std::env::var(ACCOUNT_ENV)
        && !selected.is_empty()
        && selected != name
    {
        log_warn!(
            stdout,
            "{} is set to '{}' in this shell, and it takes precedence over the default.",
            ACCOUNT_ENV,
            selected
        );
    }
    Ok(())
}

fn bind(dir: &Path, name: &str, matches: &ArgMatches) -> Result<()> {
    let (app_id, app_name) = application_for(matches)?;
    let logged_in = accounts::bind(dir, &app_id, app_name.as_deref(), name)?;
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    log_ok!(
        stdout,
        "{} now uses account '{}' from every clone and worktree on this machine.",
        app_name.as_deref().unwrap_or(&app_id),
        name
    );
    if !logged_in {
        log_warn!(
            stdout,
            "Account '{}' has no login yet, so commands there will stop and ask for one: `forklaunch login --account {}`.",
            name,
            name
        );
    }
    Ok(())
}

fn unbind(dir: &Path, matches: &ArgMatches) -> Result<()> {
    let (app_id, app_name) = application_for(matches)?;
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    let application = app_name.as_deref().unwrap_or(&app_id);
    match accounts::unbind(dir, &app_id)? {
        Some(account) => log_ok!(
            stdout,
            "{} no longer uses account '{}'; it follows the default login.",
            application,
            account
        ),
        None => log_info!(stdout, "{} was not bound to an account.", application),
    }
    Ok(())
}

fn save(dir: &Path, name: &str, from: Option<PathBuf>, force: bool) -> Result<()> {
    accounts::save(dir, name, from.as_deref(), force)?;
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    match from {
        Some(path) => log_ok!(stdout, "Imported {} as account '{}'.", path.display(), name),
        None => log_ok!(
            stdout,
            "Saved the default login as account '{}'. It is still the default.",
            name
        ),
    }
    Ok(())
}

fn remove(dir: &Path, name: &str) -> Result<()> {
    let unbound = accounts::remove(dir, name)?;
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    log_ok!(stdout, "Removed account '{}' and its stored login.", name);
    if !unbound.is_empty() {
        let applications = unbound
            .iter()
            .map(|app| app.name.clone().unwrap_or_else(|| app.id.clone()))
            .collect::<Vec<_>>()
            .join(", ");
        log_info!(
            stdout,
            "These applications were bound to it and now follow the default login: {}",
            applications
        );
    }
    Ok(())
}
