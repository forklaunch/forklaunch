use std::io::Write;
use std::{fs::File, path::Path};

use anyhow::{Context, Result, bail};
use flate2::{Compression, write::GzEncoder};
use ignore::WalkBuilder;
use reqwest::{blocking::Client, header};
use serde::Deserialize;
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

/// Create a tarball of the application directory, respecting .gitignore
pub(crate) fn create_app_tarball(app_root: &Path, output_path: &Path) -> Result<()> {
    let tar_gz = File::create(output_path)
        .with_context(|| format!("Failed to create tarball file at {:?}", output_path))?;
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = Builder::new(enc);

    // Use the 'ignore' crate to walk the directory respecting .gitignore
    // This also handles nested .gitignore files and .git/info/exclude
    let walker = WalkBuilder::new(app_root)
        .hidden(false) // Include hidden files (except those in .gitignore)
        .git_ignore(true) // Respect .gitignore
        .git_global(true) // Respect global gitignore
        .git_exclude(true) // Respect .git/info/exclude
        .require_git(false) // Works even without a .git directory
        .build();

    let mut file_count = 0;
    for entry in walker {
        let entry = entry.with_context(|| "Failed to read directory entry")?;
        let path = entry.path();

        if path.starts_with(app_root.join(".git")) {
            continue;
        }

        if path == output_path {
            continue;
        }

        if path.is_file() {
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
    log_info!(stdout, "[INFO] Uploaded {} bytes to S3", file_size);
    Ok(())
}
