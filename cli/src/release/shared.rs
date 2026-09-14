use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::http_client,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReleaseSummary {
    #[serde(default)]
    pub(crate) id: Option<String>,
    #[serde(default)]
    pub(crate) version: Option<String>,
    #[serde(default)]
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) created_at: Option<String>,
    #[serde(default)]
    pub(crate) git_commit: Option<String>,
    #[serde(default)]
    pub(crate) git_branch: Option<String>,
    #[serde(default)]
    pub(crate) released_by: Option<String>,
    #[serde(default)]
    pub(crate) notes: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ReleaseListResponse {
    #[serde(default)]
    pub(crate) releases: Vec<ReleaseSummary>,
}

/// Fetch the release list for an application via `GET /releases/`.
///
/// Shared by `release info` and `release list` so both hit the same
/// platform endpoint with identical parsing.
pub(crate) fn fetch_releases(app: &str, limit: u32) -> Result<Vec<ReleaseSummary>> {
    let url = format!(
        "{}/releases/?applicationId={}&limit={}",
        get_platform_management_api_url(),
        app,
        limit
    );
    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        bail!(
            "Failed to list releases: {}",
            response.text().unwrap_or_default()
        );
    }
    let list: ReleaseListResponse = response
        .json()
        .with_context(|| "Failed to parse release list response")?;
    Ok(list.releases)
}
