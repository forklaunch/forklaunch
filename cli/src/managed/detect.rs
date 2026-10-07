//! Deploy-time detection of whether the app being deployed is really a managed
//! template rather than a single-app project.
//!
//! Whether an app is "managed" is a control-plane fact, not something the manifest
//! records: a managed template is keyed by its source repository. So this asks the
//! control plane, listing the organization's templates and matching each template's
//! `sourceRepo` against the manifest's `git_repository`. It never fails the deploy on
//! its own account. A control plane without the `/managed-mode` routes, or credentials
//! that cannot be scoped to an organization, both resolve to `Inconclusive` so the
//! caller proceeds as a single-app deploy.

use super::client::{extract_list, managed_url};
use super::types::AppTemplate;
use crate::core::{hmac::AuthMode, http_client::get_with_auth};

/// Outcome of asking the control plane whether the app being deployed is a managed
/// template.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ManagedDetection {
    /// The app's git repository matches a template the organization has registered.
    /// Carries the template slug so the caller can redirect the user to
    /// `managed instance create`.
    MatchedTemplate(String),
    /// The control plane exposes managed mode and answered, but no template matched.
    /// A first deploy can use this to hint that managed mode exists.
    ManagedModeAvailableNoMatch,
    /// Detection could not run: this control plane has no `/managed-mode` routes (an
    /// older or self-hosted platform answers 404), the credentials carry no organization
    /// identity (HMAC/CI answers 401/403), or the request itself failed. The caller must
    /// treat the app as NOT managed and proceed.
    Inconclusive,
}

/// Normalizes a git repository URL for tolerant comparison: trims surrounding
/// whitespace, drops a trailing `.git` and any trailing slash, and lowercases (host case
/// is not significant, and the repositories these compare are not case-sensitive in
/// practice). It deliberately does not try to reconcile ssh and https forms, because the
/// only differences in scope are the trivial ones, and over-normalizing risks a false
/// match that would wrongly block a deploy.
fn normalize_repo(repo: &str) -> String {
    let trimmed = repo.trim().trim_end_matches('/');
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    without_git.trim_end_matches('/').to_lowercase()
}

/// Given the resolved manifest's `git_repository`, returns the matching template slug (as
/// `MatchedTemplate`) if any published or unpublished template's `sourceRepo` matches.
///
/// Reuses the same 404 tolerance the rest of the managed client relies on: any
/// non-success status (404 for an unmounted router, 401/403 for HMAC/CI credentials that
/// carry no organization) resolves to `Inconclusive`, as does a network error or a
/// response this CLI cannot parse. The deploy is never failed just because detection
/// could not run.
pub(crate) fn detect_managed_template(
    auth_mode: &AuthMode,
    git_repository: Option<&str>,
) -> ManagedDetection {
    // With no repository recorded in the manifest there is nothing to match on.
    let Some(git_repository) = git_repository else {
        return ManagedDetection::Inconclusive;
    };
    let target = normalize_repo(git_repository);
    if target.is_empty() {
        return ManagedDetection::Inconclusive;
    }

    // Include unpublished templates on purpose: deploying a DRAFT managed template down
    // the single-app pipeline is exactly as wrong as deploying a published one.
    let url = managed_url("/templates?includeUnpublished=true");
    let response = match get_with_auth(auth_mode, &url) {
        Ok(response) => response,
        Err(_) => return ManagedDetection::Inconclusive,
    };

    if !response.status().is_success() {
        return ManagedDetection::Inconclusive;
    }

    let value = match response.json::<serde_json::Value>() {
        Ok(value) => value,
        Err(_) => return ManagedDetection::Inconclusive,
    };
    let templates: Vec<AppTemplate> = match extract_list(value, &["templates"]) {
        Ok(templates) => templates,
        Err(_) => return ManagedDetection::Inconclusive,
    };

    for template in &templates {
        let (Some(source_repo), Some(slug)) =
            (template.source_repo.as_deref(), template.slug.as_deref())
        else {
            continue;
        };
        if normalize_repo(source_repo) == target {
            return ManagedDetection::MatchedTemplate(slug.to_string());
        }
    }

    ManagedDetection::ManagedModeAvailableNoMatch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_repo_ignores_trailing_git_slash_and_case() {
        assert_eq!(
            normalize_repo("https://GitHub.com/Acme/clinic.git"),
            normalize_repo("https://github.com/acme/clinic")
        );
        assert_eq!(
            normalize_repo("https://github.com/acme/clinic/"),
            "https://github.com/acme/clinic"
        );
        assert_eq!(
            normalize_repo("https://github.com/acme/clinic.git/"),
            "https://github.com/acme/clinic"
        );
    }

    #[test]
    fn normalize_repo_does_not_conflate_different_repositories() {
        assert_ne!(
            normalize_repo("https://github.com/acme/clinic"),
            normalize_repo("https://github.com/acme/clinic-staging")
        );
    }
}
