//! GitHub CI, Dependabot and branch-protection files derived from the languages
//! actually present in an application. Each generated file records the hash of
//! its body; reconciliation rewrites a file only while that hash (or a known
//! earlier generated release) still matches, so customized files are preserved.
use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use ramhorns::{Content, Template};
use serde_json::Value as JsonValue;

use super::{template::get_file_contents, watermark::generate_watermark};
use crate::constants::BUN_VERSION;

const OWNERSHIP: &str = "forklaunch-sha256:";
const DEFAULT_RUST_TOOLCHAIN: &str = "1.98.0";
const NODE_VERSION: &str = "22";
const SMOKE_SCRIPT: &str = ".github/scripts/container-smoke.mjs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runtime {
    Node,
    Bun,
}

#[derive(Debug, PartialEq, Eq)]
struct Script {
    directory: String,
    name: &'static str,
}

#[derive(Debug)]
struct JsWorkspace {
    runtime: Runtime,
    directory: String,
    /// A modules directory that is its own pnpm workspace root has its own lockfile.
    nested_install: Option<String>,
    pinned_package_manager: bool,
    scripts: Vec<Script>,
}

#[derive(Debug)]
struct Shape {
    rust_toolchain: Option<String>,
    js: Option<JsWorkspace>,
    dockerfiles: Vec<String>,
}

impl Shape {
    fn jobs(&self) -> Vec<&'static str> {
        let mut jobs = Vec::new();
        if self.rust_toolchain.is_some() {
            jobs.push("Rust");
        }
        if self.js.is_some() {
            jobs.push("TypeScript");
        }
        if !self.dockerfiles.is_empty() {
            jobs.push("Docker");
        }
        jobs
    }
}

fn package_json(path: &Path) -> Option<JsonValue> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn has_script(package: &JsonValue, name: &str) -> bool {
    package["scripts"][name].is_string()
}

fn detect(root: &Path) -> Result<Shape> {
    let manifest: toml::Value = toml::from_str(
        &fs::read_to_string(root.join(".forklaunch/manifest.toml"))
            .context("Missing .forklaunch/manifest.toml")?,
    )?;
    let modules = manifest
        .get("modules_path")
        .and_then(toml::Value::as_str)
        .unwrap_or("src")
        .trim_end_matches('/')
        .to_owned();
    let runtime = match manifest.get("runtime").and_then(toml::Value::as_str) {
        Some("bun") => Runtime::Bun,
        _ => Runtime::Node,
    };

    let rust_toolchain = fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|source| source.parse::<toml::Table>().ok())
        .filter(|cargo| cargo.contains_key("workspace"))
        .map(|_| {
            fs::read_to_string(root.join("rust-toolchain.toml"))
                .ok()
                .and_then(|source| source.parse::<toml::Table>().ok())
                .and_then(|toolchain| {
                    toolchain
                        .get("toolchain")?
                        .get("channel")?
                        .as_str()
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| DEFAULT_RUST_TOOLCHAIN.to_owned())
        });

    // The install root is the workspace that owns the lockfile: the repository
    // root for Rust and mixed applications, the modules directory for
    // TypeScript applications whose root carries no package manifest.
    let root_package = package_json(&root.join("package.json"));
    let root_is_workspace = root_package.as_ref().is_some_and(|package| {
        root.join("pnpm-workspace.yaml").is_file() || package.get("workspaces").is_some()
    });
    let modules_package = package_json(&root.join(&modules).join("package.json"));
    let install = if root_is_workspace {
        root_package.map(|package| (".".to_owned(), package))
    } else {
        modules_package.clone().map(|package| (modules.clone(), package))
    };
    let js = install.map(|(directory, package)| {
        let mut scripts = Vec::new();
        if has_script(&package, "build") {
            scripts.push(Script { directory: directory.clone(), name: "build" });
        } else if has_script(&package, "sdk:typecheck") {
            scripts.push(Script { directory: directory.clone(), name: "sdk:typecheck" });
        }
        // Only scripts that exist are invoked, in the directory that defines them.
        for name in ["lint", "test"] {
            if has_script(&package, name) {
                scripts.push(Script { directory: directory.clone(), name });
            } else if directory != modules
                && modules_package.as_ref().is_some_and(|package| has_script(package, name))
            {
                scripts.push(Script { directory: modules.clone(), name });
            }
        }
        let nested_install = (directory != modules && root.join(&modules).join("pnpm-workspace.yaml").is_file())
            .then(|| modules.clone());
        JsWorkspace {
            runtime,
            pinned_package_manager: package["packageManager"].is_string(),
            directory,
            nested_install,
            scripts,
        }
    });

    let mut dockerfiles = fs::read_dir(root)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            (name == "Dockerfile" || name.starts_with("Dockerfile."))
                && !name.ends_with(".dockerignore")
        })
        .collect::<Vec<_>>();
    dockerfiles.sort();

    Ok(Shape {
        rust_toolchain,
        js,
        dockerfiles,
    })
}

fn rust_setup(toolchain: &str) -> String {
    format!(
        "      - name: Install Rust {toolchain}\n        run: rustup toolchain install {toolchain} --profile minimal --component rustfmt,clippy && rustup default {toolchain}\n      - uses: Swatinem/rust-cache@v2\n"
    )
}

const FRESHNESS: &str = "      - name: Generated sources are committed\n        run: git diff --exit-code && test -z \"$(git status --porcelain)\"\n";

fn run_step(name: &str, directory: &str, command: &str) -> String {
    let directory = if directory == "." {
        String::new()
    } else {
        format!("        working-directory: {directory}\n")
    };
    format!("      - name: {name}\n{directory}        run: {command}\n")
}

fn render_ci(shape: &Shape) -> String {
    let mut ci = String::from(
        "name: CI\non:\n  pull_request:\n    branches: [main]\n  push:\n    branches: [main]\n\npermissions:\n  contents: read\n\nconcurrency:\n  group: ci-${{ github.ref }}\n  cancel-in-progress: true\n\njobs:\n",
    );
    if let Some(toolchain) = &shape.rust_toolchain {
        ci.push_str("  rust:\n    name: Rust\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n");
        ci.push_str(&rust_setup(toolchain));
        // Not `--all`: that also formats path dependencies such as blueprint copies.
        ci.push_str(&run_step("Format", ".", "cargo fmt -- --check"));
        ci.push_str(&run_step("Lint", ".", "cargo clippy --workspace --all-targets --locked"));
        ci.push_str(&run_step("Test", ".", "cargo test --workspace --locked"));
        // Service build scripts regenerate OpenAPI, contracts and both SDKs.
        ci.push_str(FRESHNESS);
    }
    if let Some(js) = &shape.js {
        ci.push_str("  typescript:\n    name: TypeScript\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n");
        if let Some(toolchain) = &shape.rust_toolchain {
            // Mixed builds compile Rust contracts before TypeScript packages.
            ci.push_str(&rust_setup(toolchain));
        }
        let (install, run) = match js.runtime {
            Runtime::Node => {
                ci.push_str(&format!(
                    "      - uses: actions/setup-node@v4\n        with:\n          node-version: '{NODE_VERSION}'\n"
                ));
                ci.push_str(if js.pinned_package_manager {
                    "      - run: corepack enable\n"
                } else {
                    "      - run: npm install -g pnpm\n"
                });
                ("pnpm install --frozen-lockfile", "pnpm run")
            }
            Runtime::Bun => {
                // Same Bun as the generated image, so the frozen install in CI
                // and in the Docker build read the lockfile identically.
                ci.push_str(&format!(
                    "      - uses: oven-sh/setup-bun@v2\n        with:\n          bun-version: {BUN_VERSION}\n"
                ));
                ("bun install --frozen-lockfile", "bun run")
            }
        };
        ci.push_str(&run_step("Install", &js.directory, install));
        if let Some(nested) = &js.nested_install {
            ci.push_str(&run_step(&format!("Install {nested} workspace"), nested, install));
        }
        for script in &js.scripts {
            let name = match script.name {
                "build" => "Build",
                "sdk:typecheck" => "Type-check SDKs",
                "lint" => "Lint",
                _ => "Test",
            };
            ci.push_str(&run_step(name, &script.directory, &format!("{run} {}", script.name)));
        }
        ci.push_str(FRESHNESS);
    }
    if !shape.dockerfiles.is_empty() {
        ci.push_str(&format!(
            "  docker:\n    name: Docker\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/setup-node@v4\n        with:\n          node-version: '{NODE_VERSION}'\n"
        ));
        for dockerfile in &shape.dockerfiles {
            ci.push_str(&run_step(
                &format!("Build and start {dockerfile}"),
                ".",
                &format!("node {SMOKE_SCRIPT} {dockerfile}"),
            ));
        }
    }
    ci
}

fn dependabot_entry(ecosystem: &str, directory: &str, group: bool) -> String {
    let mut entry = format!(
        "  - package-ecosystem: \"{ecosystem}\"\n    directory: \"{directory}\"\n    schedule:\n      interval: \"weekly\"\n    open-pull-requests-limit: 10\n    labels:\n      - \"dependencies\"\n"
    );
    if group {
        entry.push_str("    groups:\n      minor-and-patch:\n        update-types:\n          - \"minor\"\n          - \"patch\"\n");
    }
    entry
}

fn render_dependabot(shape: &Shape) -> String {
    let mut config = String::from("version: 2\nupdates:\n");
    if let Some(js) = &shape.js {
        let ecosystem = match js.runtime {
            Runtime::Node => "npm",
            Runtime::Bun => "bun",
        };
        let directory = if js.directory == "." {
            "/".to_owned()
        } else {
            format!("/{}", js.directory)
        };
        config.push_str(&dependabot_entry(ecosystem, &directory, true));
    }
    if shape.rust_toolchain.is_some() {
        config.push_str(&dependabot_entry("cargo", "/", true));
    }
    config.push_str(&dependabot_entry("github-actions", "/", false));
    config
}

fn render_branch_protection(shape: &Shape) -> String {
    let checks = shape
        .jobs()
        .iter()
        .map(|job| format!("  - **{job}**\n"))
        .collect::<String>();
    format!(
        "# Branch Protection Setup Guide\n\nThis guide explains how to enable branch protection rules for your GitHub repository to enforce code quality and review standards.\n\n## Steps to Enable Branch Protection\n\n1. Navigate to your repository on GitHub.\n2. Go to **Settings** > **Branches**.\n3. Under **Branch protection rules**, click **Add rule**.\n4. In the **Branch name pattern** field, enter `main`.\n\n## Recommended Settings\n\n### Require Pull Request Reviews\n\n- Check **Require a pull request before merging**.\n- Set **Required number of approvals before merging** to at least `1`.\n- Optionally enable **Dismiss stale pull request approvals when new commits are pushed**.\n\n### Require Status Checks to Pass\n\n- Check **Require status checks to pass before merging**.\n- Select these checks from `.github/workflows/ci.yml`:\n{checks}- Check **Require branches to be up to date before merging** to ensure the branch is tested with the latest base branch changes.\n\nThe workflow installs from committed lockfiles (`--locked`, `--frozen-lockfile`) and fails when a build regenerates SDKs, contracts or OpenAPI documents that were not committed. Install and build once locally, then commit the lockfiles and generated sources.\n\n### Additional Recommendations\n\n- **Require conversation resolution before merging** to ensure all review comments are addressed.\n- **Do not allow bypassing the above settings** to enforce rules for all contributors, including administrators.\n- **Restrict who can push to matching branches** if you want to limit direct pushes.\n\n## Applying the Rule\n\nClick **Create** (or **Save changes** if editing an existing rule) to apply the branch protection settings.\n"
    )
}

fn digest(body: &str) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(body.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn owned_content(file: &str, body: &str) -> String {
    let digest = digest(body);
    let notice = "This is an auto-generated file. Modifications are encouraged but may inhibit automated upgrades.";
    let header = match Path::new(file).extension().and_then(|ext| ext.to_str()) {
        Some("md") => format!("<!-- Generated by ForkLaunch · File: {file} · {OWNERSHIP} {digest} -->\n\n"),
        Some("mjs") => format!(
            "/**\n * Generated by ForkLaunch\n * File: {file}\n * {notice}\n * {OWNERSHIP} {digest}\n */\n\n"
        ),
        _ => format!("# Generated by ForkLaunch\n# File: {file}\n# {notice}\n# {OWNERSHIP} {digest}\n\n"),
    };
    header + body
}

/// A file is generator-owned while its body still matches its recorded hash.
fn is_unmodified(content: &str) -> bool {
    let Some(start) = content.find(OWNERSHIP) else {
        return false;
    };
    let recorded = content[start + OWNERSHIP.len()..]
        .trim_start()
        .split(|c: char| !c.is_ascii_hexdigit())
        .next()
        .unwrap_or_default();
    content[start..]
        .find("\n\n")
        .is_some_and(|end| digest(&content[start + end + 2..]) == recorded)
}

#[derive(Content)]
struct LegacyCi {
    is_node: bool,
    is_bun: bool,
}

/// Exact output of earlier releases, which wrote no ownership hash.
fn legacy_contents(file: &str) -> Result<Vec<String>> {
    let watermark = generate_watermark(file).unwrap_or_default();
    let raw = match file {
        "ci.yml" => {
            let template = Template::new(get_file_contents(Path::new("github/legacy/ci.yml"))?)?;
            vec![
                template.render(&LegacyCi { is_node: true, is_bun: false }),
                template.render(&LegacyCi { is_node: false, is_bun: true }),
            ]
        }
        "dependabot.yml" => vec![get_file_contents(Path::new("github/legacy/dependabot.yml"))?],
        "BRANCH_PROTECTION.md" => {
            vec![get_file_contents(Path::new("github/legacy/BRANCH_PROTECTION.md"))?]
        }
        _ => Vec::new(),
    };
    Ok(raw
        .into_iter()
        .flat_map(|content| [format!("{watermark}{content}"), content])
        .collect())
}

/// Write or refresh generator-owned GitHub files for the application's current
/// languages. Called by init, change, delete and sync after projects change.
pub(crate) fn reconcile_github_configs(root: &Path) -> Result<()> {
    let shape = detect(root)?;
    let mut files: Vec<(PathBuf, String)> = vec![
        (".github/workflows/ci.yml".into(), render_ci(&shape)),
        (".github/dependabot.yml".into(), render_dependabot(&shape)),
        (".github/BRANCH_PROTECTION.md".into(), render_branch_protection(&shape)),
    ];
    if !shape.dockerfiles.is_empty() {
        files.push((
            SMOKE_SCRIPT.into(),
            get_file_contents(Path::new("github/container-smoke.mjs"))?,
        ));
    }
    for (relative, body) in files {
        let path = root.join(&relative);
        let file = relative.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let content = owned_content(file, &body);
        match fs::read_to_string(&path) {
            Ok(existing) if existing == content => {}
            Ok(existing)
                if !is_unmodified(&existing) && !legacy_contents(file)?.contains(&existing) =>
            {
                eprintln!(
                    "Kept customized {}; delete it to regenerate for this application's languages",
                    relative.display()
                );
            }
            _ => {
                fs::create_dir_all(path.parent().context("GitHub file has no parent")?)?;
                fs::write(&path, content)
                    .with_context(|| format!("Failed to write {}", path.display()))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(files: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        root
    }

    const MANIFEST: &str = "app_name = 'app'\nmodules_path = 'src'\nruntime = 'node'\n";
    const TS_SCRIPTS: &str = r#"{"packageManager":"pnpm@11.28.7","scripts":{"build":"pnpm -r build","lint":"oxlint --config .oxlint.config.json","test":"vitest"}}"#;

    #[test]
    fn typescript_applications_install_and_run_scripts_in_the_modules_workspace() {
        let root = app(&[(".forklaunch/manifest.toml", MANIFEST), ("src/package.json", TS_SCRIPTS), ("Dockerfile", "FROM node")]);
        reconcile_github_configs(root.path()).unwrap();
        let ci = fs::read_to_string(root.path().join(".github/workflows/ci.yml")).unwrap();
        assert!(!ci.contains("cargo") && !ci.contains("rustup"));
        for script in ["pnpm install --frozen-lockfile", "pnpm run build", "pnpm run lint", "pnpm run test"] {
            assert!(ci.contains(&format!("        working-directory: src\n        run: {script}\n")), "{script}");
        }
        assert!(ci.contains("corepack enable") && ci.contains("container-smoke.mjs Dockerfile"));
        let dependabot = fs::read_to_string(root.path().join(".github/dependabot.yml")).unwrap();
        assert!(dependabot.contains("directory: \"/src\"") && !dependabot.contains("cargo"));
        serde_yml::from_str::<serde_yml::Value>(&ci).unwrap();
        serde_yml::from_str::<serde_yml::Value>(&dependabot).unwrap();
    }

    #[test]
    fn bun_applications_install_with_the_generated_image_bun() {
        let manifest = MANIFEST.replace("runtime = 'node'", "runtime = 'bun'");
        let scripts = r#"{"scripts":{"build":"bun run --filter '*' build"}}"#;
        let root = app(&[(".forklaunch/manifest.toml", &manifest), ("src/package.json", scripts), ("Dockerfile", "FROM oven/bun")]);
        reconcile_github_configs(root.path()).unwrap();
        let ci = fs::read_to_string(root.path().join(".github/workflows/ci.yml")).unwrap();
        assert!(ci.contains(&format!("      - uses: oven-sh/setup-bun@v2\n        with:\n          bun-version: {BUN_VERSION}\n")), "{ci}");
        assert!(ci.contains("run: bun install --frozen-lockfile"));
        serde_yml::from_str::<serde_yml::Value>(&ci).unwrap();
    }

    /// The repository's own e2e workflow installs the Bun the generated images
    /// use; a bun.lock from any other release may not survive the frozen install.
    #[test]
    fn repository_e2e_workflow_pins_the_generated_image_bun() {
        let workflow = include_str!("../../../.github/workflows/e2e_tests.yml");
        let pins: Vec<_> = workflow
            .lines()
            .filter_map(|line| line.trim().strip_prefix("bun-version:"))
            .map(str::trim)
            .collect();
        let setups = workflow.matches("oven-sh/setup-bun@").count();
        assert!(!pins.is_empty() && pins.len() == setups, "every setup-bun step pins bun-version");
        assert!(pins.iter().all(|pin| *pin == BUN_VERSION), "{pins:?} != {BUN_VERSION}");
    }

    #[test]
    fn mixed_applications_provision_rust_and_run_scripts_where_they_are_defined() {
        let root = app(&[
            (".forklaunch/manifest.toml", MANIFEST),
            ("Cargo.toml", "[workspace]\nmembers = []\n"),
            ("package.json", r#"{"packageManager":"pnpm@11.28.7","scripts":{"build":"cargo build --workspace && pnpm -r build","sdk:typecheck":"x"}}"#),
            ("pnpm-workspace.yaml", "packages: ['src/*']\n"),
            ("src/package.json", TS_SCRIPTS),
            ("Dockerfile.rust", "FROM rust"),
            ("Dockerfile.typescript", "FROM node"),
            ("Dockerfile.rust.dockerignore", "target"),
        ]);
        reconcile_github_configs(root.path()).unwrap();
        let ci = fs::read_to_string(root.path().join(".github/workflows/ci.yml")).unwrap();
        let typescript = &ci[ci.find("  typescript:").unwrap()..ci.find("  docker:").unwrap()];
        assert!(typescript.contains("rustup toolchain install 1.98.0"), "Mixed builds compile Rust contracts");
        assert!(typescript.contains("        run: pnpm install --frozen-lockfile\n"));
        assert!(!typescript.contains("Install src workspace"), "Only a separate workspace root installs twice");
        assert!(typescript.contains("      - name: Build\n        run: pnpm run build\n"));
        assert!(typescript.contains("        working-directory: src\n        run: pnpm run lint\n"));
        assert!(!typescript.contains("sdk:typecheck"), "The root build already builds SDK packages");
        for check in ["cargo fmt -- --check", "cargo clippy --workspace --all-targets --locked", "cargo test --workspace --locked", "git status --porcelain"] {
            assert!(ci.contains(check), "{check}");
        }
        assert!(ci.contains("container-smoke.mjs Dockerfile.rust\n") && ci.contains("container-smoke.mjs Dockerfile.typescript\n"));
        assert!(!ci.contains("dockerignore"));
        let dependabot = fs::read_to_string(root.path().join(".github/dependabot.yml")).unwrap();
        assert!(dependabot.contains("\"cargo\"") && dependabot.contains("\"npm\"\n    directory: \"/\""));
        let protection = fs::read_to_string(root.path().join(".github/BRANCH_PROTECTION.md")).unwrap();
        assert!(protection.contains("**Rust**") && protection.contains("**TypeScript**") && protection.contains("**Docker**"));
    }

    #[test]
    fn a_modules_directory_with_its_own_workspace_root_installs_its_own_lockfile() {
        let root = app(&[
            (".forklaunch/manifest.toml", MANIFEST),
            ("Cargo.toml", "[workspace]\nmembers = []\n"),
            ("package.json", r#"{"packageManager":"pnpm@11.28.7","scripts":{"build":"pnpm -r build"}}"#),
            ("pnpm-workspace.yaml", "packages: ['src/*']\n"),
            ("src/package.json", TS_SCRIPTS),
            ("src/pnpm-workspace.yaml", "packages: ['core']\n"),
        ]);
        reconcile_github_configs(root.path()).unwrap();
        let ci = fs::read_to_string(root.path().join(".github/workflows/ci.yml")).unwrap();
        let root_install = ci.find("      - name: Install\n        run: pnpm install --frozen-lockfile\n").unwrap();
        let nested = ci.find("      - name: Install src workspace\n        working-directory: src\n        run: pnpm install --frozen-lockfile\n").unwrap();
        assert!(root_install < nested && nested < ci.find("pnpm run lint").unwrap());
    }

    #[test]
    fn rust_applications_respect_their_toolchain_and_type_check_generated_sdks() {
        let root = app(&[
            (".forklaunch/manifest.toml", MANIFEST),
            ("Cargo.toml", "[workspace]\nmembers = []\n"),
            ("rust-toolchain.toml", "[toolchain]\nchannel = \"1.99.0\"\n"),
            ("package.json", r#"{"packageManager":"pnpm@10.0.0","scripts":{"sdk:typecheck":"pnpm --filter x build"}}"#),
            ("pnpm-workspace.yaml", "packages: []\n"),
        ]);
        reconcile_github_configs(root.path()).unwrap();
        let ci = fs::read_to_string(root.path().join(".github/workflows/ci.yml")).unwrap();
        assert!(ci.contains("rustup toolchain install 1.99.0"));
        assert!(ci.contains("run: pnpm run sdk:typecheck") && !ci.contains("pnpm run lint"));
        assert!(!ci.contains("docker:") && !root.path().join(SMOKE_SCRIPT).exists());
    }

    #[test]
    fn reconciliation_follows_language_changes_but_preserves_customized_files() {
        let root = app(&[(".forklaunch/manifest.toml", MANIFEST), ("src/package.json", TS_SCRIPTS)]);
        // Files written by earlier releases carry no hash; they are recognized exactly.
        let legacy = format!(
            "{}{}",
            generate_watermark("ci.yml").unwrap(),
            Template::new(get_file_contents(Path::new("github/legacy/ci.yml")).unwrap())
                .unwrap()
                .render(&LegacyCi { is_node: true, is_bun: false })
        );
        let ci_path = root.path().join(".github/workflows/ci.yml");
        fs::create_dir_all(ci_path.parent().unwrap()).unwrap();
        fs::write(&ci_path, &legacy).unwrap();
        fs::write(root.path().join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        reconcile_github_configs(root.path()).unwrap();
        let mixed = fs::read_to_string(&ci_path).unwrap();
        assert!(mixed.contains("cargo test --workspace --locked") && is_unmodified(&mixed));

        fs::remove_file(root.path().join("Cargo.toml")).unwrap();
        reconcile_github_configs(root.path()).unwrap();
        assert!(!fs::read_to_string(&ci_path).unwrap().contains("cargo"), "Removing Rust removes its jobs");

        let custom = fs::read_to_string(&ci_path).unwrap().replace("pnpm run test", "pnpm run test:ci");
        fs::write(&ci_path, &custom).unwrap();
        fs::write(root.path().join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        reconcile_github_configs(root.path()).unwrap();
        assert_eq!(fs::read_to_string(&ci_path).unwrap(), custom);
    }
}
