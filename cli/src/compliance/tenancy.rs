//! `forklaunch compliance audit-tenancy`: a static check for the access
//! patterns that read or write encrypted columns under the wrong tenant key.
//!
//! Every encrypted column is written under one tenant's key (HKDF of the
//! master key and the tenant id) and decrypts only under that key; a WHERE
//! clause on one is encrypted under the *current* tenant. So an entity
//! manager that reaches a classified entity must be bound to the row's
//! tenant, and there is no such thing as "no tenant": the empty tenant `''`
//! is what a forgotten binding looks like, and rows written under it are
//! unreadable from anywhere a real tenant is in play.
//!
//! The check runs offline over a project's production TypeScript and reports
//! each occurrence with file and line. Errors fail the command (exit 1) so it
//! can gate CI; warnings do not unless `--strict`. A line can be exempted
//! with `// forklaunch-tenancy: allow <reason>` on the line above it — the
//! reason is required and is reported alongside the exemption.

use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{command::command, validate::require_manifest},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TenancySeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TenancyFinding {
    pub(crate) severity: TenancySeverity,
    pub(crate) rule: &'static str,
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) snippet: String,
    pub(crate) message: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TenancyExemption {
    pub(crate) rule: &'static str,
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) reason: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TenancyReport {
    pub(crate) files_scanned: usize,
    pub(crate) findings: Vec<TenancyFinding>,
    /// Findings listed in the baseline file: known, not yet fixed, and not
    /// failing the run. New findings are the ones that fail it.
    pub(crate) baselined: Vec<TenancyFinding>,
    pub(crate) exemptions: Vec<TenancyExemption>,
}

/// One accepted-for-now finding. Keyed on file, rule and snippet rather than
/// line number, so unrelated edits above it do not invalidate the entry;
/// fixing the line (or moving the file) drops it out of the baseline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BaselineEntry {
    pub(crate) file: String,
    pub(crate) rule: String,
    pub(crate) snippet: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TenancyBaseline {
    pub(crate) entries: Vec<BaselineEntry>,
}

impl TenancyBaseline {
    fn from_findings(findings: &[TenancyFinding]) -> Self {
        let mut entries: Vec<BaselineEntry> = findings
            .iter()
            .map(|f| BaselineEntry {
                file: f.file.clone(),
                rule: f.rule.to_string(),
                snippet: f.snippet.clone(),
            })
            .collect();
        entries.sort();
        entries.dedup();
        Self { entries }
    }

    fn covers(&self, finding: &TenancyFinding) -> bool {
        self.entries.iter().any(|e| {
            e.file == finding.file && e.rule == finding.rule && e.snippet == finding.snippet
        })
    }
}

/// Split findings into new ones and ones the baseline already lists.
pub(crate) fn apply_baseline(
    findings: Vec<TenancyFinding>,
    baseline: &TenancyBaseline,
) -> (Vec<TenancyFinding>, Vec<TenancyFinding>) {
    findings.into_iter().partition(|f| !baseline.covers(f))
}

const NON_PRODUCTION_DIRS: &[&str] = &["node_modules", "dist", "lib", "__test__", ".git"];

const ALLOW_MARKER: &str = "forklaunch-tenancy: allow";

const RULE_EMPTY_TENANT: &str = "empty-tenant";
const RULE_UNBOUND_RESOLVE: &str = "unbound-resolve";
const RULE_SUPER_ADMIN_READ: &str = "super-admin-read";
const RULE_UNBOUND_FORK: &str = "unbound-fork";

/// Text that means "the empty tenant is being bound on purpose".
const EMPTY_TENANT_PATTERNS: &[&str] = &[
    "tenantId: ''",
    "tenantId: \"\"",
    "tenantId: ``",
    "withEncryptionContext('',",
    "withEncryptionContext(\"\",",
    "setEncryptionTenantId('')",
    "setEncryptionTenantId(\"\")",
];

fn is_production_ts(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with(".ts")
        && !name.ends_with(".d.ts")
        && !name.ends_with(".test.ts")
        && !name.ends_with(".spec.ts")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if !NON_PRODUCTION_DIRS.contains(&name) {
                walk(&path, out);
            }
        } else if is_production_ts(&path) {
            out.push(path);
        }
    }
}

/// Strip `//` line comments and `/* */` blocks so prose about the entity
/// manager does not count as a dependency on it.
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_block = false;
    let mut in_line = false;
    let mut in_string: Option<char> = None;
    while let Some(c) = chars.next() {
        if in_block {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block = false;
            }
            continue;
        }
        if in_line {
            if c == '\n' {
                in_line = false;
                out.push(c);
            }
            continue;
        }
        if let Some(quote) = in_string {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == quote {
                in_string = None;
            }
            continue;
        }
        match c {
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                in_block = true;
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                in_line = true;
            }
            '\'' | '"' | '`' => {
                in_string = Some(c);
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Does a registration block's factory take the entity manager? Either it
/// destructures `EntityMgr` from its first argument or it resolves it by
/// token. A comment mentioning the manager does not count, and neither does
/// a per-tenant factory that forks from `Orm` itself.
fn block_takes_entity_manager(body: &str) -> bool {
    let code = strip_comments(body);
    if code.contains("resolve!('EntityMgr'")
        || code.contains("resolve('EntityMgr'")
        || code.contains("resolve!(\"EntityMgr\"")
    {
        return true;
    }
    let Some(idx) = code.find("factory:") else {
        return false;
    };
    let after = &code[idx..];
    let Some(open) = after.find('(') else {
        return false;
    };
    let Some(close) = after[open..].find(')') else {
        return false;
    };
    let args = &after[open..open + close];
    args.split(',').any(|arg| {
        arg.trim()
            .trim_matches(|c| c == '{' || c == '}' || c == ' ')
            == "EntityMgr"
    })
}

/// Tokens whose registration in `registrations.ts` takes the entity manager:
/// resolving one of these with no tenant context hands the service an
/// unbound manager. The `EntityMgr` token itself always counts.
pub(crate) fn em_dependent_tokens(registrations: &str) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    tokens.insert("EntityMgr".to_string());
    // Registration blocks look like `\n  Name: {` at two-space indent and run
    // until the next such block.
    let mut current: Option<(String, String)> = None;
    for line in registrations.lines() {
        let trimmed = line.trim_end();
        let is_block_start = trimmed.starts_with("  ")
            && !trimmed.starts_with("   ")
            && trimmed.ends_with(": {")
            && trimmed[2..trimmed.len() - 3]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_');
        if is_block_start {
            if let Some((name, body)) = current.take() {
                if block_takes_entity_manager(&body) {
                    tokens.insert(name);
                }
            }
            let name = trimmed[2..trimmed.len() - 3].to_string();
            current = Some((name, String::new()));
        } else if let Some((_, body)) = current.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some((name, body)) = current.take() {
        if block_takes_entity_manager(&body) {
            tokens.insert(name);
        }
    }
    tokens
}

/// `const NAME = <anything>.scopedResolver(tokens.TOKEN)` bindings in a file,
/// keeping only those whose token depends on the entity manager.
fn em_resolver_names(source: &str, em_tokens: &BTreeSet<String>) -> Vec<String> {
    let mut names = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("const ") else {
            continue;
        };
        let Some((name, expr)) = rest.split_once('=') else {
            continue;
        };
        let expr = expr.trim();
        let Some(idx) = expr.find(".scopedResolver(") else {
            continue;
        };
        let token_expr = &expr[idx + ".scopedResolver(".len()..];
        let token = token_expr
            .trim_start_matches("tokens.")
            .trim_start_matches('\'')
            .trim_start_matches('"')
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>();
        if em_tokens.contains(&token) {
            names.push(name.trim().to_string());
        }
    }
    names
}

/// The text of a call starting at `line_index`, joined with following lines
/// until its parentheses balance (bounded), so multi-line contexts are seen.
fn call_window(lines: &[&str], line_index: usize) -> String {
    let mut window = String::new();
    let mut depth: i32 = 0;
    let mut started = false;
    for line in lines.iter().skip(line_index).take(8) {
        window.push_str(line);
        window.push(' ');
        for c in line.chars() {
            match c {
                '(' => {
                    depth += 1;
                    started = true;
                }
                ')' => depth -= 1,
                _ => {}
            }
        }
        if started && depth <= 0 {
            break;
        }
    }
    window
}

fn has_call_without_tenant(window: &str, name: &str) -> bool {
    let needle = format!("{name}(");
    let mut search = window;
    while let Some(idx) = search.find(&needle) {
        let before_ok = idx == 0
            || !search[..idx]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.');
        let after = &search[idx + needle.len()..];
        if before_ok {
            // `name()` and `name({ context: { entityManagerOptions } })` alike:
            // anything that does not name a tenantId is unbound. Find the
            // matching close paren.
            let mut depth = 1;
            let mut end = after.len();
            for (i, c) in after.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if !after[..end].contains("tenantId") {
                return true;
            }
        }
        search = &search[idx + needle.len()..];
    }
    false
}

fn previous_line_allows<'a>(lines: &[&'a str], index: usize) -> Option<&'a str> {
    if index == 0 {
        return None;
    }
    let previous = lines[index - 1].trim();
    let idx = previous.find(ALLOW_MARKER)?;
    let reason = previous[idx + ALLOW_MARKER.len()..].trim();
    Some(reason)
}

/// Scan one file. `em_tokens` are the registration tokens that carry an
/// entity manager (from `em_dependent_tokens`).
pub(crate) fn scan_source(
    display_path: &str,
    source: &str,
    em_tokens: &BTreeSet<String>,
) -> (Vec<TenancyFinding>, Vec<TenancyExemption>) {
    let mut findings = Vec::new();
    let mut exemptions = Vec::new();
    let resolver_names = em_resolver_names(source, em_tokens);
    let lines: Vec<&str> = source.lines().collect();

    let report = |rule: &'static str,
                  severity: TenancySeverity,
                  index: usize,
                  message: &'static str,
                  findings: &mut Vec<TenancyFinding>,
                  exemptions: &mut Vec<TenancyExemption>| {
        if let Some(reason) = previous_line_allows(&lines, index) {
            exemptions.push(TenancyExemption {
                rule,
                file: display_path.to_string(),
                line: index + 1,
                reason: if reason.is_empty() {
                    "(no reason given)".to_string()
                } else {
                    reason.to_string()
                },
            });
            return;
        }
        findings.push(TenancyFinding {
            severity,
            rule,
            file: display_path.to_string(),
            line: index + 1,
            snippet: lines[index].trim().chars().take(140).collect(),
            message,
        });
    };

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }

        if EMPTY_TENANT_PATTERNS.iter().any(|p| line.contains(p)) {
            report(
                RULE_EMPTY_TENANT,
                TenancySeverity::Error,
                index,
                "binds the empty tenant: rows written here decrypt nowhere a real tenant is bound, and reads here cannot open any tenant's rows. Bind the owning organization, the user, or the platform tenant.",
                &mut findings,
                &mut exemptions,
            );
            continue;
        }

        if line.contains("wrapEmWithTenantContext(")
            && (line.contains(", '')") || line.contains(", \"\")"))
        {
            report(
                RULE_EMPTY_TENANT,
                TenancySeverity::Error,
                index,
                "wraps an entity manager with the empty tenant; bind a real tenant instead.",
                &mut findings,
                &mut exemptions,
            );
            continue;
        }

        if resolver_names.iter().any(|name| {
            line.contains(&format!("{name}("))
                && has_call_without_tenant(&call_window(&lines, index), name)
        }) {
            report(
                RULE_UNBOUND_RESOLVE,
                TenancySeverity::Error,
                index,
                "resolves an entity-manager-backed service with no tenantId in its context: the manager it gets is unbound. Pass { context: { tenantId } }, or locate the row through plaintext columns first (locatorEm + fields) and bind to what it says.",
                &mut findings,
                &mut exemptions,
            );
            continue;
        }

        if line.contains("getSuperAdminContext(") {
            report(
                RULE_SUPER_ADMIN_READ,
                TenancySeverity::Warning,
                index,
                "getSuperAdminContext returns the raw manager: the tenant filter is off and the encryption tenant is whatever is ambient (usually none). Reads through it that hydrate an encrypted column fail or silently miss; writes land under no key.",
                &mut findings,
                &mut exemptions,
            );
            continue;
        }

        let looks_like_prose = trimmed.starts_with('`')
            || trimmed.starts_with('\'')
            || trimmed.starts_with('"')
            || trimmed.starts_with('-');
        if line.contains(".fork(")
            && !looks_like_prose
            && !line.contains("wrapEmWithTenantContext(")
            && !line.contains("Orm.em.fork(")
            && !line.contains("orm.em.fork(")
        {
            report(
                RULE_UNBOUND_FORK,
                TenancySeverity::Warning,
                index,
                "a fork of a tenant-bound entity manager is not bound: it keeps the filter params but not the encryption tenant. Wrap the fork with wrapEmWithTenantContext(fork, tenantId), or fork from the ORM and bind the result.",
                &mut findings,
                &mut exemptions,
            );
        }
    }

    (findings, exemptions)
}

/// Scan every project under `modules_path`.
pub(crate) fn audit_tenancy(modules_path: &Path) -> Result<TenancyReport> {
    let mut report = TenancyReport::default();
    if !modules_path.exists() {
        return Ok(report);
    }
    let mut projects: Vec<PathBuf> = fs::read_dir(modules_path)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    projects.sort();
    for project_path in projects {
        let registrations =
            fs::read_to_string(project_path.join("registrations.ts")).unwrap_or_default();
        let em_tokens = em_dependent_tokens(&registrations);
        let mut files = Vec::new();
        walk(&project_path, &mut files);
        for file in files {
            let Ok(source) = fs::read_to_string(&file) else {
                continue;
            };
            report.files_scanned += 1;
            let display = file
                .strip_prefix(modules_path)
                .unwrap_or(&file)
                .to_string_lossy()
                .to_string();
            let (findings, exemptions) = scan_source(&display, &source, &em_tokens);
            report.findings.extend(findings);
            report.exemptions.extend(exemptions);
        }
    }
    report.findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    Ok(report)
}

#[derive(Debug)]
pub(crate) struct AuditTenancyCommand;

impl AuditTenancyCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for AuditTenancyCommand {
    fn command(&self) -> Command {
        command(
            "audit-tenancy",
            "Statically check that every read and write of an encrypted column binds the row's tenant (no empty tenant, no unbound entity managers). Exit 1 on errors; run it in CI.",
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("The application path"),
        )
        .arg(
            Arg::new("strict")
                .long("strict")
                .help("Treat warnings as errors")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("baseline")
                .long("baseline")
                .help("Baseline file of known findings (default: .forklaunch/tenancy-baseline.json when it exists). Findings it lists are reported but do not fail the run; new ones do.")
                .value_name("FILE"),
        )
        .arg(
            Arg::new("write_baseline")
                .long("write-baseline")
                .help("Write every current finding to the baseline file and exit 0. Use once to adopt the check; burn the file down from there.")
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
        let (app_root, manifest) = require_manifest(matches)?;
        let modules_path = app_root.join(&manifest.modules_path);
        let mut report = audit_tenancy(&modules_path)?;
        let strict = matches.get_flag("strict");

        let baseline_path = matches
            .get_one::<String>("baseline")
            .map(PathBuf::from)
            .unwrap_or_else(|| app_root.join(".forklaunch").join("tenancy-baseline.json"));
        if matches.get_flag("write_baseline") {
            let baseline = TenancyBaseline::from_findings(&report.findings);
            if let Some(parent) = baseline_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(
                &baseline_path,
                serde_json::to_string_pretty(&baseline)? + "\n",
            )?;
            let mut stdout = StandardStream::stdout(ColorChoice::Auto);
            log_ok!(
                stdout,
                "Wrote {} finding(s) to {}",
                baseline.entries.len(),
                baseline_path.display()
            );
            return Ok(());
        }
        if baseline_path.exists() {
            let baseline: TenancyBaseline =
                serde_json::from_str(&fs::read_to_string(&baseline_path)?)?;
            let (fresh, known) = apply_baseline(std::mem::take(&mut report.findings), &baseline);
            report.findings = fresh;
            report.baselined = known;
        }

        let errors = report
            .findings
            .iter()
            .filter(|f| f.severity == TenancySeverity::Error)
            .count();
        let warnings = report.findings.len() - errors;
        let failing = errors > 0 || (strict && warnings > 0);

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            let mut stdout = StandardStream::stdout(ColorChoice::Auto);
            log_info!(
                stdout,
                "Scanned {} production TypeScript file(s) under {}",
                report.files_scanned,
                modules_path.display()
            );
            for finding in &report.findings {
                match finding.severity {
                    TenancySeverity::Error => log_error!(
                        stdout,
                        "{}:{} [{}] {}\n    {}",
                        finding.file,
                        finding.line,
                        finding.rule,
                        finding.snippet,
                        finding.message
                    ),
                    TenancySeverity::Warning => log_warn!(
                        stdout,
                        "{}:{} [{}] {}\n    {}",
                        finding.file,
                        finding.line,
                        finding.rule,
                        finding.snippet,
                        finding.message
                    ),
                }
            }
            if !report.baselined.is_empty() {
                log_info!(
                    stdout,
                    "{} known finding(s) in the baseline ({}) are not failing this run",
                    report.baselined.len(),
                    baseline_path.display()
                );
            }
            for exemption in &report.exemptions {
                log_info!(
                    stdout,
                    "{}:{} [{}] allowed: {}",
                    exemption.file,
                    exemption.line,
                    exemption.rule,
                    exemption.reason
                );
            }
            if report.findings.is_empty() {
                log_ok!(
                    stdout,
                    "No new unbound tenant access found ({} baselined, {} exemption(s))",
                    report.baselined.len(),
                    report.exemptions.len()
                );
            } else {
                writeln!(stdout)?;
                log_warn!(
                    stdout,
                    "{} error(s), {} warning(s), {} exemption(s)",
                    errors,
                    warnings,
                    report.exemptions.len()
                );
            }
        }

        if failing {
            std::process::exit(1);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRATIONS: &str = r#"
const serviceDependencies = runtimeDependencies.chain({
  EntityMgr: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ Orm }, context?: { tenantId?: string }) =>
      wrapEmWithTenantContext(Orm.em.fork(), context?.tenantId)
  },
  BillingCacheService: {
    lifetime: Lifetime.Singleton,
    type: type<BillingCacheService>(),
    factory: ({ TtlCache }) => createBillingCacheService(TtlCache)
  },
  PlanService: {
    lifetime: Lifetime.Scoped,
    type: StripePlanService,
    factory: ({ StripeClient, EntityMgr, OtelCollector }) =>
      new StripePlanService(StripeClient, EntityMgr, OtelCollector)
  },
  FeatureFlagService: {
    lifetime: Lifetime.Scoped,
    type: FeatureFlagService,
    factory: ({ Orm, BillingCacheService }) =>
      new FeatureFlagService((tenantId) => wrapEmWithTenantContext(Orm.em.fork(), tenantId), BillingCacheService)
  }
});
"#;

    fn tokens() -> BTreeSet<String> {
        em_dependent_tokens(REGISTRATIONS)
    }

    #[test]
    fn registration_tokens_that_carry_the_entity_manager() {
        let tokens = tokens();
        assert!(tokens.contains("EntityMgr"));
        assert!(tokens.contains("PlanService"));
        assert!(!tokens.contains("BillingCacheService"));
        // Takes a per-tenant factory, not the scoped manager.
        assert!(!tokens.contains("FeatureFlagService"));
    }

    #[test]
    fn flags_the_empty_tenant_and_unbound_resolves() {
        let source = r#"
const emFactory = ci.scopedResolver(tokens.EntityMgr);
const serviceFactory = ci.scopedResolver(tokens.PlanService);
const cacheFactory = ci.scopedResolver(tokens.BillingCacheService);

const a = emFactory({ context: { tenantId: '' } });
const b = emFactory();
const c = serviceFactory().listPlans();
const d = cacheFactory();
const e = emFactory({ context: { tenantId: organizationId } });
const f = serviceFactory({ context: { tenantId: PLATFORM_TENANT_ID } });
const g = emFactory({ context: { entityManagerOptions: {} } });
"#;
        let (findings, exemptions) = scan_source("billing/x.ts", source, &tokens());
        assert!(exemptions.is_empty());
        let lines: Vec<(usize, &str)> = findings.iter().map(|f| (f.line, f.rule)).collect();
        assert_eq!(
            lines,
            vec![
                (6, RULE_EMPTY_TENANT),
                (7, RULE_UNBOUND_RESOLVE),
                (8, RULE_UNBOUND_RESOLVE),
                (12, RULE_UNBOUND_RESOLVE),
            ]
        );
        assert!(
            findings
                .iter()
                .all(|f| f.severity == TenancySeverity::Error)
        );
    }

    #[test]
    fn a_context_spread_over_several_lines_counts_as_bound() {
        let source = r#"
const serviceFactory = ci.scopedResolver(tokens.PlanService);
const bound = await serviceFactory({
  context: { tenantId: PLATFORM_TENANT_ID }
}).listPlans();
const unbound = await serviceFactory({
  context: { entityManagerOptions: { clear: true } }
}).listPlans();
"#;
        let (findings, _) = scan_source("billing/x.ts", source, &tokens());
        let lines: Vec<usize> = findings.iter().map(|f| f.line).collect();
        assert_eq!(lines, vec![6]);
    }

    #[test]
    fn prose_about_the_entity_manager_does_not_make_a_token_depend_on_it() {
        let registrations = r#"
const deps = base.chain({
  Thing: {
    lifetime: Lifetime.Scoped,
    type: Thing,
    // a scoped resolve of EntityMgr hands back the cached instance, so this
    // service forks from the ORM instead
    factory: ({ Orm }) => new Thing((t) => wrapEmWithTenantContext(Orm.em.fork(), t))
  },
  Other: {
    lifetime: Lifetime.Scoped,
    type: Other,
    factory: ({ OtelCollector }, context, resolve) =>
      new Other(resolve!('EntityMgr', context), OtelCollector)
  }
});
"#;
        let tokens = em_dependent_tokens(registrations);
        assert!(!tokens.contains("Thing"));
        assert!(tokens.contains("Other"));
    }

    #[test]
    fn the_baseline_hides_known_findings_and_surfaces_new_ones() {
        let known = TenancyFinding {
            severity: TenancySeverity::Error,
            rule: RULE_UNBOUND_RESOLVE,
            file: "iam/a.ts".to_string(),
            line: 10,
            snippet: "const em = emFactory();".to_string(),
            message: "",
        };
        let moved = TenancyFinding {
            line: 42,
            ..known.clone()
        };
        let fresh = TenancyFinding {
            file: "iam/b.ts".to_string(),
            ..known.clone()
        };
        let baseline = TenancyBaseline::from_findings(&[known.clone()]);
        let (new, old) = apply_baseline(vec![moved.clone(), fresh.clone()], &baseline);
        assert_eq!(new, vec![fresh]);
        assert_eq!(old, vec![moved]);
    }

    #[test]
    fn an_allow_comment_turns_a_finding_into_an_exemption() {
        let source = r#"
const emFactory = ci.scopedResolver(tokens.EntityMgr);
// forklaunch-tenancy: allow locating rows through plaintext columns only
const locator = emFactory();
"#;
        let (findings, exemptions) = scan_source("core/x.ts", source, &tokens());
        assert!(findings.is_empty());
        assert_eq!(exemptions.len(), 1);
        assert_eq!(exemptions[0].line, 4);
        assert_eq!(
            exemptions[0].reason,
            "locating rows through plaintext columns only"
        );
    }

    #[test]
    fn warns_on_super_admin_reads_and_unwrapped_forks() {
        let source = r#"
const raw = getSuperAdminContext(em);
const fork = em.fork();
const bound = wrapEmWithTenantContext(em.fork(), tenantId);
const fromOrm = Orm.em.fork();
"#;
        let (findings, _) = scan_source("iam/x.ts", source, &tokens());
        let rules: Vec<(usize, &str)> = findings.iter().map(|f| (f.line, f.rule)).collect();
        assert_eq!(
            rules,
            vec![(2, RULE_SUPER_ADMIN_READ), (3, RULE_UNBOUND_FORK)]
        );
        assert!(
            findings
                .iter()
                .all(|f| f.severity == TenancySeverity::Warning)
        );
    }

    #[test]
    fn comments_and_docblocks_are_not_scanned() {
        let source = r#"
// const em = emFactory({ context: { tenantId: '' } });
/**
 * tenantId: '' used to be the global tenant.
 */
const emFactory = ci.scopedResolver(tokens.EntityMgr);
"#;
        let (findings, _) = scan_source("x.ts", source, &tokens());
        assert!(findings.is_empty());
    }
}
