//! The account keyring: several platform logins on one machine.
//!
//! One login in `~/.forklaunch/token` stops working the moment a person (or
//! an agent acting for them) operates applications in more than one
//! organization: every release or deploy against the "other" organization
//! answers "Application not found" until someone logs in again by hand.
//!
//! The keyring keeps one token file per named account under
//! `~/.forklaunch/accounts/<name>.toml`, in exactly the format the single
//! login always used, and picks one per invocation:
//!
//! 1. `--account <name>`
//! 2. `FORKLAUNCH_ACCOUNT=<name>`
//! 3. the account bound to the application the command runs in
//!    (`forklaunch account bind`, keyed by the manifest's application id, so
//!    every clone and worktree of the application resolves the same way)
//! 4. the default login
//!
//! Nothing above the default is process-global, so two agents can release
//! for two organizations at the same time without stepping on each other.
//!
//! Older CLIs know none of this and read `~/.forklaunch/token`, and a
//! project that pins one (`cli_version`) is re-executed with it. So on unix
//! that path is kept as a symlink to the default account's file, and while
//! a pinned older CLI runs for some other account the symlink is pointed at
//! that account under a lock (`handoff`). This CLI never reads the symlink:
//! the default is recorded in `accounts.toml`.

use std::{
    collections::{BTreeMap, BTreeSet},
    env::var,
    fs::{self, read_to_string},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{Context, Result, bail};
use clap::ArgMatches;
use serde::{Deserialize, Serialize};

use super::version_check::invocation_manifest_root;

pub(crate) const ACCOUNT_ENV: &str = "FORKLAUNCH_ACCOUNT";
pub(crate) const ACCOUNT_FLAG: &str = "account";

/// Why this invocation uses the account it uses. Shown by `account current`,
/// because "which login did that just run as, and why" is the question the
/// keyring exists to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AccountSource {
    Flag,
    Env,
    AppBinding {
        app_id: String,
        app_name: Option<String>,
    },
    Default,
}

impl AccountSource {
    pub(crate) fn key(&self) -> &'static str {
        match self {
            AccountSource::Flag => "flag",
            AccountSource::Env => "env",
            AccountSource::AppBinding { .. } => "app-binding",
            AccountSource::Default => "default",
        }
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            AccountSource::Flag => "--account".to_string(),
            AccountSource::Env => ACCOUNT_ENV.to_string(),
            AccountSource::AppBinding { app_id, app_name } => format!(
                "bound to application {}",
                app_name.as_deref().unwrap_or(app_id)
            ),
            AccountSource::Default => "default".to_string(),
        }
    }
}

/// The login an invocation resolved to. `name` is `None` only for a default
/// login that was never saved into the keyring.
#[derive(Debug, Clone)]
pub(crate) struct Selection {
    pub(crate) name: Option<String>,
    pub(crate) source: AccountSource,
    pub(crate) token_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppBinding {
    account: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    app_name: Option<String>,
}

/// `~/.forklaunch/accounts.toml`. Holds no credentials, only which account
/// is the default and which account each application uses.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default: Option<String>,
    #[serde(default)]
    apps: BTreeMap<String, AppBinding>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AppRef {
    pub(crate) id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
}

/// What can be said about a stored login without calling the platform.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Identity {
    pub(crate) email: Option<String>,
    #[serde(rename = "organizationId")]
    pub(crate) organization_id: Option<String>,
    #[serde(rename = "expiresAt")]
    pub(crate) expires_at: Option<i64>,
    /// `api-key` renews itself, `session` renews until the browser session
    /// ends, `token` is a pasted JWT that cannot renew.
    pub(crate) kind: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AccountInfo {
    pub(crate) name: Option<String>,
    #[serde(rename = "default")]
    pub(crate) is_default: bool,
    #[serde(rename = "loggedIn")]
    pub(crate) logged_in: bool,
    #[serde(flatten)]
    pub(crate) identity: Identity,
    pub(crate) applications: Vec<AppRef>,
}

pub(crate) fn forklaunch_dir() -> Result<PathBuf> {
    Ok(Path::new(&var("HOME")?).join(".forklaunch"))
}

fn accounts_dir(dir: &Path) -> PathBuf {
    dir.join("accounts")
}

pub(crate) fn account_path(dir: &Path, name: &str) -> PathBuf {
    accounts_dir(dir).join(format!("{name}.toml"))
}

fn legacy_token_path(dir: &Path) -> PathBuf {
    dir.join("token")
}

fn index_path(dir: &Path) -> PathBuf {
    dir.join("accounts.toml")
}

/// An account name becomes a file name and an environment variable value, so
/// it is held to what is safe as both.
pub(crate) fn validate_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['.', '-'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '@'));
    if !ok {
        bail!(
            "'{name}' is not a usable account name. Use letters, digits and . _ - + @ \
             (at most 64 characters, not starting with . or -)."
        );
    }
    Ok(())
}

fn read_index(dir: &Path) -> Result<Index> {
    let path = index_path(dir);
    if !path.exists() {
        return Ok(Index::default());
    }
    let content = read_to_string(&path)?;
    toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))
}

fn write_index(dir: &Path, index: &Index) -> Result<()> {
    create_private_dir(dir)?;
    let path = index_path(dir);
    let tmp = dir.join(format!(".accounts.toml.{}", std::process::id()));
    fs::write(&tmp, toml::to_string(index)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn create_private_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

fn write_private_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    #[cfg(unix)]
    {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(content.as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        fs::write(path, content)?;
    }
    Ok(())
}

/// Is `path` a real file (as opposed to missing, or the default symlink)?
fn is_regular_file(path: &Path) -> bool {
    path.symlink_metadata()
        .map(|m| m.file_type().is_file())
        .unwrap_or(false)
}

/// The account that is the default login, if the default is a keyring
/// account at all.
pub(crate) fn default_account_name(dir: &Path) -> Option<String> {
    read_index(dir).ok()?.default
}

/// The default login's file: the default account's own file, or the single
/// login of a machine that has no default account.
fn default_token_path(dir: &Path) -> PathBuf {
    match default_account_name(dir) {
        Some(name) => account_path(dir, &name),
        None => legacy_token_path(dir),
    }
}

/// The account `~/.forklaunch/token` points at, when it is a pointer into
/// the keyring and not a login of its own.
#[cfg(unix)]
fn pointer_target(dir: &Path) -> Option<String> {
    let target = fs::read_link(legacy_token_path(dir)).ok()?;
    let target = if target.is_absolute() {
        target
    } else {
        dir.join(target)
    };
    if target.parent()? != accounts_dir(dir) || target.extension()? != "toml" {
        return None;
    }
    Some(target.file_stem()?.to_str()?.to_string())
}

/// Point `~/.forklaunch/token` at an account. Symlink beside the target and
/// rename over it, so an older CLI never finds the path missing.
#[cfg(unix)]
fn point_legacy_at(dir: &Path, name: &str) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let tmp = dir.join(format!(
        ".token.{}.{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::os::unix::fs::symlink(Path::new("accounts").join(format!("{name}.toml")), &tmp)?;
    fs::rename(&tmp, legacy_token_path(dir))?;
    Ok(())
}

#[cfg(unix)]
fn open_lock(dir: &Path) -> Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    create_private_dir(dir)?;
    Ok(fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(dir.join("handoff.lock"))?)
}

/// Set a login found at `~/.forklaunch/token` aside so the pointer can take
/// its place. It can only have been written by an older CLI used directly,
/// and it is kept because it may be the only copy of that login.
#[cfg(unix)]
fn set_aside(dir: &Path) -> Result<PathBuf> {
    let aside = dir.join(format!(
        "token.orphan-{}",
        chrono::Utc::now().timestamp_millis()
    ));
    fs::rename(legacy_token_path(dir), &aside)?;
    Ok(aside)
}

/// Bring `~/.forklaunch/token` back in line with the default account, unless
/// a pinned older CLI is running and owns the pointer (it restores it when
/// it finishes). Returns the path of a login that had to be set aside.
#[cfg(unix)]
fn sync_pointer(dir: &Path, replace_login: bool) -> Result<Option<PathBuf>> {
    let default = default_account_name(dir);
    let pointer = pointer_target(dir);
    let legacy = legacy_token_path(dir);
    let in_sync = match &default {
        Some(name) => pointer.as_deref() == Some(name.as_str()),
        None => pointer.is_none(),
    };
    if in_sync {
        return Ok(None);
    }
    let lock = open_lock(dir)?;
    if lock.try_lock().is_err() {
        return Ok(None);
    }
    let mut aside = None;
    match default {
        Some(name) => {
            if is_regular_file(&legacy) && !replace_login {
                aside = Some(set_aside(dir)?);
            }
            point_legacy_at(dir, &name)?;
        }
        None => fs::remove_file(&legacy)?,
    }
    let _ = lock.unlock();
    Ok(aside)
}

pub(crate) fn account_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(accounts_dir(dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != "toml" {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_string())
        })
        .collect();
    names.sort();
    names
}

/// Pick the login for one invocation. A named account that has no stored
/// login is still a valid selection: `login` needs to be able to create it,
/// and every other command reports it when it reaches for the token.
pub(crate) fn resolve(
    dir: &Path,
    flag: Option<&str>,
    env: Option<&str>,
    app: Option<(&str, Option<&str>)>,
) -> Result<Selection> {
    let explicit = flag
        .map(|name| (name, AccountSource::Flag))
        .or_else(|| env.map(|name| (name, AccountSource::Env)));
    if let Some((name, source)) = explicit {
        validate_name(name)?;
        return Ok(Selection {
            name: Some(name.to_string()),
            source,
            token_path: account_path(dir, name),
        });
    }

    if let Some((app_id, app_name)) = app
        && let Some(binding) = read_index(dir)?.apps.get(app_id)
    {
        return Ok(Selection {
            name: Some(binding.account.clone()),
            source: AccountSource::AppBinding {
                app_id: app_id.to_string(),
                app_name: app_name.map(str::to_string).or(binding.app_name.clone()),
            },
            token_path: account_path(dir, &binding.account),
        });
    }

    Ok(Selection {
        name: default_account_name(dir),
        source: AccountSource::Default,
        token_path: default_token_path(dir),
    })
}

/// Make `name` the default login.
///
/// A default that was never saved into the keyring would stop being used by
/// the switch, so that case is refused unless `force` says so; even then the
/// login is set aside, not deleted, and its path is returned.
pub(crate) fn set_default(dir: &Path, name: &str, force: bool) -> Result<Option<PathBuf>> {
    if !account_path(dir, name).exists() {
        bail!("{}", no_such_account(dir, name));
    }
    let mut index = read_index(dir)?;
    if index.default.is_none() && is_regular_file(&legacy_token_path(dir)) && !force {
        bail!(
            "The current default login is not saved in the keyring, and switching would \
             stop using it. Keep it with `forklaunch account save <name>`, or pass --force \
             to set it aside."
        );
    }
    index.default = Some(name.to_string());
    write_index(dir, &index)?;

    #[cfg(unix)]
    {
        sync_pointer(dir, false)
    }
    #[cfg(not(unix))]
    {
        // No symlinks to lean on: give older CLIs a copy of the default.
        let _ = fs::copy(account_path(dir, name), legacy_token_path(dir));
        Ok(None)
    }
}

#[derive(Deserialize)]
struct StoredToken {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_at: Option<i64>,
}

/// Save a login into the keyring under `name`: the unsaved default login, or
/// a token file kept by hand (`from`). Saving the default makes the new
/// account the default, so nothing changes about which login is in use.
pub(crate) fn save(dir: &Path, name: &str, from: Option<&Path>, force: bool) -> Result<()> {
    validate_name(name)?;
    let dest = account_path(dir, name);
    if dest.exists() && !force {
        bail!("An account named '{name}' already exists. Pass --force to replace it.");
    }

    let legacy = legacy_token_path(dir);
    let source = match from {
        Some(path) => path.to_path_buf(),
        None => {
            if let Some(current) = default_account_name(dir) {
                bail!("The default login is already saved, as account '{current}'.");
            }
            if !is_regular_file(&legacy) {
                bail!(
                    "There is no login to save. Run `forklaunch login --account {name}` instead."
                );
            }
            legacy.clone()
        }
    };

    let content =
        read_to_string(&source).with_context(|| format!("Failed to read {}", source.display()))?;
    toml::from_str::<StoredToken>(&content)
        .with_context(|| format!("{} is not a forklaunch login file", source.display()))?;
    write_private_file(&dest, &content)?;

    if from.is_none() {
        let mut index = read_index(dir)?;
        index.default = Some(name.to_string());
        write_index(dir, &index)?;
        // The copy is in the keyring, so replacing the original with the
        // pointer discards nothing.
        #[cfg(unix)]
        sync_pointer(dir, true)?;
    }
    Ok(())
}

/// Forget an account: its login and every application bound to it. Returns
/// the applications that were unbound.
///
/// The default cannot be forgotten while other accounts remain, because
/// there is no right answer to which of them should take its place.
pub(crate) fn remove(dir: &Path, name: &str) -> Result<Vec<AppRef>> {
    let path = account_path(dir, name);
    let mut index = read_index(dir)?;
    let is_default = index.default.as_deref() == Some(name);
    let is_bound = index.apps.values().any(|binding| binding.account == name);
    if !path.exists() && !is_default && !is_bound {
        bail!("{}", no_such_account(dir, name));
    }
    if is_default && account_names(dir).iter().any(|other| other != name) {
        bail!(
            "'{name}' is the default login. Make another account the default first with \
             `forklaunch account use <name>`."
        );
    }
    if path.exists() {
        fs::remove_file(&path)?;
    }
    if is_default {
        index.default = None;
    }
    let unbound: Vec<AppRef> = index
        .apps
        .iter()
        .filter(|(_, binding)| binding.account == name)
        .map(|(id, binding)| AppRef {
            id: id.clone(),
            name: binding.app_name.clone(),
        })
        .collect();
    index.apps.retain(|_, binding| binding.account != name);
    write_index(dir, &index)?;
    #[cfg(unix)]
    sync_pointer(dir, false)?;
    Ok(unbound)
}

/// Drop the stored login of the selected account. Its name, its place as
/// the default and its application bindings all stay, so the next command
/// says which login is missing instead of quietly using a different one.
pub(crate) fn logout(dir: &Path, selection: &Selection) -> Result<()> {
    let target = match &selection.name {
        Some(name) => account_path(dir, name),
        None => selection.token_path.clone(),
    };
    fs::remove_file(&target)?;
    Ok(())
}

/// Bind an application to an account. The account does not need a login
/// yet: binding first means a command in that application can only ever
/// fail for want of the right login, never succeed as the wrong one.
/// Returns whether the account has a login stored.
pub(crate) fn bind(dir: &Path, app_id: &str, app_name: Option<&str>, name: &str) -> Result<bool> {
    validate_name(name)?;
    let mut index = read_index(dir)?;
    index.apps.insert(
        app_id.to_string(),
        AppBinding {
            account: name.to_string(),
            app_name: app_name.map(str::to_string),
        },
    );
    write_index(dir, &index)?;
    Ok(account_path(dir, name).exists())
}

/// Returns the account the application was bound to, if it was bound.
pub(crate) fn unbind(dir: &Path, app_id: &str) -> Result<Option<String>> {
    let mut index = read_index(dir)?;
    let removed = index.apps.remove(app_id).map(|binding| binding.account);
    if removed.is_some() {
        write_index(dir, &index)?;
    }
    Ok(removed)
}

/// Read who a stored login is from the token itself. Unverified, and only
/// ever used to label an account; the platform still decides what it may do.
pub(crate) fn identity(token_path: &Path) -> Identity {
    let Ok(content) = read_to_string(token_path) else {
        return Identity::default();
    };
    let Ok(stored) = toml::from_str::<StoredToken>(&content) else {
        return Identity::default();
    };
    let claims = jwt_claims(&stored.access_token);
    let claim = |key: &str| {
        claims
            .as_ref()
            .and_then(|c| c.get(key))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    Identity {
        email: claim("email"),
        organization_id: claim("organizationId"),
        expires_at: stored.expires_at.filter(|at| *at != i64::MAX),
        kind: Some(
            if stored
                .refresh_token
                .starts_with(super::token::API_KEY_PREFIX)
            {
                "api-key"
            } else if stored.refresh_token.is_empty() {
                "token"
            } else {
                "session"
            },
        ),
    }
}

fn jwt_claims(token: &str) -> Option<serde_json::Value> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Every login on this machine: the keyring's accounts (including ones that
/// are named as the default or by a binding but have no login stored), then
/// the default login if it was never saved into the keyring.
pub(crate) fn list(dir: &Path) -> Result<Vec<AccountInfo>> {
    let index = read_index(dir)?;
    let mut names: BTreeSet<String> = account_names(dir).into_iter().collect();
    names.extend(index.default.clone());
    names.extend(index.apps.values().map(|binding| binding.account.clone()));

    let mut accounts: Vec<AccountInfo> = names
        .into_iter()
        .map(|name| {
            let path = account_path(dir, &name);
            AccountInfo {
                is_default: index.default.as_deref() == Some(name.as_str()),
                logged_in: path.exists(),
                identity: identity(&path),
                applications: index
                    .apps
                    .iter()
                    .filter(|(_, binding)| binding.account == name)
                    .map(|(id, binding)| AppRef {
                        id: id.clone(),
                        name: binding.app_name.clone(),
                    })
                    .collect(),
                name: Some(name),
            }
        })
        .collect();

    let legacy = legacy_token_path(dir);
    if index.default.is_none() && is_regular_file(&legacy) {
        accounts.push(AccountInfo {
            name: None,
            is_default: true,
            logged_in: true,
            identity: identity(&legacy),
            applications: Vec::new(),
        });
    }
    Ok(accounts)
}

/// `~/.forklaunch/token` held on one account for as long as a pinned older
/// CLI runs. Dropping it hands the pointer back to the default.
#[derive(Debug)]
pub(crate) struct Handoff {
    #[cfg(unix)]
    lock: fs::File,
    #[cfg(unix)]
    dir: PathBuf,
}

impl Drop for Handoff {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = self.lock.unlock();
            let _ = sync_pointer(&self.dir, false);
        }
    }
}

/// Let a CLI that predates the keyring run as the selected account.
///
/// Such a CLI reads `~/.forklaunch/token` and nothing else, so that path is
/// pointed at the account for the duration. Commands for the same account
/// share the pointer; a command for a different account waits its turn.
/// That wait is the price of an old pin, and it is the only place the
/// keyring serializes anything.
pub(crate) fn handoff(dir: &Path, selection: &Selection) -> Result<Option<Handoff>> {
    // An unsaved single login already lives at the path an older CLI reads.
    let Some(name) = &selection.name else {
        return Ok(None);
    };

    #[cfg(unix)]
    {
        let legacy = legacy_token_path(dir);
        if is_regular_file(&legacy) && default_account_name(dir).is_none() {
            bail!(
                "This project pins an older forklaunch CLI, which can only be pointed at \
                 account '{name}' once the current default login is in the keyring too. \
                 Save it first with `forklaunch account save <name>`."
            );
        }
        let lock = open_lock(dir)?;
        let mut announced = false;
        loop {
            lock.lock_shared()?;
            if pointer_target(dir).as_deref() == Some(name.as_str()) {
                return Ok(Some(Handoff {
                    lock,
                    dir: dir.to_path_buf(),
                }));
            }
            lock.unlock()?;
            if lock.try_lock().is_err() {
                if !announced {
                    eprintln!(
                        "Waiting: a project-pinned older forklaunch CLI is running as account \
                         '{}', and it must finish before one can run as '{name}'.",
                        pointer_target(dir).as_deref().unwrap_or("another account")
                    );
                    announced = true;
                }
                lock.lock()?;
            }
            if is_regular_file(&legacy) {
                set_aside(dir)?;
            }
            point_legacy_at(dir, name)?;
            // Not atomic with the shared lock taken next; the loop re-checks.
            lock.unlock()?;
        }
    }
    #[cfg(not(unix))]
    {
        if default_account_name(dir).as_deref() != Some(name.as_str()) {
            bail!(
                "This project pins an older forklaunch CLI, which on this platform can only \
                 use the default login, not account '{name}'. Run `forklaunch account use \
                 {name}` first, or raise `cli_version` in .forklaunch/manifest.toml."
            );
        }
        Ok(None)
    }
}

fn no_such_account(dir: &Path, name: &str) -> String {
    let known = account_names(dir);
    let known = if known.is_empty() {
        "The keyring is empty.".to_string()
    } else {
        format!("Known accounts: {}.", known.join(", "))
    };
    format!(
        "There is no login stored for account '{name}'. {known} \
         Run `forklaunch login --account {name}` to add it."
    )
}

/// What to tell someone whose selected account has no usable login.
pub(crate) fn missing_login_message(selection: &Selection) -> String {
    match (&selection.name, forklaunch_dir()) {
        (Some(name), Ok(dir)) => {
            let mut message = no_such_account(&dir, name);
            if let AccountSource::AppBinding { .. } = selection.source {
                message.push_str(
                    " (This application is bound to that account; `forklaunch account unbind` \
                     removes the binding.)",
                );
            }
            message
        }
        _ => "No token found. Please run `forklaunch login` to authenticate".to_string(),
    }
}

/// The application an invocation runs in, read from its manifest.
pub(crate) fn manifest_application(manifest_root: &Path) -> Option<(String, Option<String>)> {
    let content = read_to_string(manifest_root.join(".forklaunch").join("manifest.toml")).ok()?;
    let value: toml::Value = toml::from_str(&content).ok()?;
    let id = value.get("id")?.as_str()?.to_string();
    let name = value
        .get("app_name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Some((id, name))
}

static SELECTION: OnceLock<Selection> = OnceLock::new();

/// `--account` is a global flag, and clap leaves a global's value on the
/// level it was typed at, so look at every level of the invocation.
fn account_flag(matches: &ArgMatches) -> Option<String> {
    let mut current = Some(matches);
    while let Some(level) = current {
        if let Ok(Some(name)) = level.try_get_one::<String>(ACCOUNT_FLAG) {
            return Some(name.clone());
        }
        current = level.subcommand().map(|(_, sub)| sub);
    }
    None
}

/// Resolve the account for this process, once, before anything reads a token.
pub(crate) fn init_selection(matches: &ArgMatches, sub_matches: &ArgMatches) -> Result<()> {
    let dir = forklaunch_dir()?;
    // An older CLI used directly can leave the pointer wrong (it deletes it
    // when a login expires, and writes a plain file when it logs in). Put it
    // right before anything else looks. Machines without a keyring are left
    // exactly as they are.
    #[cfg(unix)]
    if index_path(&dir).exists()
        && let Ok(Some(aside)) = sync_pointer(&dir, false)
    {
        eprintln!(
            "A login written by an older forklaunch CLI was found at ~/.forklaunch/token and \
             moved to {}. To keep it: `forklaunch account save <name> --from {}`.",
            aside.display(),
            aside.display()
        );
    }
    let flag = account_flag(matches);
    let env = var(ACCOUNT_ENV).ok().filter(|name| !name.is_empty());
    let app = invocation_manifest_root(sub_matches).and_then(|root| manifest_application(&root));
    let selection = resolve(
        &dir,
        flag.as_deref(),
        env.as_deref(),
        app.as_ref()
            .map(|(id, name)| (id.as_str(), name.as_deref())),
    )?;
    let _ = SELECTION.set(selection);
    Ok(())
}

/// The account this process runs as. Falls back to the default login when
/// nothing resolved one (unit tests, and code paths that never parse a
/// command line).
pub(crate) fn selection() -> Result<Selection> {
    if let Some(selection) = SELECTION.get() {
        return Ok(selection.clone());
    }
    let dir = forklaunch_dir()?;
    Ok(Selection {
        name: default_account_name(&dir),
        source: AccountSource::Default,
        token_path: default_token_path(&dir),
    })
}

/// A first login into a named account on a machine with no default login
/// becomes the default, so a fresh machine needs no second step.
pub(crate) fn after_login() -> Result<Option<String>> {
    let selection = selection()?;
    let Some(name) = selection
        .name
        .filter(|_| selection.source != AccountSource::Default)
    else {
        return Ok(None);
    };
    let dir = forklaunch_dir()?;
    let has_default =
        default_account_name(&dir).is_some() || is_regular_file(&legacy_token_path(&dir));
    if !has_default {
        set_default(&dir, &name, false)?;
        return Ok(Some(format!(
            "Saved as account '{name}' (now the default)."
        )));
    }
    Ok(Some(format!("Saved as account '{name}'.")))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn jwt(email: &str, org: &str) -> String {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        format!(
            "{}.{}.sig",
            URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#),
            URL_SAFE_NO_PAD.encode(format!(
                r#"{{"email":"{email}","organizationId":"{org}","exp":1789701966}}"#
            ))
        )
    }

    fn token_file(email: &str, org: &str, refresh: &str) -> String {
        format!(
            "access_token = \"{}\"\nrefresh_token = \"{refresh}\"\nexpires_at = 1789701966\n",
            jwt(email, org)
        )
    }

    /// A machine as it looks before the keyring: one unsaved login.
    fn legacy_home() -> TempDir {
        let home = TempDir::new().unwrap();
        fs::write(
            legacy_token_path(home.path()),
            token_file("first@example.com", "org-1", "session-1"),
        )
        .unwrap();
        home
    }

    fn add(dir: &Path, name: &str) {
        write_private_file(
            &account_path(dir, name),
            &token_file(&format!("{name}@example.com"), name, "flk_key"),
        )
        .unwrap();
    }

    /// A machine with a keyring: `first` saved and the default, plus `names`.
    fn keyring(names: &[&str]) -> TempDir {
        let home = legacy_home();
        save(home.path(), "first", None, false).unwrap();
        for name in names {
            add(home.path(), name);
        }
        home
    }

    /// Who an older CLI, which reads only `~/.forklaunch/token`, would be.
    fn what_an_older_cli_reads(dir: &Path) -> Option<String> {
        identity(&legacy_token_path(dir)).email
    }

    fn named(dir: &Path, name: &str) -> Selection {
        resolve(dir, Some(name), None, None).unwrap()
    }

    #[test]
    fn an_untouched_machine_keeps_using_its_one_login() {
        let home = legacy_home();
        let selection = resolve(home.path(), None, None, None).unwrap();
        assert_eq!(selection.name, None);
        assert_eq!(selection.source, AccountSource::Default);
        assert_eq!(selection.token_path, legacy_token_path(home.path()));
        // Nothing to hand off: the login is already where an older CLI looks.
        assert!(handoff(home.path(), &selection).unwrap().is_none());
        assert!(!index_path(home.path()).exists());
    }

    #[test]
    fn saving_the_default_login_keeps_it_the_default() {
        let home = legacy_home();
        let dir = home.path();
        save(dir, "first", None, false).unwrap();

        assert_eq!(default_account_name(dir).as_deref(), Some("first"));
        let selection = resolve(dir, None, None, None).unwrap();
        assert_eq!(selection.name.as_deref(), Some("first"));
        assert_eq!(selection.token_path, account_path(dir, "first"));
        assert_eq!(
            what_an_older_cli_reads(dir).as_deref(),
            Some("first@example.com")
        );
        assert!(save(dir, "again", None, false).is_err());
    }

    #[test]
    fn flag_beats_env_beats_binding_beats_default() {
        let home = keyring(&["flagged", "from-env", "bound"]);
        let dir = home.path();
        bind(dir, "app-1", Some("shop"), "bound").unwrap();
        let app = Some(("app-1", Some("shop")));

        let pick = |flag, env, app| resolve(dir, flag, env, app).unwrap();
        assert_eq!(
            pick(Some("flagged"), Some("from-env"), app).name.as_deref(),
            Some("flagged")
        );
        assert_eq!(
            pick(None, Some("from-env"), app).name.as_deref(),
            Some("from-env")
        );
        let bound = pick(None, None, app);
        assert_eq!(bound.name.as_deref(), Some("bound"));
        assert_eq!(bound.source.key(), "app-binding");
        assert_eq!(pick(None, None, None).name.as_deref(), Some("first"));
        // An application nobody bound falls through to the default.
        assert_eq!(
            pick(None, None, Some(("app-2", None))).name.as_deref(),
            Some("first")
        );
    }

    #[test]
    fn a_named_account_with_no_login_still_resolves_so_login_can_create_it() {
        let home = TempDir::new().unwrap();
        let selection = named(home.path(), "new");
        assert_eq!(selection.token_path, account_path(home.path(), "new"));
        assert!(!selection.token_path.exists());
    }

    /// Binding ahead of the login is the safe order: the application can
    /// then only fail for want of that login, never run as the default.
    #[test]
    fn an_application_can_be_bound_before_its_account_has_a_login() {
        let home = keyring(&[]);
        let dir = home.path();
        assert!(!bind(dir, "app-1", Some("shop"), "later").unwrap());

        let selection = resolve(dir, None, None, Some(("app-1", None))).unwrap();
        assert_eq!(selection.name.as_deref(), Some("later"));
        assert!(!selection.token_path.exists());
        assert!(missing_login_message(&selection).contains("later"));

        let listed = list(dir).unwrap();
        let later = listed
            .iter()
            .find(|a| a.name.as_deref() == Some("later"))
            .unwrap();
        assert!(!later.logged_in);
        assert_eq!(later.applications[0].name.as_deref(), Some("shop"));
    }

    #[test]
    fn switching_the_default_does_not_lose_an_unsaved_login() {
        let home = legacy_home();
        let dir = home.path();
        add(dir, "other");

        let err = set_default(dir, "other", false).unwrap_err().to_string();
        assert!(err.contains("account save"), "{err}");
        assert_eq!(default_account_name(dir), None);

        // Forced, the login is set aside where it can still be imported.
        let aside = set_default(dir, "other", true).unwrap().unwrap();
        assert_eq!(default_account_name(dir).as_deref(), Some("other"));
        assert_eq!(identity(&aside).email.as_deref(), Some("first@example.com"));
        save(dir, "first", Some(&aside), false).unwrap();
    }

    #[test]
    fn switching_the_default_between_saved_accounts_is_free() {
        let home = keyring(&["second"]);
        let dir = home.path();

        assert_eq!(set_default(dir, "second", false).unwrap(), None);
        assert_eq!(default_account_name(dir).as_deref(), Some("second"));
        assert_eq!(
            what_an_older_cli_reads(dir).as_deref(),
            Some("second@example.com")
        );
        assert!(account_path(dir, "first").exists());
        assert!(set_default(dir, "missing", false).is_err());
    }

    #[test]
    fn a_hand_kept_token_file_can_be_imported_without_touching_the_default() {
        let home = legacy_home();
        let dir = home.path();
        let kept = dir.join("token.operator");
        fs::write(
            &kept,
            token_file("operator@example.com", "org-9", "session-9"),
        )
        .unwrap();

        save(dir, "operator", Some(&kept), false).unwrap();
        assert_eq!(default_account_name(dir), None);
        assert!(kept.exists());
        assert_eq!(
            identity(&account_path(dir, "operator")).email.as_deref(),
            Some("operator@example.com")
        );

        fs::write(&kept, "not a token").unwrap();
        assert!(save(dir, "junk", Some(&kept), false).is_err());
    }

    #[test]
    fn removing_an_account_drops_its_bindings_and_never_strands_the_default() {
        let home = keyring(&["second"]);
        let dir = home.path();
        bind(dir, "app-1", Some("shop"), "first").unwrap();
        bind(dir, "app-2", None, "second").unwrap();

        // The default cannot go while another account could take its place.
        let err = remove(dir, "first").unwrap_err().to_string();
        assert!(err.contains("account use"), "{err}");

        let unbound = remove(dir, "second").unwrap();
        assert_eq!(unbound.len(), 1);
        assert_eq!(unbound[0].id, "app-2");
        assert!(!account_path(dir, "second").exists());

        // The last account takes the keyring's default with it.
        let unbound = remove(dir, "first").unwrap();
        assert_eq!(unbound[0].id, "app-1");
        assert_eq!(default_account_name(dir), None);
        assert!(legacy_token_path(dir).symlink_metadata().is_err());
    }

    #[test]
    fn logging_out_keeps_the_name_so_the_next_command_names_the_gap() {
        let home = keyring(&[]);
        let dir = home.path();
        bind(dir, "app-1", Some("shop"), "first").unwrap();

        let selection = resolve(dir, None, None, Some(("app-1", None))).unwrap();
        logout(dir, &selection).unwrap();

        assert!(!account_path(dir, "first").exists());
        assert_eq!(default_account_name(dir).as_deref(), Some("first"));
        let after = resolve(dir, None, None, Some(("app-1", None))).unwrap();
        assert_eq!(after.name.as_deref(), Some("first"));
        assert!(!after.token_path.exists());
        // An older CLI finds no login either, not somebody else's.
        assert_eq!(what_an_older_cli_reads(dir), None);
    }

    #[test]
    fn listing_labels_each_login_and_shows_an_unsaved_default() {
        let home = legacy_home();
        let dir = home.path();
        add(dir, "shop");
        bind(dir, "app-1", Some("shop-api"), "shop").unwrap();

        let accounts = list(dir).unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].name.as_deref(), Some("shop"));
        assert_eq!(accounts[0].identity.kind, Some("api-key"));
        assert_eq!(
            accounts[0].identity.organization_id.as_deref(),
            Some("shop")
        );
        assert_eq!(
            accounts[0].applications[0].name.as_deref(),
            Some("shop-api")
        );
        assert!(!accounts[0].is_default);
        assert_eq!(accounts[1].name, None);
        assert!(accounts[1].is_default);
        assert_eq!(accounts[1].identity.kind, Some("session"));
    }

    #[test]
    fn account_names_must_be_safe_as_file_names() {
        for good in [
            "pact",
            "rohin+pact",
            "shared.cluster",
            "a_b-c",
            "me@example.com",
        ] {
            assert!(validate_name(good).is_ok(), "{good}");
        }
        for bad in ["", "../etc", "a/b", ".hidden", "-flag", "with space"] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
        assert!(resolve(Path::new("/nonexistent"), Some("../x"), None, None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn stored_logins_are_readable_only_by_their_owner() {
        use std::os::unix::fs::PermissionsExt;
        let home = keyring(&[]);
        let mode = |path: PathBuf| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(account_path(home.path(), "first")), 0o600);
        assert_eq!(mode(accounts_dir(home.path())), 0o700);
    }

    // ── Older CLIs ────────────────────────────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn an_older_cli_is_handed_the_selected_account_and_the_default_comes_back() {
        let home = keyring(&["shop"]);
        let dir = home.path();

        let held = handoff(dir, &named(dir, "shop")).unwrap().unwrap();
        assert_eq!(
            what_an_older_cli_reads(dir).as_deref(),
            Some("shop@example.com")
        );
        // This CLI is not fooled by the pointer while it is borrowed.
        assert_eq!(
            resolve(dir, None, None, None).unwrap().name.as_deref(),
            Some("first")
        );

        drop(held);
        assert_eq!(
            what_an_older_cli_reads(dir).as_deref(),
            Some("first@example.com")
        );
    }

    #[cfg(unix)]
    #[test]
    fn older_clis_for_the_same_account_run_together_and_others_wait() {
        use std::{sync::mpsc, thread, time::Duration};
        let home = keyring(&["shop", "bank"]);
        let dir = home.path().to_path_buf();

        let first = handoff(&dir, &named(&dir, "shop")).unwrap().unwrap();
        let second = handoff(&dir, &named(&dir, "shop")).unwrap().unwrap();

        let (acquired, on_acquired) = mpsc::channel();
        let (release, on_release) = mpsc::channel::<()>();
        let waiter = {
            let dir = dir.clone();
            thread::spawn(move || {
                let held = handoff(&dir, &named(&dir, "bank")).unwrap().unwrap();
                acquired.send(what_an_older_cli_reads(&dir)).unwrap();
                on_release.recv().unwrap();
                drop(held);
            })
        };

        // While either `shop` command runs, `bank` waits and `shop` stays put.
        assert!(
            on_acquired
                .recv_timeout(Duration::from_millis(200))
                .is_err()
        );
        drop(first);
        assert!(
            on_acquired
                .recv_timeout(Duration::from_millis(200))
                .is_err()
        );
        assert_eq!(
            what_an_older_cli_reads(&dir).as_deref(),
            Some("shop@example.com")
        );

        drop(second);
        assert_eq!(
            on_acquired
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .as_deref(),
            Some("bank@example.com")
        );
        release.send(()).unwrap();
        waiter.join().unwrap();
        assert_eq!(
            what_an_older_cli_reads(&dir).as_deref(),
            Some("first@example.com")
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_older_cli_is_not_pointed_away_from_an_unsaved_default_login() {
        let home = legacy_home();
        let dir = home.path();
        add(dir, "shop");

        let err = handoff(dir, &named(dir, "shop")).unwrap_err().to_string();
        assert!(err.contains("account save"), "{err}");
        assert_eq!(
            what_an_older_cli_reads(dir).as_deref(),
            Some("first@example.com")
        );
    }

    /// An older CLI used directly deletes the path when a login expires and
    /// writes a plain file when it logs in. Neither may cost a login or
    /// leave the default pointing nowhere.
    #[cfg(unix)]
    #[test]
    fn what_an_older_cli_did_to_the_pointer_is_put_right() {
        let home = keyring(&[]);
        let dir = home.path();

        fs::remove_file(legacy_token_path(dir)).unwrap();
        assert_eq!(sync_pointer(dir, false).unwrap(), None);
        assert_eq!(
            what_an_older_cli_reads(dir).as_deref(),
            Some("first@example.com")
        );

        fs::remove_file(legacy_token_path(dir)).unwrap();
        fs::write(
            legacy_token_path(dir),
            token_file("stray@example.com", "org-7", "session-7"),
        )
        .unwrap();
        let aside = sync_pointer(dir, false).unwrap().unwrap();
        assert_eq!(identity(&aside).email.as_deref(), Some("stray@example.com"));
        assert_eq!(
            what_an_older_cli_reads(dir).as_deref(),
            Some("first@example.com")
        );
    }
}
