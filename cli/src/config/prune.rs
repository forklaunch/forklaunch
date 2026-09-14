//! Remove stored environment variables that nothing declares.
//!
//! Dead keys accumulate: a component seeded by copying another component's
//! config, a renamed variable, a runner setting that went away. Until now the
//! only cleanup tools were soft — `config unset` and `config push` leave a
//! tombstone row behind — so a scope could carry dozens of keys no code reads.
//!
//! "Unused" is decided per scope against what the code actually reads: the
//! same AST scan of the local workspace that produces the release manifest's
//! `requiredEnvironmentVariables`. A key is kept if the scope's own project
//! reads it, if any library module reads it (libraries run inside every
//! component), or — for the application scope — if anything reads it. If the
//! workspace cannot be scanned, this command refuses to prune rather than
//! guess.
//!
//! A second, conservative source guards the scan's blind spots: every module
//! source and config file is searched textually, and a key mentioned anywhere
//! (a `process.env` read, an env block handed to a subprocess, a Dockerfile, an
//! `.env.template`) is kept. Keys consumed by the runtime or by tooling rather
//! than application code (NODE_OPTIONS, HOME, DOCKER_*, AWS_*, ANTHROPIC_*, ...)
//! are reported separately and skipped unless `--include-runtime`, and
//! `--match` scopes a deletion to a glob, so a bulk run never has to trust the
//! whole list.
//!
//! Keys the platform regenerates on every deploy (DB_*, REDIS_*, OTEL_*,
//! inter-service URLs, ...) are reported separately and skipped unless
//! `--include-platform` is given: deleting them is harmless but pointless.
//!
//! Nothing is deleted without `--yes`; the default run is a dry run. Values
//! are never printed. After deleting, the configuration is pulled again and
//! every key is verified gone — a control plane that does not yet honor
//! `hardDelete` would otherwise report success while changing nothing.
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::Write,
    path::Path,
};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Value, json};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::{CliCommand, unset::ScopeTarget};
use crate::{
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        ast::infrastructure::env::find_all_env_vars,
        command::command,
        env::{EnvFileItem, find_workspace_root, get_modules_path, parse_env_items_from_str},
        http_client,
        manifest::{ProjectEntry, ProjectType},
        rendered_template::RenderedTemplatesCache,
        validate::{require_auth, require_integration, require_manifest},
    },
};

/// One scope's stored keys as `config pull` reports them.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct PulledScope {
    /// Platform id from the `# name (id)` header; `None` for `application`.
    pub(crate) id: Option<String>,
    /// Key -> whether it currently holds a value.
    pub(crate) keys: BTreeMap<String, bool>,
}

/// Parse `config pull` text into scopes. The `application` section has no id.
pub(crate) fn parse_scopes(content: &str) -> BTreeMap<String, PulledScope> {
    let mut scopes: BTreeMap<String, PulledScope> = BTreeMap::new();
    let mut current: Option<String> = None;
    for item in parse_env_items_from_str(content) {
        match item {
            EnvFileItem::SectionHeader(header) => {
                let body = header.trim_start_matches('#').trim();
                let (name, id) = match body.split_once(" (") {
                    Some((n, rest)) => (
                        n.trim().to_string(),
                        Some(rest.trim_end_matches(')').trim().to_string()),
                    ),
                    None => (body.to_string(), None),
                };
                scopes.entry(name.clone()).or_default().id = id;
                current = Some(name);
            }
            EnvFileItem::KeyValue(key, value) => {
                let scope = current.clone().unwrap_or_else(|| "application".to_string());
                scopes
                    .entry(scope)
                    .or_default()
                    .keys
                    .insert(key, !value.trim().is_empty());
            }
        }
    }
    scopes
}

/// The project a scope belongs to: `payments-service` / `payments-worker` -> `payments`.
pub(crate) fn project_of_scope(scope: &str) -> String {
    scope
        .strip_suffix("-service")
        .or_else(|| scope.strip_suffix("-worker"))
        .unwrap_or(scope)
        .to_string()
}

/// Which endpoint owns a pulled scope. The manifest names a project once
/// (`payments`), while the pulled config names its deployed components
/// (`payments-service`, `payments-worker`), so both suffixes must resolve —
/// the `unset` resolver only knew the bare name and `-worker`.
pub(crate) fn scope_target_from<'a>(
    projects: impl IntoIterator<Item = (&'a str, &'a ProjectType)>,
    scope: &str,
    id: String,
) -> Option<ScopeTarget> {
    let base = project_of_scope(scope);
    let (_, kind) = projects.into_iter().find(|(name, _)| *name == base)?;
    match kind {
        ProjectType::Library => None,
        _ if scope.ends_with("-worker") => Some(ScopeTarget::Worker(id)),
        _ => Some(ScopeTarget::Service(id)),
    }
}

pub(crate) fn scope_target(
    projects: &[ProjectEntry],
    scope: &str,
    id: String,
) -> Option<ScopeTarget> {
    scope_target_from(
        projects.iter().map(|p| (p.name.as_str(), &p.r#type)),
        scope,
        id,
    )
}

/// Keys the platform generates on every deploy. Deleting a stored copy is
/// harmless (it comes back) but pointless, so they are skipped by default.
pub(crate) fn is_platform_managed(key: &str, project_names: &[String]) -> bool {
    const EXACT: &[&str] = &[
        "HOST",
        "PORT",
        "PROTOCOL",
        "VERSION",
        "DOCS_PATH",
        "DOCS_SERVER_URLS",
        "DOCS_SERVER_DESCRIPTIONS",
        "NODE_ENV",
        "QUEUE_NAME",
        "WS_PORT",
        "PGSSLMODE",
        "PROMETHEUS_URL",
        "LOKI_URL",
        "TEMPO_URL",
        "MONITORING_SECRET",
        "JWKS_PUBLIC_KEY_URL",
        "BETTER_AUTH_BASE_URL",
    ];
    const PREFIXES: &[&str] = &["DB_", "DATABASE_", "REDIS_", "KAFKA_", "OTEL_"];
    if EXACT.contains(&key) || PREFIXES.iter().any(|p| key.starts_with(p)) {
        return true;
    }
    // Inter-service URLs: MANAGED_APPS_WORKER_URL -> managed-apps-worker.
    if let Some(stem) = key.strip_suffix("_URL") {
        let component = stem.to_lowercase().replace('_', "-");
        let base = project_of_scope(&component);
        return project_names.iter().any(|p| *p == component || *p == base);
    }
    false
}

/// Keys consumed by the runtime, Docker, cloud SDKs, or agent subprocesses —
/// never by application code, so no scan can see the read. Skipped by default.
pub(crate) fn is_runtime_consumed(key: &str) -> bool {
    const EXACT: &[&str] = &[
        "HOME", "PATH", "TMPDIR", "CI", "SHELL", "USER", "LANG", "TZ",
    ];
    const PREFIXES: &[&str] = &[
        "NODE_",
        "DOCKER_",
        "COMPOSE_",
        "AWS_",
        "ANTHROPIC_",
        "CLAUDE_",
        "GOOGLE_",
        "OPENAI_",
        "GEMINI_",
        "FORKLAUNCH_",
        "SANDBOX_",
        "SES_",
        "GITHUB_",
        "GIT_",
        "npm_",
        "PNPM_",
        "BUN_",
        "DENO_",
    ];
    EXACT.contains(&key) || PREFIXES.iter().any(|p| key.starts_with(p))
}

/// Minimal glob: `*` matches any run of characters; everything else is literal.
pub(crate) fn glob_match(pattern: &str, key: &str) -> bool {
    fn go(p: &[u8], k: &[u8]) -> bool {
        match (p.first(), k.first()) {
            (None, None) => true,
            (Some(b'*'), _) => go(&p[1..], k) || (!k.is_empty() && go(p, &k[1..])),
            (Some(a), Some(b)) if a == b => go(&p[1..], &k[1..]),
            _ => false,
        }
    }
    go(pattern.as_bytes(), key.as_bytes())
}

fn is_word_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Whether `key` appears as a whole word anywhere in `text`.
pub(crate) fn mentions_key(text: &str, key: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(pos) = text[from..].find(key) {
        let start = from + pos;
        let end = start + key.len();
        let before_ok = start == 0 || !is_word_char(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_word_char(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = end;
    }
    false
}

fn is_scannable(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.starts_with("Dockerfile") || name.starts_with(".env") {
        return true;
    }
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx" | "js" | "mjs" | "cjs" | "json" | "yaml" | "yml" | "toml" | "sh" | "md")
    )
}

/// Which of `candidates` are mentioned as a whole word in any scannable file
/// under `root` (node_modules, dist, build outputs and VCS dirs skipped).
pub(crate) fn referenced_in_tree(root: &Path, candidates: &HashSet<String>) -> HashSet<String> {
    const SKIP_DIRS: &[&str] = &[
        "node_modules",
        "dist",
        "build",
        ".git",
        "target",
        ".turbo",
        "coverage",
    ];
    let mut found: HashSet<String> = HashSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !SKIP_DIRS.contains(&name) {
                    stack.push(path);
                }
                continue;
            }
            if !is_scannable(&path) || found.len() == candidates.len() {
                continue;
            }
            if path.metadata().is_ok_and(|meta| meta.len() > 2_000_000) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for key in candidates {
                if !found.contains(key) && mentions_key(&text, key) {
                    found.insert(key.clone());
                }
            }
        }
    }
    found
}

#[derive(Debug, PartialEq)]
pub(crate) enum Verdict {
    Declared,
    Referenced,
    Platform,
    Runtime,
    Orphan,
}

pub(crate) fn classify_key(
    key: &str,
    declared: &HashSet<String>,
    referenced: &HashSet<String>,
    project_names: &[String],
    include_platform: bool,
    include_runtime: bool,
) -> Verdict {
    if declared.contains(key) {
        Verdict::Declared
    } else if referenced.contains(key) {
        Verdict::Referenced
    } else if !include_platform && is_platform_managed(key, project_names) {
        Verdict::Platform
    } else if !include_runtime && is_runtime_consumed(key) {
        Verdict::Runtime
    } else {
        Verdict::Orphan
    }
}

/// The endpoint and body that hard-deletes `keys` in one scope.
pub(crate) fn prune_request(
    target: &ScopeTarget,
    app: &str,
    environment: &str,
    region: &str,
    keys: &[String],
) -> (String, Value) {
    let api = get_platform_management_api_url();
    match target {
        ScopeTarget::Application => (
            format!(
                "{}/applications/{}/environments/{}/variables",
                api, app, environment
            ),
            json!({
                "region": region,
                "variables": keys.iter().map(|k| json!({
                    "key": k, "value": "", "required": false, "hasValue": false,
                    "source": "application", "hardDelete": true
                })).collect::<Vec<_>>()
            }),
        ),
        ScopeTarget::Service(id) => (
            format!(
                "{}/services/{}/environments/{}/variables",
                api, id, environment
            ),
            json!({ "region": region, "variables": keys.iter().map(|k| json!({ "key": k, "value": "", "hardDelete": true })).collect::<Vec<_>>() }),
        ),
        ScopeTarget::Worker(id) => (
            format!(
                "{}/workers/{}/environments/{}/variables",
                api, id, environment
            ),
            json!({ "region": region, "variables": keys.iter().map(|k| json!({ "key": k, "value": "", "hardDelete": true })).collect::<Vec<_>>() }),
        ),
    }
}

/// Everything each project reads, plus the union over libraries and over all.
struct Declared {
    by_project: HashMap<String, HashSet<String>>,
    libraries: HashSet<String>,
    all: HashSet<String>,
}

fn declared_for(declared: &Declared, scope: &str) -> HashSet<String> {
    if scope == "application" {
        return declared.all.clone();
    }
    let mut set = declared.libraries.clone();
    if let Some(own) = declared.by_project.get(&project_of_scope(scope)) {
        set.extend(own.iter().cloned());
    }
    set
}

#[derive(Debug)]
pub(crate) struct PruneCommand;

impl PruneCommand {
    pub(crate) fn new() -> Self {
        Self
    }

    fn pull(app: &str, region: &str, environment: &str) -> Result<String> {
        let url = format!(
            "{}/config/pull?applicationId={}&region={}&environment={}",
            get_platform_management_api_url(),
            app,
            region,
            environment
        );
        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to pull current config: {}",
                response.text().unwrap_or_default()
            );
        }
        Ok(response.text()?)
    }
}

impl CliCommand for PruneCommand {
    fn command(&self) -> Command {
        command("prune", "Remove stored environment variables that nothing in the application reads")
            .arg(Arg::new("region").short('r').long("region").required(true).help("Region (e.g. us-east-1)"))
            .arg(Arg::new("environment").short('e').long("environment").required(true).help("Environment name (e.g. production, staging)"))
            .arg(Arg::new("service").short('s').long("service").help("Only prune one scope (a service/worker name, or 'application')"))
            .arg(Arg::new("yes").long("yes").short('y').action(ArgAction::SetTrue).help("Actually delete. Without this the command only reports what it would remove"))
            .arg(Arg::new("include_platform").long("include-platform").action(ArgAction::SetTrue).help("Also delete keys the platform regenerates on every deploy (DB_*, REDIS_*, OTEL_*, inter-service URLs)"))
            .arg(Arg::new("include_runtime").long("include-runtime").action(ArgAction::SetTrue).help("Also delete keys consumed by the runtime/tooling rather than app code (NODE_OPTIONS, HOME, DOCKER_*, AWS_*, ANTHROPIC_*, ...)"))
            .arg(Arg::new("match").long("match").short('m').action(ArgAction::Append).help("Only consider keys matching this glob (repeatable, e.g. --match 'TWILIO_*')"))
            .arg(Arg::new("base_path").long("path").short('p').help("Path to application root (optional)"))
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (app_root, manifest) = require_manifest(matches)?;
        let app = require_integration(&manifest)?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let region = matches.get_one::<String>("region").expect("required");
        let environment = matches.get_one::<String>("environment").expect("required");
        let only_scope = matches.get_one::<String>("service").cloned();
        let delete = matches.get_flag("yes");
        let include_platform = matches.get_flag("include_platform");
        let include_runtime = matches.get_flag("include_runtime");
        let patterns: Vec<String> = matches
            .get_many::<String>("match")
            .map(|v| v.cloned().collect())
            .unwrap_or_default();

        // What the code reads. Refuse to guess if the workspace cannot be scanned.
        let workspace_root = find_workspace_root(&app_root)?;
        let modules_path = get_modules_path(&workspace_root)?;
        let cache = RenderedTemplatesCache::new();
        let usages = find_all_env_vars(&modules_path, &cache)
            .context("could not scan the workspace for declared variables")?;
        if usages.is_empty() {
            bail!(
                "no environment variable declarations found in the workspace — refusing to prune on an empty picture"
            );
        }
        let by_project: HashMap<String, HashSet<String>> = usages
            .iter()
            .map(|(p, u)| (p.clone(), u.iter().map(|x| x.var_name.clone()).collect()))
            .collect();
        let library_names: HashSet<String> = manifest
            .projects
            .iter()
            .filter(|p: &&ProjectEntry| matches!(p.r#type, ProjectType::Library))
            .map(|p| p.name.clone())
            .collect();
        let libraries: HashSet<String> = by_project
            .iter()
            .filter(|(p, _)| library_names.contains(*p))
            .flat_map(|(_, s)| s.iter().cloned())
            .collect();
        let all: HashSet<String> = by_project.values().flatten().cloned().collect();
        let declared = Declared {
            by_project,
            libraries,
            all,
        };
        let project_names: Vec<String> = manifest.projects.iter().map(|p| p.name.clone()).collect();

        let content = Self::pull(&app, region, environment)?;
        let scopes = parse_scopes(&content);

        // Second source: any mention in module source/config files keeps a key.
        let candidates: HashSet<String> = scopes
            .values()
            .flat_map(|s| s.keys.keys().cloned())
            .collect();
        let referenced = referenced_in_tree(&modules_path, &candidates);

        let mut plan: BTreeMap<String, (Option<String>, Vec<String>)> = BTreeMap::new();
        let mut platform_skipped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut runtime_skipped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut referenced_kept: BTreeMap<String, usize> = BTreeMap::new();
        for (scope, pulled) in &scopes {
            if only_scope.as_ref().is_some_and(|only| only != scope) {
                continue;
            }
            let declared_here = declared_for(&declared, scope);
            for (key, has_value) in &pulled.keys {
                if !patterns.is_empty() && !patterns.iter().any(|p| glob_match(p, key)) {
                    continue;
                }
                match classify_key(
                    key,
                    &declared_here,
                    &referenced,
                    &project_names,
                    include_platform,
                    include_runtime,
                ) {
                    Verdict::Declared => {}
                    Verdict::Referenced => {
                        *referenced_kept.entry(scope.clone()).or_default() += 1;
                    }
                    Verdict::Platform => {
                        platform_skipped
                            .entry(scope.clone())
                            .or_default()
                            .insert(key.clone());
                    }
                    Verdict::Runtime => {
                        runtime_skipped
                            .entry(scope.clone())
                            .or_default()
                            .insert(key.clone());
                    }
                    Verdict::Orphan => {
                        plan.entry(scope.clone())
                            .or_insert_with(|| (pulled.id.clone(), Vec::new()))
                            .1
                            .push(format!(
                                "{}{}",
                                key,
                                if *has_value { "" } else { " (no value)" }
                            ));
                    }
                }
            }
        }

        let total: usize = plan.values().map(|(_, k)| k.len()).sum();
        for (scope, (_, keys)) in &plan {
            log_warn!(
                stdout,
                "{}: {} unused variable(s) nothing declares",
                scope,
                keys.len()
            );
            for k in keys {
                writeln!(stdout, "    - {}", k)?;
            }
        }
        for (scope, n) in &referenced_kept {
            log_info!(
                stdout,
                "{}: {} undeclared key(s) are still mentioned in module source/config files, kept.",
                scope,
                n
            );
        }
        for (scope, keys) in &runtime_skipped {
            log_info!(
                stdout,
                "{}: {} runtime/tooling key(s) skipped (use --include-runtime to delete): {}",
                scope,
                keys.len(),
                keys.iter().cloned().collect::<Vec<_>>().join(", ")
            );
        }
        for (scope, keys) in &platform_skipped {
            log_info!(
                stdout,
                "{}: {} platform-managed key(s) regenerated on deploy, skipped (use --include-platform to delete): {}",
                scope,
                keys.len(),
                keys.iter().cloned().collect::<Vec<_>>().join(", ")
            );
        }
        if total == 0 {
            log_ok!(stdout, "Nothing to prune for {} ({}).", environment, region);
            return Ok(());
        }
        if !delete {
            log_info!(
                stdout,
                "Dry run: {} variable(s) would be deleted. Re-run with --yes to delete them. Values are never shown.",
                total
            );
            return Ok(());
        }

        let mut deleted: Vec<(String, String)> = Vec::new();
        for (scope, (id, keys)) in &plan {
            let target = if scope == "application" {
                ScopeTarget::Application
            } else {
                let id = id.clone().ok_or_else(|| {
                    anyhow::anyhow!("scope '{}' has no platform id in the pulled config", scope)
                })?;
                scope_target(&manifest.projects, scope, id).ok_or_else(|| {
                    anyhow::anyhow!(
                        "scope '{}' is not a service or worker in this application's manifest",
                        scope
                    )
                })?
            };
            let names: Vec<String> = keys
                .iter()
                .map(|k| k.split(' ').next().unwrap_or(k).to_string())
                .collect();
            let (url, body) = prune_request(&target, &app, environment, region, &names);
            let response =
                http_client::put(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
            if !response.status().is_success() {
                bail!("Failed to prune scope '{}': {}", scope, response.text()?);
            }
            for n in names {
                deleted.push((scope.clone(), n));
            }
        }

        // Verify: a control plane that does not honor hardDelete would have
        // accepted the request and changed nothing.
        let after = parse_scopes(&Self::pull(&app, region, environment)?);
        let remaining: Vec<String> = deleted
            .iter()
            .filter(|(scope, key)| {
                after
                    .get(scope)
                    .map(|s| s.keys.contains_key(key))
                    .unwrap_or(false)
            })
            .map(|(scope, key)| format!("{}/{}", scope, key))
            .collect();
        let removed = deleted.len() - remaining.len();
        log_ok!(
            stdout,
            "PRUNED {} variable(s) for {} ({}).",
            removed,
            environment,
            region
        );
        if !remaining.is_empty() {
            log_warn!(
                stdout,
                "{} variable(s) are still present after the delete — the control plane may not support hardDelete on this endpoint yet: {}",
                remaining.len(),
                remaining.join(", ")
            );
        }
        log_info!(
            stdout,
            "Running tasks keep their existing environment — redeploy to apply."
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PULLED: &str = "# application\nIAM_DB_NAME=iam_database\nSHARED=1\n\n# payments-service (svc-1)\nSTRIPE_KEY=sk\nDB_NAME=payments_database\nOLD_RUNNER=x\n\n# payments-worker (wkr-1)\nQUEUE_NAME=payments\nTWILIO_AUTH_SID=\n";

    #[test]
    fn parses_scopes_with_ids_and_value_presence() {
        let s = parse_scopes(PULLED);
        assert_eq!(s["application"].id, None);
        assert_eq!(s["payments-service"].id.as_deref(), Some("svc-1"));
        assert_eq!(s["payments-worker"].keys["TWILIO_AUTH_SID"], false);
        assert_eq!(s["payments-service"].keys["STRIPE_KEY"], true);
    }

    #[test]
    fn scope_maps_to_its_project() {
        assert_eq!(project_of_scope("payments-service"), "payments");
        assert_eq!(project_of_scope("payments-worker"), "payments");
        assert_eq!(project_of_scope("payments"), "payments");
    }

    #[test]
    fn platform_keys_include_inter_service_urls() {
        let projects = vec!["managed-apps".to_string(), "iam".to_string()];
        assert!(is_platform_managed("REDIS_DB", &projects));
        assert!(is_platform_managed("OTEL_SERVICE_NAME", &projects));
        assert!(is_platform_managed("MANAGED_APPS_WORKER_URL", &projects));
        assert!(is_platform_managed("IAM_URL", &projects));
        assert!(!is_platform_managed("STRIPE_KEY", &projects));
        assert!(!is_platform_managed("TWILIO_AUTH_SID", &projects));
    }

    #[test]
    fn classifies_declared_platform_and_orphan() {
        let declared: HashSet<String> = ["STRIPE_KEY".to_string()].into_iter().collect();
        let referenced: HashSet<String> = ["STUDIO_MODEL".to_string()].into_iter().collect();
        let projects = vec!["payments".to_string()];
        assert_eq!(
            classify_key(
                "STRIPE_KEY",
                &declared,
                &referenced,
                &projects,
                false,
                false
            ),
            Verdict::Declared
        );
        assert_eq!(
            classify_key(
                "STUDIO_MODEL",
                &declared,
                &referenced,
                &projects,
                false,
                false
            ),
            Verdict::Referenced
        );
        assert_eq!(
            classify_key("DB_NAME", &declared, &referenced, &projects, false, false),
            Verdict::Platform
        );
        assert_eq!(
            classify_key("DB_NAME", &declared, &referenced, &projects, true, false),
            Verdict::Orphan
        );
        assert_eq!(
            classify_key(
                "NODE_OPTIONS",
                &declared,
                &referenced,
                &projects,
                false,
                false
            ),
            Verdict::Runtime
        );
        assert_eq!(
            classify_key(
                "NODE_OPTIONS",
                &declared,
                &referenced,
                &projects,
                false,
                true
            ),
            Verdict::Orphan
        );
        assert_eq!(
            classify_key(
                "TWILIO_AUTH_SID",
                &declared,
                &referenced,
                &projects,
                false,
                false
            ),
            Verdict::Orphan
        );
    }

    #[test]
    fn scope_target_resolves_service_and_worker_suffixes() {
        let projects = [
            ("managed-apps", ProjectType::Worker),
            ("iam", ProjectType::Service),
            ("core", ProjectType::Library),
        ];
        let it = || projects.iter().map(|(n, t)| (*n, t));
        assert!(matches!(
            scope_target_from(it(), "managed-apps-service", "s".into()),
            Some(ScopeTarget::Service(_))
        ));
        assert!(matches!(
            scope_target_from(it(), "managed-apps-worker", "w".into()),
            Some(ScopeTarget::Worker(_))
        ));
        assert!(matches!(
            scope_target_from(it(), "iam", "i".into()),
            Some(ScopeTarget::Service(_))
        ));
        assert!(scope_target_from(it(), "core", "c".into()).is_none());
        assert!(scope_target_from(it(), "nope-service", "n".into()).is_none());
    }

    #[test]
    fn glob_and_word_matching() {
        assert!(glob_match("TWILIO_*", "TWILIO_AUTH_SID"));
        assert!(glob_match("*_SID", "TWILIO_AUTH_SID"));
        assert!(!glob_match("TWILIO_*", "STRIPE_KEY"));
        assert!(glob_match("EXACT", "EXACT"));
        assert!(mentions_key(
            "const x = process.env.STUDIO_MODEL;",
            "STUDIO_MODEL"
        ));
        assert!(!mentions_key("STUDIO_MODEL_ID=1", "STUDIO_MODEL"));
        assert!(mentions_key("KEY=1", "KEY"));
    }

    #[test]
    fn textual_scan_finds_process_env_reads_and_skips_node_modules() {
        let dir = std::env::temp_dir().join(format!("prune-scan-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("svc/node_modules/pkg")).unwrap();
        std::fs::write(
            dir.join("svc/runner.ts"),
            "spawn('x', { env: { ...process.env, STUDIO_MODEL: '1' } })",
        )
        .unwrap();
        std::fs::write(
            dir.join("svc/node_modules/pkg/index.js"),
            "process.env.ONLY_IN_DEPS",
        )
        .unwrap();
        let candidates: HashSet<String> = ["STUDIO_MODEL", "ONLY_IN_DEPS", "NOWHERE"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let found = referenced_in_tree(&dir, &candidates);
        assert!(found.contains("STUDIO_MODEL"));
        assert!(!found.contains("ONLY_IN_DEPS"));
        assert!(!found.contains("NOWHERE"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn prune_request_bodies_carry_hard_delete() {
        let keys = vec!["A".to_string(), "B".to_string()];
        let (url, body) = prune_request(
            &ScopeTarget::Worker("wkr-1".into()),
            "app-1",
            "production",
            "us-west-2",
            &keys,
        );
        assert!(url.ends_with("/workers/wkr-1/environments/production/variables"));
        assert_eq!(body["variables"][1]["key"], "B");
        assert_eq!(body["variables"][1]["hardDelete"], true);
        let (url, body) = prune_request(
            &ScopeTarget::Application,
            "app-1",
            "production",
            "us-west-2",
            &keys,
        );
        assert!(url.ends_with("/applications/app-1/environments/production/variables"));
        assert_eq!(body["variables"][0]["source"], "application");
        assert_eq!(body["variables"][0]["hardDelete"], true);
    }
}
