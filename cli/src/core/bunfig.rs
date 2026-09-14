use std::path::Path;

use anyhow::Result;

use super::rendered_template::RenderedTemplate;

/// First-party packages exempted from bun's release-age gate.
///
/// Bun's `minimumReleaseAgeExcludes` matches **exact package names only** —
/// verified against bun 1.4.0, where `"@forklaunch/*"`, `"@forklaunch/**"` and
/// `"@forklaunch"` all leave the gate in force, and exclusions do not cascade
/// to transitive dependencies. Excluding `@forklaunch/core` alone lets core
/// through and still blocks the `@forklaunch/common` and `@forklaunch/validator`
/// it depends on. So every name has to be listed.
///
/// pnpm differs: `minimumReleaseAgeExclude` there does honour `@forklaunch/*`,
/// which is why `pnpm_workspace.rs` gets away with a single pattern.
///
/// Kept in sync with `package_json_constants.rs` by a test below, so adding a
/// package there without adding it here fails the build rather than silently
/// reintroducing release-day breakage.
const FORKLAUNCH_PACKAGES: &[&str] = &[
    "@forklaunch/better-auth-mikro-orm-fork",
    "@forklaunch/blueprint-billing",
    "@forklaunch/blueprint-core",
    "@forklaunch/blueprint-iam",
    "@forklaunch/blueprint-monitoring",
    "@forklaunch/bunrun",
    "@forklaunch/common",
    "@forklaunch/core",
    "@forklaunch/express",
    "@forklaunch/hyper-express",
    "@forklaunch/implementation-billing-base",
    "@forklaunch/implementation-billing-stripe",
    "@forklaunch/implementation-cac-base",
    "@forklaunch/implementation-ecommerce-base",
    "@forklaunch/implementation-ecommerce-paypal",
    "@forklaunch/implementation-ecommerce-stripe",
    "@forklaunch/implementation-iam-base",
    "@forklaunch/implementation-messaging-base",
    "@forklaunch/implementation-messaging-twilio",
    "@forklaunch/implementation-worker-bullmq",
    "@forklaunch/implementation-worker-database",
    "@forklaunch/implementation-worker-kafka",
    "@forklaunch/implementation-worker-redis",
    "@forklaunch/infrastructure-redis",
    "@forklaunch/infrastructure-s3",
    "@forklaunch/interfaces-billing",
    "@forklaunch/interfaces-cac",
    "@forklaunch/interfaces-ecommerce",
    "@forklaunch/interfaces-iam",
    "@forklaunch/interfaces-messaging",
    "@forklaunch/interfaces-worker",
    "@forklaunch/internal",
    "@forklaunch/testing",
    "@forklaunch/universal-sdk",
    "@forklaunch/validator",
];

/// Bun equivalent of pnpm_workspace.rs's minimumReleaseAge default. Bun's
/// [install] table takes seconds, not minutes: 86400 = 24h, matching pnpm's
/// 1440-minute default and forklaunch-platform's own pnpm-workspace.yaml.
/// Same rationale: pin it explicitly so a scaffold's local `bun install`
/// resolves under the same age policy the deploy pipeline enforces, rather
/// than locking in fresh versions the pipeline's bun then rejects.
///
/// The exemption list matters as much as the gate. The gate exists to slow
/// compromised third-party publishes; applied to our own packages it only ever
/// blocks the release it is meant to deliver, and a scaffold pinned to
/// just-published versions fails `bun install` outright for 24 hours:
///
///     error: No version matching "@forklaunch/core" found for specifier
///     "~1.5.17" (blocked by minimum-release-age: 86400 seconds)
fn bunfig_toml() -> String {
    let mut out = String::from("[install]\nminimumReleaseAge = 86400\nminimumReleaseAgeExcludes = [\n");
    for package in FORKLAUNCH_PACKAGES {
        out.push_str(&format!("  \"{package}\",\n"));
    }
    out.push_str("]\n");
    out
}

pub(crate) fn generate_bunfig(application_path: &str) -> Result<Option<RenderedTemplate>> {
    let bunfig_path = Path::new(application_path).join("bunfig.toml");
    if bunfig_path.exists() {
        return Ok(None);
    }

    Ok(Some(RenderedTemplate {
        path: bunfig_path,
        content: bunfig_toml(),
        context: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> String {
        let dir = tempfile::tempdir().unwrap();
        generate_bunfig(dir.path().to_str().unwrap())
            .unwrap()
            .unwrap()
            .content
    }

    #[test]
    fn test_generate_bunfig_pins_minimum_release_age() {
        assert!(rendered().contains("minimumReleaseAge = 86400"));
    }

    #[test]
    fn test_generate_bunfig_skips_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bunfig.toml"), "# existing\n").unwrap();
        let rendered = generate_bunfig(dir.path().to_str().unwrap()).unwrap();
        assert!(rendered.is_none());
    }

    #[test]
    fn first_party_packages_are_exempt_from_the_age_gate() {
        let content = rendered();
        assert!(content.contains("minimumReleaseAgeExcludes"));
        // The two that broke CI on release day, plus a workspace-protocol one.
        for package in [
            "@forklaunch/core",
            "@forklaunch/validator",
            "@forklaunch/implementation-iam-base",
        ] {
            assert!(
                content.contains(&format!("\"{package}\"")),
                "missing exemption for {package}"
            );
        }
    }

    #[test]
    fn the_exemption_list_uses_exact_names_not_globs() {
        // bun 1.4.0 ignores patterns here; a glob would parse and silently do
        // nothing, which is worse than an error.
        let content = rendered();
        assert!(
            !content.contains("@forklaunch/*"),
            "bun does not honour globs in minimumReleaseAgeExcludes"
        );
    }

    #[test]
    fn the_exemption_list_covers_every_package_the_cli_pins() {
        // package_json_constants.rs names each package in a comment above its
        // version constant. Anything pinned there but missing here comes back
        // as a release-day install failure, so drift fails the build instead.
        let constants = include_str!("package_json/package_json_constants.rs");
        let mut missing = Vec::new();
        for line in constants.lines() {
            let trimmed = line.trim();
            let Some(name) = trimmed.strip_prefix("// @forklaunch/") else {
                continue;
            };
            let name = format!("@forklaunch/{}", name.trim());
            if !FORKLAUNCH_PACKAGES.contains(&name.as_str()) {
                missing.push(name);
            }
        }
        assert!(
            missing.is_empty(),
            "package_json_constants.rs pins these with no bunfig exemption: {missing:?}"
        );
    }
}
