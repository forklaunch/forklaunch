//! Guard against setting a variable that nothing in the application reads.
//!
//! The failure this exists for: three `config set` calls in a single session
//! each stored a live credential under a name no component declares
//! (`TWILIO_ACCOUNT_TOKEN` for `TWILIO_AUTH_TOKEN`, `TWILIO_AUTH_SID` for
//! `TWILIO_ACCOUNT_SID`). Every one was accepted silently; the only signal was
//! the word "Added" instead of "Updated" in the success line.
//!
//! Two sources are unioned to decide whether a name is known, because either
//! one alone produces false alarms:
//!
//! * the config the platform just returned — it carries both the variables
//!   that already hold values and, as bare `KEY=` lines, the required
//!   variables from the deployed release manifest;
//! * an AST scan of the local workspace — the same scan whose output becomes
//!   `requiredEnvironmentVariables` in the release manifest, so it also covers
//!   variables declared `optional(...)` that the platform does not emit.
//!
//! Neither source is authoritative on its own: the platform's view lags a
//! working tree that has not been released yet, and the working tree lags a
//! release cut from a different checkout. So an unrecognised name is reported
//! as a warning, never as a hard failure.

use std::{
    collections::HashSet,
    io::{IsTerminal, Write},
    path::Path,
};

use anyhow::{Result, bail};
use dialoguer::{Confirm, theme::ColorfulTheme};
use termcolor::{StandardStream, WriteColor};

use crate::core::{
    ast::infrastructure::env::find_all_env_vars,
    env::{EnvFileItem, find_workspace_root, get_modules_path, parse_env_items_from_str},
    rendered_template::RenderedTemplatesCache,
    string::closest_matches,
};

/// How many "did you mean" candidates to offer. Both typos from the incident
/// sat the same edit distance from two different real names, so offering a
/// single guess would have been a coin flip.
const MAX_SUGGESTIONS: usize = 3;

/// Every variable name appearing anywhere in `config pull` output, across all
/// scopes. Section headers are ignored, so a name found under one service
/// counts as known when setting it on another — a variable that exists
/// somewhere in the application is not a typo.
pub(crate) fn names_in_pulled_config(content: &str) -> HashSet<String> {
    parse_env_items_from_str(content)
        .into_iter()
        .filter_map(|item| match item {
            EnvFileItem::KeyValue(key, _) => Some(key),
            EnvFileItem::SectionHeader(_) => None,
        })
        .collect()
}

/// Every variable name declared by a component in the local workspace.
///
/// `None` means the workspace could not be scanned — no modules directory, no
/// `registrations.ts` anywhere, or a parse failure. That is "we cannot tell",
/// which must never be reported as "this variable is undeclared".
pub(crate) fn names_declared_in_workspace(app_root: &Path) -> Option<HashSet<String>> {
    let workspace_root = find_workspace_root(app_root).ok()?;
    let modules_path = get_modules_path(&workspace_root).ok()?;
    let rendered_templates_cache = RenderedTemplatesCache::new();
    let project_env_vars = find_all_env_vars(&modules_path, &rendered_templates_cache).ok()?;

    if project_env_vars.is_empty() {
        return None;
    }

    Some(
        project_env_vars
            .values()
            .flatten()
            .map(|usage| usage.var_name.clone())
            .collect(),
    )
}

/// Decide whether `key` is a name something is known to read, given the two
/// sources above. `None` for `declared` means the workspace scan was
/// unavailable, in which case only the platform's view is consulted and an
/// unknown name is treated as known.
pub(crate) fn is_known_name(
    key: &str,
    in_config: &HashSet<String>,
    declared: Option<&HashSet<String>>,
) -> bool {
    if in_config.contains(key) {
        return true;
    }
    match declared {
        Some(declared) => declared.contains(key),
        None => true,
    }
}

/// Build the "did you mean" line from both name sources combined.
pub(crate) fn suggestions_for(
    key: &str,
    in_config: &HashSet<String>,
    declared: Option<&HashSet<String>>,
) -> Vec<String> {
    let mut candidates: Vec<String> = in_config
        .iter()
        .chain(declared.into_iter().flatten())
        .cloned()
        .collect();
    candidates.sort();
    candidates.dedup();

    closest_matches(key, &candidates, MAX_SUGGESTIONS)
}

/// Format suggestions as `'A', 'B' or 'C'`.
pub(crate) fn format_suggestions(suggestions: &[String]) -> String {
    let quoted: Vec<String> = suggestions.iter().map(|s| format!("'{}'", s)).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {}", rest.join(", "), last),
    }
}

/// Warn when `key` is a name nothing is known to read, and — on an
/// interactive terminal without `--force` — ask before writing it.
///
/// Returns `Err` only when the operator declines the prompt. A scripted
/// invocation is never blocked: it gets the warning on the way past, so
/// existing automation keeps working.
#[allow(clippy::too_many_arguments)]
pub(crate) fn confirm_if_undeclared(
    stdout: &mut StandardStream,
    key: &str,
    app_root: &Path,
    pulled_config: &str,
    environment: &str,
    region: &str,
    force: bool,
) -> Result<()> {
    let in_config = names_in_pulled_config(pulled_config);

    // The workspace scan parses every TypeScript file under the modules
    // directory, so it only runs when the cheap check has already failed.
    if in_config.contains(key) {
        return Ok(());
    }

    let declared = names_declared_in_workspace(app_root);
    if is_known_name(key, &in_config, declared.as_ref()) {
        return Ok(());
    }

    log_warn!(
        stdout,
        "'{}' is not declared by any component in this application, and no variable by that name exists in {} ({}).",
        key,
        environment,
        region
    );

    let suggestions = suggestions_for(key, &in_config, declared.as_ref());
    if !suggestions.is_empty() {
        log_warn!(
            stdout,
            "       Did you mean {}?",
            format_suggestions(&suggestions)
        );
    }
    log_warn!(
        stdout,
        "       Setting it will store the value under a name nothing reads."
    );

    if force {
        log_warn!(stdout, "       Continuing because --force was given.");
        return Ok(());
    }

    if !std::io::stdin().is_terminal() {
        log_warn!(
            stdout,
            "       Continuing: not an interactive terminal, so there is nobody to ask. Pass --force to silence this."
        );
        return Ok(());
    }

    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Set '{}' anyway?", key))
        .default(false)
        .interact()?;

    if !confirmed {
        bail!("aborted — nothing was set");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_of(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_names_in_pulled_config_collects_across_scopes() {
        let content = "# application\n\
                       DB_HOST=db.example.com\n\
                       # payments (svc-123)\n\
                       STRIPE_API_KEY=sk_live_x\n\
                       # mailer-worker (wkr-456)\n\
                       QUEUE_NAME=mail\n";

        let names = names_in_pulled_config(content);
        assert_eq!(names.len(), 3);
        assert!(names.contains("DB_HOST"));
        assert!(names.contains("STRIPE_API_KEY"));
        assert!(names.contains("QUEUE_NAME"));
    }

    /// The platform emits declared-but-unset required variables as bare
    /// `KEY=` lines. Those are declared names and must count as known.
    #[test]
    fn test_names_in_pulled_config_includes_valueless_required_vars() {
        let content = "# application\nDB_HOST=db.example.com\nECS_AGENT_URI=\n";
        let names = names_in_pulled_config(content);
        assert!(names.contains("ECS_AGENT_URI"));
    }

    #[test]
    fn test_names_in_pulled_config_ignores_headers() {
        let names = names_in_pulled_config("# application\n# payments (svc-1)\n");
        assert!(names.is_empty());
    }

    #[test]
    fn test_is_known_name_from_platform_config() {
        let in_config = set_of(&["DB_HOST"]);
        let declared = set_of(&["REDIS_URL"]);
        assert!(is_known_name("DB_HOST", &in_config, Some(&declared)));
    }

    #[test]
    fn test_is_known_name_from_workspace_declaration() {
        let in_config = set_of(&["DB_HOST"]);
        let declared = set_of(&["REDIS_URL"]);
        assert!(is_known_name("REDIS_URL", &in_config, Some(&declared)));
    }

    #[test]
    fn test_unknown_name_is_flagged() {
        let in_config = set_of(&["DB_HOST"]);
        let declared = set_of(&["REDIS_URL"]);
        assert!(!is_known_name(
            "TWILIO_ACCOUNT_TOKEN",
            &in_config,
            Some(&declared)
        ));
    }

    /// No workspace scan means no evidence, and no evidence must not read as
    /// evidence of absence.
    #[test]
    fn test_unscannable_workspace_never_flags() {
        let in_config = set_of(&["DB_HOST"]);
        assert!(is_known_name("ANYTHING_AT_ALL", &in_config, None));
    }

    #[test]
    fn test_suggestions_draw_from_both_sources() {
        let in_config = set_of(&["TWILIO_ACCOUNT_SID"]);
        let declared = set_of(&["TWILIO_AUTH_TOKEN"]);

        let suggestions = suggestions_for("TWILIO_ACCOUNT_TOKEN", &in_config, Some(&declared));
        assert!(suggestions.contains(&"TWILIO_ACCOUNT_SID".to_string()));
        assert!(suggestions.contains(&"TWILIO_AUTH_TOKEN".to_string()));
    }

    #[test]
    fn test_suggestions_are_capped() {
        let in_config = set_of(&["DB_HOSTA", "DB_HOSTB", "DB_HOSTC", "DB_HOSTD"]);
        let suggestions = suggestions_for("DB_HOST", &in_config, None);
        assert_eq!(suggestions.len(), MAX_SUGGESTIONS);
    }

    #[test]
    fn test_format_suggestions_shapes() {
        assert_eq!(format_suggestions(&[]), "");
        assert_eq!(format_suggestions(&["A".to_string()]), "'A'");
        assert_eq!(
            format_suggestions(&["A".to_string(), "B".to_string()]),
            "'A' or 'B'"
        );
        assert_eq!(
            format_suggestions(&["A".to_string(), "B".to_string(), "C".to_string()]),
            "'A', 'B' or 'C'"
        );
    }
}
