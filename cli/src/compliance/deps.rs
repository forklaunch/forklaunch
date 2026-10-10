//! Known-vulnerable and malicious dependencies, read from the app's lockfiles.
//!
//! Every package version pinned by a lockfile (npm, pnpm, yarn, bun; pip
//! requirements pins, poetry, uv, Pipfile) is looked up in OSV, the database
//! behind `osv-scanner` that merges the GitHub advisories, the PyPA advisories
//! and the OpenSSF malicious-packages feed. A version is either listed as
//! affected or it is not, so the check is exact for what the database knows.
//!
//! Lookups go to `api.osv.dev` and are cached under `~/.forklaunch/cache/osv`
//! for a day. When the lookup cannot complete (offline, OSV unreachable,
//! `FORKLAUNCH_NO_ADVISORY_LOOKUP=1`, or the offline card with nothing cached)
//! the scan says so with `dependency-scan-incomplete` rather than reporting a
//! clean result it never checked.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::checks::{LocalFinding, Severity};

const OSV_API: &str = "https://api.osv.dev/v1";
/// OSV accepts up to 1,000 queries per batch.
const BATCH: usize = 1000;
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    ".next",
    ".turbo",
    "coverage",
    ".venv",
    "venv",
    "__pycache__",
];

/// How the scan may reach the advisory database.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Lookup {
    /// Ask OSV for anything not cached in the last day.
    Network,
    /// Use only what is cached (the offline card promises no network).
    CacheOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Package {
    pub(crate) ecosystem: &'static str,
    pub(crate) name: String,
    pub(crate) version: String,
    /// Lockfile it was read from, relative to the app root.
    pub(crate) file: String,
}

/// One lockfile's packages, and whether a lockfile exists at all.
pub(crate) struct Inventory {
    pub(crate) packages: Vec<Package>,
    pub(crate) lockfiles: Vec<String>,
    /// A `package.json` or `pyproject.toml` at the app root with no lockfile beside it.
    pub(crate) unpinned_manifest: Option<String>,
}

fn walk(root: &Path, names: &[&str]) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .max_depth(6)
        .into_iter()
        .filter_entry(|e| {
            !(e.file_type().is_dir()
                && SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()))
        })
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && names.iter().any(|n| {
                    let f = e.file_name().to_string_lossy();
                    if let Some(prefix) = n.strip_suffix('*') {
                        f.starts_with(prefix) && f.ends_with(".txt")
                    } else {
                        f == *n
                    }
                })
        })
        .map(|e| e.into_path())
        .collect()
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_version(v: &str) -> bool {
    v.chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn pypi_name(n: &str) -> String {
    let mut out = String::new();
    let mut sep = false;
    for c in n.to_lowercase().chars() {
        if c == '-' || c == '_' || c == '.' {
            sep = true;
        } else {
            if sep && !out.is_empty() {
                out.push('-');
            }
            sep = false;
            out.push(c);
        }
    }
    out
}

/// `@scope/name@1.2.3(peer@x)` / `/name@1.2.3` / `/name/1.2.3` (pnpm v5) → (name, version).
fn pnpm_key(line: &str) -> Option<(String, String)> {
    if !line.starts_with("  ") || line.starts_with("   ") || !line.trim_end().ends_with(':') {
        return None;
    }
    let key = line
        .trim()
        .trim_end_matches(':')
        .trim_matches(|c| c == '\'' || c == '"');
    let key = key.strip_prefix('/').unwrap_or(key);
    let key = key.split('(').next()?;
    let scoped = key.starts_with('@');
    let body = if scoped { &key[1..] } else { key };
    // v6+: name@version; v5: name/version
    let (name, version) = match body.rfind('@') {
        Some(i) => (&body[..i], &body[i + 1..]),
        None => {
            let i = body.rfind('/')?;
            (&body[..i], &body[i + 1..])
        }
    };
    if !is_version(version) || name.is_empty() {
        return None;
    }
    Some((
        format!("{}{}", if scoped { "@" } else { "" }, name),
        version.to_string(),
    ))
}

/// The yarn.lock entry header's package name: `"@scope/a@^1", "@scope/a@^2":` → `@scope/a`.
fn yarn_name(header: &str) -> Option<String> {
    let first = header
        .split(',')
        .next()?
        .trim()
        .trim_end_matches(':')
        .trim_matches('"');
    let at = if let Some(rest) = first.strip_prefix('@') {
        rest.find('@').map(|i| i + 1)?
    } else {
        first.find('@')?
    };
    let name = &first[..at];
    (!name.is_empty()).then(|| name.to_string())
}

pub(crate) fn inventory(root: &Path) -> Inventory {
    let mut packages = Vec::new();
    let mut lockfiles = BTreeSet::new();
    let mut add = |eco: &'static str, name: String, version: String, file: &Path| {
        if !name.is_empty() && is_version(&version) {
            packages.push(Package {
                ecosystem: eco,
                name,
                version,
                file: rel(root, file),
            });
        }
    };

    for f in walk(root, &["package-lock.json", "npm-shrinkwrap.json"]) {
        lockfiles.insert(rel(root, &f));
        let Ok(d) = fs::read_to_string(&f)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .ok_or(())
        else {
            continue;
        };
        if let Some(pkgs) = d.get("packages").and_then(Value::as_object) {
            for (path, v) in pkgs {
                if path.is_empty() || v.get("link").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                let name = v
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        path.rsplit("node_modules/")
                            .next()
                            .unwrap_or(path)
                            .to_string()
                    });
                let version = v.get("version").and_then(Value::as_str).unwrap_or("");
                add("npm", name, version.to_string(), &f);
            }
        } else {
            fn deps(d: &Value, out: &mut Vec<(String, String)>) {
                if let Some(m) = d.get("dependencies").and_then(Value::as_object) {
                    for (n, v) in m {
                        if let Some(ver) = v.get("version").and_then(Value::as_str) {
                            out.push((n.clone(), ver.to_string()));
                        }
                        deps(v, out);
                    }
                }
            }
            let mut out = Vec::new();
            deps(&d, &mut out);
            for (n, v) in out {
                add("npm", n, v, &f);
            }
        }
    }

    for f in walk(root, &["pnpm-lock.yaml"]) {
        lockfiles.insert(rel(root, &f));
        let text = fs::read_to_string(&f).unwrap_or_default();
        // Only the `packages:` (and v9 `snapshots:`) sections name resolved versions.
        let mut in_packages = false;
        for line in text.lines() {
            if !line.starts_with(' ') && !line.is_empty() {
                in_packages = line.starts_with("packages:") || line.starts_with("snapshots:");
                continue;
            }
            if in_packages && let Some((n, v)) = pnpm_key(line) {
                add("npm", n, v, &f);
            }
        }
    }

    for f in walk(root, &["yarn.lock"]) {
        lockfiles.insert(rel(root, &f));
        let text = fs::read_to_string(&f).unwrap_or_default();
        let mut name: Option<String> = None;
        for line in text.lines() {
            if !line.is_empty() && !line.starts_with(' ') && !line.starts_with('#') {
                // Workspace and local entries are the app's own code, not published packages.
                let local = ["@workspace:", "@link:", "@portal:", "@file:"]
                    .iter()
                    .any(|m| line.contains(m));
                name = if local { None } else { yarn_name(line) };
                continue;
            }
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("version") {
                let v = rest
                    .trim_start_matches(':')
                    .trim()
                    .trim_matches('"')
                    .to_string();
                if let Some(n) = name.take() {
                    add("npm", n, v, &f);
                }
            }
        }
    }

    for f in walk(root, &["bun.lock"]) {
        lockfiles.insert(rel(root, &f));
        let text = fs::read_to_string(&f).unwrap_or_default();
        // "key": ["name@1.2.3", ...]
        for line in text.lines() {
            let t = line.trim_start();
            let Some(i) = t.find(": [\"") else { continue };
            if !t.starts_with('"') {
                continue;
            }
            let spec = &t[i + 4..];
            let Some(end) = spec.find('"') else { continue };
            let spec = &spec[..end];
            let at = if let Some(rest) = spec.strip_prefix('@') {
                rest.find('@').map(|j| j + 1)
            } else {
                spec.find('@')
            };
            if let Some(at) = at {
                add(
                    "npm",
                    spec[..at].to_string(),
                    spec[at + 1..].to_string(),
                    &f,
                );
            }
        }
    }

    for f in walk(root, &["requirements*"]) {
        lockfiles.insert(rel(root, &f));
        for line in fs::read_to_string(&f).unwrap_or_default().lines() {
            let line = line
                .split('#')
                .next()
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("");
            if let Some((n, v)) = line.split_once("==") {
                let n = n.split('[').next().unwrap_or("").trim();
                let v = v.split_whitespace().next().unwrap_or("");
                if !n.is_empty()
                    && n.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
                {
                    add("PyPI", pypi_name(n), v.to_string(), &f);
                }
            }
        }
    }

    for f in walk(root, &["poetry.lock", "uv.lock"]) {
        lockfiles.insert(rel(root, &f));
        let mut name: Option<String> = None;
        for line in fs::read_to_string(&f).unwrap_or_default().lines() {
            let quoted = |l: &str| l.split('"').nth(1).map(str::to_string);
            if line.starts_with("name = ") {
                name = quoted(line).map(|n| pypi_name(&n));
            } else if line.starts_with("version = ")
                && let (Some(n), Some(v)) = (name.take(), quoted(line))
            {
                add("PyPI", n, v, &f);
            }
        }
    }

    for f in walk(root, &["Pipfile.lock"]) {
        lockfiles.insert(rel(root, &f));
        let Some(d) = fs::read_to_string(&f)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            continue;
        };
        for sec in ["default", "develop"] {
            if let Some(m) = d.get(sec).and_then(Value::as_object) {
                for (n, v) in m {
                    let ver = v
                        .get("version")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim_start_matches('=');
                    add("PyPI", pypi_name(n), ver.to_string(), &f);
                }
            }
        }
    }

    packages.sort();
    packages
        .dedup_by(|a, b| a.ecosystem == b.ecosystem && a.name == b.name && a.version == b.version);

    let has_root_lock = lockfiles.iter().any(|l| !l.contains('/'));
    let unpinned_manifest = if has_root_lock {
        None
    } else {
        ["package.json", "pyproject.toml"]
            .into_iter()
            .find(|m| root.join(m).is_file())
            .map(str::to_string)
    };
    Inventory {
        packages,
        lockfiles: lockfiles.into_iter().collect(),
        unpinned_manifest,
    }
}

// ─── OSV ────────────────────────────────────────────────────────────────────

fn default_cache() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".forklaunch").join("cache").join("osv"))
}

/// Where lookups are cached and how the database may be reached.
struct Osv {
    lookup: Lookup,
    cache: Option<PathBuf>,
    client: reqwest::blocking::Client,
}

fn cache_path(cache: &Option<PathBuf>, kind: &str, key: &str) -> Option<PathBuf> {
    let digest = Sha256::digest(key.as_bytes());
    let name: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
    cache
        .as_ref()
        .map(|d| d.join(kind).join(format!("{name}.json")))
}

fn read_cache(
    cache: &Option<PathBuf>,
    kind: &str,
    key: &str,
    ttl: Option<Duration>,
) -> Option<Value> {
    let p = cache_path(cache, kind, key)?;
    if let Some(ttl) = ttl {
        let age = fs::metadata(&p)
            .ok()?
            .modified()
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())?;
        if age > ttl {
            return None;
        }
    }
    serde_json::from_str(&fs::read_to_string(p).ok()?).ok()
}

fn write_cache(cache: &Option<PathBuf>, kind: &str, key: &str, v: &Value) {
    if let Some(p) = cache_path(cache, kind, key) {
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(p, v.to_string());
    }
}

fn query_key(p: &Package) -> String {
    format!("{}/{}@{}", p.ecosystem, p.name, p.version)
}

#[derive(Deserialize)]
struct BatchResponse {
    results: Vec<BatchResult>,
}
#[derive(Deserialize, Default)]
struct BatchResult {
    #[serde(default)]
    vulns: Vec<BatchVuln>,
}
#[derive(Deserialize)]
struct BatchVuln {
    id: String,
}

/// Advisory ids per package; `None` when the lookup could not complete.
fn advisory_ids(pkgs: &[Package], osv: &Osv) -> Option<Vec<Vec<String>>> {
    let mut ids: Vec<Option<Vec<String>>> = pkgs
        .iter()
        .map(|p| {
            read_cache(&osv.cache, "q", &query_key(p), Some(CACHE_TTL))
                .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        })
        .collect();
    let missing: Vec<usize> = (0..pkgs.len()).filter(|&i| ids[i].is_none()).collect();
    if !missing.is_empty() && osv.lookup == Lookup::CacheOnly {
        return None;
    }
    for chunk in missing.chunks(BATCH) {
        let queries: Vec<Value> = chunk
            .iter()
            .map(|&i| {
                json!({
                    "package": { "ecosystem": pkgs[i].ecosystem, "name": pkgs[i].name },
                    "version": pkgs[i].version
                })
            })
            .collect();
        let resp: BatchResponse = osv
            .client
            .post(format!("{OSV_API}/querybatch"))
            .json(&json!({ "queries": queries }))
            .send()
            .ok()?
            .error_for_status()
            .ok()?
            .json()
            .ok()?;
        if resp.results.len() != chunk.len() {
            return None;
        }
        for (&i, r) in chunk.iter().zip(resp.results) {
            let list: Vec<String> = r.vulns.into_iter().map(|v| v.id).collect();
            write_cache(&osv.cache, "q", &query_key(&pkgs[i]), &json!(list));
            ids[i] = Some(list);
        }
    }
    ids.into_iter().collect()
}

fn advisory(id: &str, osv: &Osv) -> Option<Value> {
    if let Some(v) = read_cache(&osv.cache, "v", id, Some(CACHE_TTL * 7)) {
        return Some(v);
    }
    if osv.lookup == Lookup::CacheOnly {
        return None;
    }
    let v: Value = osv
        .client
        .get(format!("{OSV_API}/vulns/{id}"))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    write_cache(&osv.cache, "v", id, &v);
    Some(v)
}

/// Numeric-dotted comparison, enough to pick the first fixed version above the installed one.
fn version_key(v: &str) -> Vec<u64> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .take(4)
        .map(|s| s.parse().unwrap_or(0))
        .collect()
}

/// (severity word, first fixed version above `version`) for this package in an OSV entry.
fn describe(entry: &Value, pkg: &Package) -> (String, Option<String>) {
    let mut severity = entry
        .pointer("/database_specific/severity")
        .and_then(Value::as_str)
        .map(str::to_uppercase);
    let mut fixed: Vec<String> = Vec::new();
    for a in entry
        .get("affected")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let same = a.pointer("/package/ecosystem").and_then(Value::as_str) == Some(pkg.ecosystem)
            && a.pointer("/package/name").and_then(Value::as_str).map(|n| {
                if pkg.ecosystem == "PyPI" {
                    pypi_name(n)
                } else {
                    n.to_string()
                }
            }) == Some(pkg.name.clone());
        if !same {
            continue;
        }
        if severity.is_none() {
            severity = a
                .pointer("/database_specific/severity")
                .or_else(|| a.pointer("/ecosystem_specific/severity"))
                .and_then(Value::as_str)
                .map(str::to_uppercase);
        }
        for r in a
            .get("ranges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for e in r
                .get("events")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(f) = e.get("fixed").and_then(Value::as_str) {
                    fixed.push(f.to_string());
                }
            }
        }
    }
    let installed = version_key(&pkg.version);
    fixed.sort_by_key(|f| version_key(f));
    let first_fix = fixed.into_iter().find(|f| version_key(f) > installed);
    (severity.unwrap_or_else(|| "UNRATED".to_string()), first_fix)
}

/// The dependency findings for an app. `project` names the app in each finding.
pub(crate) fn scan(root: &Path, project: &str, lookup: Lookup) -> Vec<LocalFinding> {
    scan_with(root, project, lookup, default_cache())
}

fn scan_with(
    root: &Path,
    project: &str,
    lookup: Lookup,
    cache: Option<PathBuf>,
) -> Vec<LocalFinding> {
    let inv = inventory(root);
    let mut findings = Vec::new();
    let finding = |severity, check: &str, subject: String, message: String| LocalFinding {
        severity,
        project: project.to_string(),
        check: check.to_string(),
        subject,
        message,
    };

    if let Some(manifest) = &inv.unpinned_manifest {
        findings.push(finding(
            Severity::Warning,
            "dependency-lockfile-missing",
            manifest.clone(),
            "No lockfile is committed beside it, so installs resolve whatever versions are newest that day and known-vulnerable versions cannot be ruled out. Commit the package manager's lockfile.".to_string(),
        ));
    }
    if inv.packages.is_empty() {
        return findings;
    }

    let incomplete = |why: &str| LocalFinding {
        severity: Severity::Warning,
        project: project.to_string(),
        check: "dependency-scan-incomplete".to_string(),
        subject: inv.lockfiles.join(", "),
        message: format!(
            "{} pinned package versions were not checked against the vulnerability database: {why}.",
            inv.packages.len()
        ),
    };
    if std::env::var("FORKLAUNCH_NO_ADVISORY_LOOKUP").is_ok_and(|v| v == "1") {
        findings.push(incomplete(
            "advisory lookup is turned off (FORKLAUNCH_NO_ADVISORY_LOOKUP=1)",
        ));
        return findings;
    }
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("forklaunch-cli/", env!("CARGO_PKG_VERSION")))
        .build()
    else {
        findings.push(incomplete("the HTTP client could not start"));
        return findings;
    };
    let osv = Osv {
        lookup,
        cache,
        client,
    };
    let Some(ids) = advisory_ids(&inv.packages, &osv) else {
        findings.push(incomplete(if lookup == Lookup::CacheOnly {
            "the offline card uses only cached advisory lookups; run `forklaunch compliance audit` online once to fill the cache"
        } else {
            "the OSV advisory database (api.osv.dev) could not be reached"
        }));
        return findings;
    };

    let mut seen = BTreeMap::new();
    for (pkg, ids) in inv.packages.iter().zip(ids) {
        for id in ids {
            if seen
                .insert((pkg.name.clone(), pkg.version.clone(), id.clone()), ())
                .is_some()
            {
                continue;
            }
            let subject = format!("{}@{} ({})", pkg.name, pkg.version, pkg.file);
            if id.starts_with("MAL-") {
                findings.push(finding(
                    Severity::Warning,
                    "dependency-malicious",
                    subject,
                    format!("{id}: this release is listed as malicious (OpenSSF malicious packages). Remove it, rotate any credentials the machines that installed it could reach, and pin a known-good version."),
                ));
                continue;
            }
            let Some(entry) = advisory(&id, &osv) else {
                findings.push(finding(
                    Severity::Warning,
                    "dependency-vulnerable",
                    subject,
                    format!("UNRATED {id}: listed as affected by a known vulnerability (details could not be fetched)."),
                ));
                continue;
            };
            let (severity, fix) = describe(&entry, pkg);
            let summary = entry
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("known vulnerability");
            let cve = entry
                .get("aliases")
                .and_then(Value::as_array)
                .and_then(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .find(|s| s.starts_with("CVE-"))
                })
                .map(|c| format!(" ({c})"))
                .unwrap_or_default();
            findings.push(finding(
                // Moderate and low advisories are reported but do not warn; anything else,
                // including an unrated one, does.
                if matches!(severity.as_str(), "MODERATE" | "MEDIUM" | "LOW") {
                    Severity::Info
                } else {
                    Severity::Warning
                },
                "dependency-vulnerable",
                subject,
                format!(
                    "{severity} {id}{cve}: {summary}. {}",
                    match fix {
                        Some(f) => format!("Fixed in {f}; upgrade to it or later."),
                        None => "No fixed version is published; replace the package or confirm the vulnerable code path is unused.".to_string(),
                    }
                ),
            ));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (p, c) in files {
            let path = dir.path().join(p);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, c).unwrap();
        }
        dir
    }

    fn names(inv: &Inventory) -> Vec<String> {
        inv.packages
            .iter()
            .map(|p| format!("{}:{}@{}", p.ecosystem, p.name, p.version))
            .collect()
    }

    #[test]
    fn reads_pnpm_v9_and_v6_keys_with_scopes_and_peers() {
        let dir = app(&[(
            "pnpm-lock.yaml",
            "lockfileVersion: '9.0'\n\nimporters:\n\n  .:\n    dependencies:\n      lodash:\n        specifier: ^4.17.20\n        version: 4.17.20\n\npackages:\n\n  lodash@4.17.20:\n    resolution: {integrity: sha512-x}\n\n  '@types/node@20.1.0':\n    resolution: {integrity: sha512-y}\n\n  /express/4.17.1:\n    resolution: {integrity: sha512-z}\n\nsnapshots:\n\n  '@mikro-orm/core@6.4.0(pg@8.11.0)':\n    dependencies:\n      pg: 8.11.0\n",
        )]);
        let inv = inventory(dir.path());
        assert_eq!(
            names(&inv),
            vec![
                "npm:@mikro-orm/core@6.4.0",
                "npm:@types/node@20.1.0",
                "npm:express@4.17.1",
                "npm:lodash@4.17.20",
            ]
        );
        assert_eq!(inv.unpinned_manifest, None);
    }

    #[test]
    fn reads_npm_yarn_bun_and_python_lockfiles() {
        let dir = app(&[
            (
                "package-lock.json",
                r#"{"lockfileVersion":3,"packages":{"":{"name":"app"},"node_modules/minimist":{"version":"1.2.5"},"node_modules/a/node_modules/@x/y":{"version":"2.0.0"},"node_modules/local":{"link":true}}}"#,
            ),
            (
                "web/yarn.lock",
                "# yarn lockfile v1\n\n\"@babel/core@^7.0.0\", \"@babel/core@^7.1.0\":\n  version \"7.1.0\"\n\nleft-pad@^1.0.0:\n  version \"1.3.0\"\n",
            ),
            (
                "svc/bun.lock",
                "{\n  \"packages\": {\n    \"zod\": [\"zod@3.22.0\", \"\", {}, \"sha512-x\"],\n    \"@hono/node-server\": [\"@hono/node-server@1.2.0\", \"\", {}, \"sha512-y\"],\n  }\n}\n",
            ),
            (
                "py/requirements.txt",
                "Django==3.2.0  # pinned\nrequests>=2\nPyYAML[extra]==5.3 ; python_version<'4'\n",
            ),
            (
                "py/poetry.lock",
                "[[package]]\nname = \"Jinja2\"\nversion = \"2.10\"\n",
            ),
        ]);
        let inv = inventory(dir.path());
        assert_eq!(
            names(&inv),
            vec![
                "PyPI:django@3.2.0",
                "PyPI:jinja2@2.10",
                "PyPI:pyyaml@5.3",
                "npm:@babel/core@7.1.0",
                "npm:@hono/node-server@1.2.0",
                "npm:@x/y@2.0.0",
                "npm:left-pad@1.3.0",
                "npm:minimist@1.2.5",
                "npm:zod@3.22.0",
            ]
        );
    }

    #[test]
    fn a_manifest_without_a_lockfile_is_unpinned() {
        let dir = app(&[
            ("package.json", "{}"),
            ("node_modules/x/package-lock.json", "{}"),
        ]);
        let inv = inventory(dir.path());
        assert_eq!(inv.unpinned_manifest.as_deref(), Some("package.json"));
        let findings = scan(dir.path(), "app", Lookup::CacheOnly);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check, "dependency-lockfile-missing");
    }

    #[test]
    fn nothing_cached_offline_is_reported_incomplete_not_clean() {
        let cache = tempfile::tempdir().unwrap();
        let dir = app(&[(
            "pnpm-lock.yaml",
            "lockfileVersion: '9.0'\n\npackages:\n\n  left-pad-never-cached@0.0.1:\n    resolution: {integrity: sha512-x}\n",
        )]);
        let findings = scan_with(
            dir.path(),
            "app",
            Lookup::CacheOnly,
            Some(cache.path().to_path_buf()),
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check, "dependency-scan-incomplete");
        assert!(findings[0].message.contains("offline card"));
    }

    #[test]
    fn picks_the_first_fixed_version_above_the_installed_one() {
        let entry = json!({
            "database_specific": {"severity": "HIGH"},
            "affected": [{
                "package": {"ecosystem": "npm", "name": "lodash"},
                "ranges": [{"type": "SEMVER", "events": [
                    {"introduced": "0"}, {"fixed": "4.17.21"},
                    {"introduced": "5.0.0"}, {"fixed": "5.0.1"}
                ]}]
            }]
        });
        let pkg = Package {
            ecosystem: "npm",
            name: "lodash".into(),
            version: "4.17.20".into(),
            file: "pnpm-lock.yaml".into(),
        };
        assert_eq!(
            describe(&entry, &pkg),
            ("HIGH".to_string(), Some("4.17.21".to_string()))
        );
    }
}
