use std::collections::HashMap;
use std::io::Write;
use std::{fs::File, path::Path};

use anyhow::{Context, Result, bail};
use flate2::{Compression, write::GzEncoder};
use ignore::WalkBuilder;
use reqwest::{blocking::Client, header};
use serde::Deserialize;
use serde_json::Value;
use tar::Builder;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{constants::get_platform_management_api_url, core::hmac::AuthMode};

#[derive(Debug, Deserialize)]
pub(crate) struct UploadUrlResponse {
    #[serde(rename = "uploadUrl")]
    pub upload_url: String,
    #[serde(rename = "codeSourceUrl")]
    pub code_source_url: String,
}

/// Create a tarball of the modules directory, respecting .gitignore and .flignore.
/// Paths in the tarball are relative to `app_root`, preserving the modules_path descent
/// (e.g. if modules_path is `<app_root>/packages/modules`, files appear as `packages/modules/...`).
pub(crate) fn create_app_tarball(
    app_root: &Path,
    modules_path: &Path,
    output_path: &Path,
) -> Result<()> {
    let tar_gz = File::create(output_path)
        .with_context(|| format!("Failed to create tarball file at {:?}", output_path))?;
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = Builder::new(enc);

    // Walk only the modules directory, respecting .gitignore and .flignore
    let walker = WalkBuilder::new(modules_path)
        .hidden(false) // Include hidden files (except those in .gitignore)
        .git_ignore(true) // Respect .gitignore
        .git_global(true) // Respect global gitignore
        .git_exclude(true) // Respect .git/info/exclude
        .require_git(false) // Works even without a .git directory
        .add_custom_ignore_filename(".flignore") // Respect .flignore for release exclusions
        .build();

    let mut file_count = 0;
    for entry in walker {
        let entry = entry.with_context(|| "Failed to read directory entry")?;
        let path = entry.path();

        if path.starts_with(app_root.join(".git")) {
            continue;
        }

        // Always exclude node_modules regardless of .gitignore
        if path.components().any(|c| c.as_os_str() == "node_modules") {
            continue;
        }

        if path == output_path {
            continue;
        }

        if path.is_file() {
            // Strip app_root (not modules_path) to preserve the modules_path descent
            let relative_path = path
                .strip_prefix(app_root)
                .with_context(|| format!("Failed to get relative path for {:?}", path))?;

            tar.append_path_with_name(path, relative_path)
                .with_context(|| format!("Failed to add {:?} to tarball", relative_path))?;

            file_count += 1;
        }
    }

    tar.finish().with_context(|| "Failed to finalize tarball")?;

    if file_count == 0 {
        bail!("No files found to package. Check your .gitignore settings.");
    }

    Ok(())
}

pub(crate) fn get_presigned_upload_url(
    application_id: &str,
    version: &str,
    auth_mode: &AuthMode,
) -> Result<UploadUrlResponse> {
    use crate::core::http_client;

    let url = format!("{}/releases/upload-url", get_platform_management_api_url());

    let request_body = serde_json::json!({
        "applicationId": application_id,
        "version": version
    });

    let response = http_client::post_with_auth(auth_mode, &url, request_body)
        .with_context(|| "Failed to request upload URL from platform")?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response
            .text()
            .unwrap_or_else(|_| "Unknown error".to_string());
        bail!(
            "Failed to get upload URL: {} (Status: {})",
            error_body,
            status
        );
    }

    response
        .json::<UploadUrlResponse>()
        .with_context(|| "Failed to parse upload URL response")
}

/// Upload tarball to S3 using presigned URL
pub(crate) fn upload_to_s3(file_path: &Path, presigned_url: &str) -> Result<()> {
    let client = Client::new();

    let file = File::open(file_path)
        .with_context(|| format!("Failed to open tarball file {:?}", file_path))?;

    let file_size = file
        .metadata()
        .with_context(|| format!("Failed to get metadata for file {:?}", file_path))?
        .len();

    let response = client
        .put(presigned_url)
        .body(file)
        .header(header::CONTENT_TYPE, "application/gzip")
        .header(header::CONTENT_LENGTH, file_size)
        .send()
        .with_context(|| "Failed to upload tarball to S3")?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response
            .text()
            .unwrap_or_else(|_| "Unknown error".to_string());
        bail!(
            "Failed to upload to S3: {} (Status: {})",
            error_body,
            status
        );
    }

    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    log_info!(stdout, "Uploaded {} bytes to S3", file_size);
    Ok(())
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenApiUploadUrlEntry {
    #[serde(rename = "uploadUrl")]
    pub upload_url: String,
    #[serde(rename = "s3Key")]
    pub s3_key: String,
}

/// Get batch presigned URLs for uploading OpenAPI specs to S3
pub(crate) fn get_openapi_upload_urls(
    application_id: &str,
    version: &str,
    service_names: &[String],
    auth_mode: &AuthMode,
) -> Result<HashMap<String, OpenApiUploadUrlEntry>> {
    use crate::core::http_client;

    let url = format!(
        "{}/releases/openapi-upload-urls",
        get_platform_management_api_url()
    );
    );

    let request_body = serde_json::json!({
        "applicationId": application_id,
        "version": version,
        "serviceNames": service_names
    });

    let response = http_client::post_with_auth(auth_mode, &url, request_body)
        .with_context(|| "Failed to request OpenAPI upload URLs from platform")?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response
            .text()
            .unwrap_or_else(|_| "Unknown error".to_string());
        bail!(
            "Failed to get OpenAPI upload URLs: {} (Status: {})",
            error_body,
            status
        );
    }

    let wrapper: serde_json::Value = response
        .json()
        .with_context(|| "Failed to parse OpenAPI upload URL response")?;

    // Response is { urls: { serviceName: { uploadUrl, s3Key } } }
    let urls_value = wrapper
        .get("urls")
        .ok_or_else(|| anyhow::anyhow!("Missing 'urls' field in response"))?;

    serde_json::from_value(urls_value.clone())
        .with_context(|| "Failed to parse OpenAPI upload URLs from response")
}

/// Upload a JSON value to S3 using a presigned URL
pub(crate) fn upload_json_to_s3(json_value: &Value, presigned_url: &str) -> Result<()> {
    let client = Client::new();

    let json_bytes = serde_json::to_vec(json_value)
        .with_context(|| "Failed to serialize JSON for S3 upload")?;

    let response = client
        .put(presigned_url)
        .body(json_bytes.clone())
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_LENGTH, json_bytes.len())
        .send()
        .with_context(|| "Failed to upload JSON to S3")?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response
            .text()
            .unwrap_or_else(|_| "Unknown error".to_string());
        bail!(
            "Failed to upload JSON to S3: {} (Status: {})",
            error_body,
            status
        );
    }

    Ok(())
}
