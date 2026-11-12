//! `forklaunch compliance docs`: the Document Center from the command line.
//!
//! Every document configuration (a company-wide document, or one copy of a
//! system document with its own set of applications) can be listed,
//! generated, regenerated, shown, saved and downloaded. Approval is left to a
//! person in the dashboard on purpose: an agent can draft and refresh
//! evidence, but a named human signs it off. `push` sends approved versions
//! to the organization's connected Oneleet workspace; drafts are never sent.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command, parser::ValueSource};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

// ── Types (mirror ComplianceDocumentSchemas in platform-management) ──────────

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DocumentSummary {
    document_id: String,
    title: String,
    status: String,
    latest_version: u32,
    #[serde(default)]
    approved_version: Option<u32>,
    open_inputs: u32,
    #[serde(default)]
    application_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogEntry {
    #[serde(rename = "type")]
    document_type: String,
    title: String,
    scope: String,
    documents: Vec<DocumentSummary>,
}

#[derive(Debug, Deserialize)]
struct CatalogResponse {
    documents: Vec<CatalogEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopeApplication {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentDetail {
    id: String,
    document_type: String,
    title: String,
    scope: String,
    applications: Vec<ScopeApplication>,
    status: String,
    latest_version: u32,
    #[serde(default)]
    approved_version: Option<u32>,
    content: String,
    open_inputs: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PushedControl {
    control_id: String,
    title: String,
    evidence_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PushResult {
    document_id: String,
    version: u32,
    file_name: String,
    controls: Vec<PushedControl>,
}

#[derive(Debug, Deserialize)]
struct VersionDetail {
    version: u32,
    content: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn base() -> String {
    format!("{}/compliance/documents", get_platform_management_api_url())
}

fn fail(what: &str, response: reqwest::blocking::Response) -> anyhow::Error {
    let status = response.status().as_u16();
    let body = response.text().unwrap_or_default();
    match status {
        401 => anyhow::anyhow!("{what}: not signed in. Run `forklaunch login`."),
        403 => {
            anyhow::anyhow!("{what}: your organization's plan or role does not allow this ({body})")
        }
        404 => anyhow::anyhow!("{what}: document not found"),
        _ => anyhow::anyhow!("{what} ({status}): {body}"),
    }
}

fn fetch_catalog(application_id: Option<&String>) -> Result<Vec<CatalogEntry>> {
    let mut url = base();
    if let Some(app) = application_id {
        url.push_str(&format!("?applicationId={}", urlencoding::encode(app)));
    }
    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        return Err(fail("Failed to list documents", response));
    }
    Ok(response
        .json::<CatalogResponse>()
        .context("Failed to parse documents response")?
        .documents)
}

fn fetch_document(id: &str) -> Result<DocumentDetail> {
    let url = format!("{}/{}", base(), urlencoding::encode(id));
    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        return Err(fail("Failed to load document", response));
    }
    response.json().context("Failed to parse document")
}

fn split_list(value: Option<&String>) -> Option<Vec<String>> {
    value.map(|v| {
        v.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
}

pub(crate) fn file_slug(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').chars().take(80).collect()
}

fn json_flag() -> Arg {
    Arg::new("json")
        .long("json")
        .help("Print JSON instead of text (for scripts and agents)")
        .action(ArgAction::SetTrue)
}

fn print_document_line(doc: &DocumentDetail) {
    let scope = if doc.scope == "organization" {
        "company-wide".to_string()
    } else {
        let names: Vec<&str> = doc.applications.iter().map(|a| a.name.as_str()).collect();
        format!("systems: {}", names.join(", "))
    };
    println!(
        "{}  {}  v{} {}  ({}; {} input(s) open)",
        doc.id,
        doc.title,
        doc.latest_version,
        doc.status,
        scope,
        doc.open_inputs.len()
    );
}

// ── docs ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct DocsCommand;

impl DocsCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for DocsCommand {
    fn command(&self) -> Command {
        command(
            "docs",
            "Compliance documents generated from your platform records (Document Center)",
        )
        .subcommand_required(true)
        .subcommand(
            command("list", "List document types and every document configuration")
                .arg(
                    Arg::new("app")
                        .long("app")
                        .help("Only documents that cover this application ID"),
                )
                .arg(json_flag()),
        )
        .subcommand(
            command(
                "generate",
                "Generate a document from platform records. Company-wide types get a new version; system types get a new copy",
            )
            .arg(
                Arg::new("type")
                    .required(true)
                    .help("Document type, e.g. data-flow-inventory (see `docs list`)"),
            )
            .arg(
                Arg::new("apps")
                    .long("apps")
                    .help("System documents: comma-separated application IDs to cover (default: apps that declare a framework)"),
            )
            .arg(Arg::new("title").long("title").help("Name for a new system copy"))
            .arg(json_flag()),
        )
        .subcommand(
            command(
                "regenerate",
                "Write a new version of documents from current platform records",
            )
            .arg(
                Arg::new("document_id")
                    .num_args(0..)
                    .help("Document IDs to regenerate"),
            )
            .arg(
                Arg::new("all")
                    .long("all")
                    .help("Regenerate every document configuration in the organization")
                    .action(ArgAction::SetTrue)
                    .conflicts_with("document_id"),
            )
            .arg(
                Arg::new("apps")
                    .long("apps")
                    .help("With one system document: change its applications (comma-separated IDs) first")
                    .conflicts_with("all"),
            )
            .arg(json_flag()),
        )
        .subcommand(
            command("show", "Print a document's newest text (markdown) to stdout")
                .arg(Arg::new("document_id").required(true))
                .arg(json_flag()),
        )
        .subcommand(
            command(
                "save",
                "Save edited markdown as a new version (refused if someone saved since you read it)",
            )
            .arg(Arg::new("document_id").required(true))
            .arg(
                Arg::new("file")
                    .long("file")
                    .required(true)
                    .help("Markdown file with the full new text"),
            )
            .arg(
                Arg::new("base_version")
                    .long("base-version")
                    .help("Version the edit started from (default: the current newest)"),
            )
            .arg(Arg::new("note").long("note").help("What changed")),
        )
        .subcommand(
            command(
                "download",
                "Download one document as markdown, or every document as a zip",
            )
            .arg(
                Arg::new("document_id")
                    .num_args(0..)
                    .help("Document IDs to download as markdown files"),
            )
            .arg(
                Arg::new("all")
                    .long("all")
                    .help("Download every document as one zip (README index, manifest, one file per document)")
                    .action(ArgAction::SetTrue)
                    .conflicts_with("document_id"),
            )
            .arg(
                Arg::new("approved")
                    .long("approved")
                    .help("Take the approved version instead of the newest (with --all this is the default)")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("include")
                    .long("include")
                    .value_parser(["approved", "latest"])
                    .help("With --all: approved versions only (default) or the newest version of every document"),
            )
            .arg(
                Arg::new("app")
                    .long("app")
                    .help("With --all: only documents that cover this application ID"),
            )
            .arg(
                Arg::new("format")
                    .long("format")
                    .value_parser(["md", "pdf", "both"])
                    .help("md, pdf (diagrams drawn, rendered by the platform) or both. Default: md for documents, both for --all"),
            )
            .arg(
                Arg::new("output")
                    .long("output")
                    .short('o')
                    .help("Directory (markdown) or file path (zip). Default: current directory"),
            ),
        )
        .subcommand(
            command(
                "push",
                "Send approved versions to Oneleet, attached to the controls each document is evidence for (connect Oneleet in the dashboard first)",
            )
            .arg(
                Arg::new("document_id")
                    .num_args(0..)
                    .help("Document IDs to send"),
            )
            .arg(
                Arg::new("all")
                    .long("all")
                    .help("Send every approved document in the organization")
                    .action(ArgAction::SetTrue)
                    .conflicts_with("document_id"),
            )
            .arg(json_flag()),
        )
        .subcommand(
            command(
                "delete",
                "Delete documents that were never approved (duplicate copies, mistakes)",
            )
            .arg(
                Arg::new("document_id")
                    .num_args(1..)
                    .required(true)
                    .help("Document IDs to delete"),
            ),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        require_auth()?;
        match matches.subcommand() {
            Some(("list", m)) => list(m),
            Some(("generate", m)) => generate(m),
            Some(("regenerate", m)) => regenerate(m),
            Some(("show", m)) => show(m),
            Some(("save", m)) => save(m),
            Some(("download", m)) => download(m),
            Some(("push", m)) => push(m),
            Some(("delete", m)) => delete(m),
            _ => unreachable!(),
        }
    }
}

fn list(m: &ArgMatches) -> Result<()> {
    let entries = fetch_catalog(m.get_one::<String>("app"))?;
    if m.get_flag("json") {
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }
    for entry in &entries {
        let kind = if entry.scope == "organization" {
            "company-wide"
        } else {
            "per system"
        };
        println!("{} ({}, {})", entry.title, entry.document_type, kind);
        if entry.documents.is_empty() {
            println!("    not generated yet");
        }
        for d in &entry.documents {
            let apps = d
                .application_ids
                .as_ref()
                .map(|a| format!(", {} app(s)", a.len()))
                .unwrap_or_default();
            println!(
                "    {}  {}  v{} {}{}{}",
                d.document_id,
                d.title,
                d.latest_version,
                d.status,
                apps,
                if d.open_inputs > 0 {
                    format!(", {} input(s) open", d.open_inputs)
                } else {
                    String::new()
                }
            );
        }
    }
    Ok(())
}

fn generate(m: &ArgMatches) -> Result<()> {
    let document_type = m.get_one::<String>("type").context("type is required")?;
    let mut body = json!({ "documentType": document_type });
    if let Some(apps) = split_list(m.get_one::<String>("apps")) {
        body["applicationIds"] = json!(apps);
    }
    if let Some(title) = m.get_one::<String>("title") {
        body["title"] = json!(title);
    }
    let response =
        http_client::post(&base(), body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        return Err(fail("Failed to generate document", response));
    }
    let doc: DocumentDetail = response.json().context("Failed to parse document")?;
    if m.get_flag("json") {
        println!("{}", serde_json::to_string_pretty(&doc)?);
    } else {
        print_document_line(&doc);
    }
    Ok(())
}

fn regenerate_one(id: &str, apps: Option<Vec<String>>) -> Result<DocumentDetail> {
    let body = match apps {
        Some(a) => json!({ "applicationIds": a }),
        None => json!({}),
    };
    let url = format!("{}/{}/regenerate", base(), urlencoding::encode(id));
    let response = http_client::post(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        return Err(fail(&format!("Failed to regenerate {id}"), response));
    }
    response.json().context("Failed to parse document")
}

fn all_document_ids() -> Result<Vec<String>> {
    Ok(fetch_catalog(None)?
        .into_iter()
        .flat_map(|e| e.documents.into_iter().map(|d| d.document_id))
        .collect())
}

fn requested_ids(m: &ArgMatches) -> Result<Vec<String>> {
    if m.get_flag("all") {
        return all_document_ids();
    }
    let ids: Vec<String> = m
        .get_many::<String>("document_id")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();
    if ids.is_empty() {
        bail!("Give one or more document IDs, or --all (see `forklaunch compliance docs list`)");
    }
    Ok(ids)
}

fn regenerate(m: &ArgMatches) -> Result<()> {
    let ids = requested_ids(m)?;
    let apps = split_list(m.get_one::<String>("apps"));
    if apps.is_some() && ids.len() != 1 {
        bail!("--apps changes one document's scope; give exactly one document ID");
    }
    let mut done = Vec::new();
    let mut failed = 0;
    for id in &ids {
        match regenerate_one(id, apps.clone()) {
            Ok(doc) => {
                if !m.get_flag("json") {
                    print_document_line(&doc);
                }
                done.push(doc);
            }
            Err(e) => {
                failed += 1;
                eprintln!("{e:#}");
            }
        }
    }
    if m.get_flag("json") {
        println!("{}", serde_json::to_string_pretty(&done)?);
    }
    if failed > 0 {
        bail!("{failed} of {} document(s) failed to regenerate", ids.len());
    }
    Ok(())
}

fn delete(m: &ArgMatches) -> Result<()> {
    let ids = requested_ids_only(m);
    let mut failed = 0;
    for id in &ids {
        let url = format!("{}/{}", base(), urlencoding::encode(id));
        let response = http_client::delete(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if response.status().is_success() {
            println!("{id}  deleted");
        } else {
            failed += 1;
            eprintln!("{:#}", fail(&format!("Failed to delete {id}"), response));
        }
    }
    if failed > 0 {
        bail!("{failed} of {} document(s) were not deleted", ids.len());
    }
    Ok(())
}

fn requested_ids_only(m: &ArgMatches) -> Vec<String> {
    m.get_many::<String>("document_id")
        .map(|v| v.cloned().collect())
        .unwrap_or_default()
}

fn push_one(id: &str) -> Result<PushResult> {
    let url = format!("{}/{}/push", base(), urlencoding::encode(id));
    let response =
        http_client::post(&url, json!({})).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        return Err(fail(&format!("Failed to send {id} to Oneleet"), response));
    }
    response.json().context("Failed to parse push response")
}

fn push(m: &ArgMatches) -> Result<()> {
    let ids = if m.get_flag("all") {
        // Only approved documents: a draft would be refused anyway.
        let ids: Vec<String> = fetch_catalog(None)?
            .into_iter()
            .flat_map(|e| e.documents)
            .filter(|d| d.approved_version.is_some())
            .map(|d| d.document_id)
            .collect();
        if ids.is_empty() {
            bail!("No approved documents to send. Approve documents in the dashboard first.");
        }
        ids
    } else {
        requested_ids(m)?
    };
    let mut done = Vec::new();
    let mut failed = 0;
    for id in &ids {
        match push_one(id) {
            Ok(result) => {
                if !m.get_flag("json") {
                    let target = if result.controls.is_empty() {
                        "the evidence library".to_string()
                    } else {
                        result
                            .controls
                            .iter()
                            .map(|c| c.title.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    println!("{}  v{} -> {target}", result.file_name, result.version);
                }
                done.push(result);
            }
            Err(e) => {
                failed += 1;
                eprintln!("{e:#}");
            }
        }
    }
    if m.get_flag("json") {
        println!("{}", serde_json::to_string_pretty(&done)?);
    }
    if failed > 0 {
        bail!("{failed} of {} document(s) were not sent", ids.len());
    }
    Ok(())
}

fn show(m: &ArgMatches) -> Result<()> {
    let id = m
        .get_one::<String>("document_id")
        .context("document id is required")?;
    let doc = fetch_document(id)?;
    if m.get_flag("json") {
        println!("{}", serde_json::to_string_pretty(&doc)?);
    } else {
        print!("{}", doc.content);
        if !doc.content.ends_with('\n') {
            println!();
        }
    }
    Ok(())
}

fn save(m: &ArgMatches) -> Result<()> {
    let id = m
        .get_one::<String>("document_id")
        .context("document id is required")?;
    let file = m.get_one::<String>("file").context("--file is required")?;
    let content = fs::read_to_string(file).with_context(|| format!("Failed to read {file}"))?;
    let base_version = match m.get_one::<String>("base_version") {
        Some(v) => v
            .parse::<u32>()
            .context("--base-version must be a number")?,
        None => fetch_document(id)?.latest_version,
    };
    let mut body = json!({
        "content": content,
        "baseVersion": base_version,
        "source": "manual",
    });
    if let Some(note) = m.get_one::<String>("note") {
        body["note"] = Value::String(note.clone());
    }
    let url = format!("{}/{}", base(), urlencoding::encode(id));
    let response = http_client::put(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if response.status().as_u16() == 409 {
        bail!(
            "Someone saved a newer version since v{base_version}. Run `forklaunch compliance docs show {id}` and re-apply your edit."
        );
    }
    if !response.status().is_success() {
        return Err(fail("Failed to save document", response));
    }
    let doc: DocumentDetail = response.json().context("Failed to parse document")?;
    print_document_line(&doc);
    Ok(())
}

/// `--include` and `--app` shape the zip, so they need `--all`. Checked here:
/// clap's `requires` treats an unset SetTrue flag as present (default false).
fn check_download_flags(m: &ArgMatches) -> Result<()> {
    let given = |id: &str| m.value_source(id) == Some(ValueSource::CommandLine);
    if !m.get_flag("all") && (given("include") || given("app")) {
        bail!("--include and --app apply to the zip; add --all");
    }
    Ok(())
}

fn download(m: &ArgMatches) -> Result<()> {
    check_download_flags(m)?;
    let output = m.get_one::<String>("output").map(PathBuf::from);
    if m.get_flag("all") {
        return download_zip(m, output);
    }
    let format = m
        .get_one::<String>("format")
        .map(String::as_str)
        .unwrap_or("md");
    let dir = output.unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    for id in requested_ids(m)? {
        let doc = fetch_document(&id)?;
        let (version, content) = if m.get_flag("approved") {
            let Some(v) = doc.approved_version else {
                bail!("{} ({}) has no approved version yet", doc.title, doc.id);
            };
            let url = format!("{}/{}/versions/{}", base(), urlencoding::encode(&id), v);
            let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
            if !response.status().is_success() {
                return Err(fail("Failed to load the approved version", response));
            }
            let detail: VersionDetail = response.json().context("Failed to parse version")?;
            (detail.version, detail.content)
        } else {
            (doc.latest_version, doc.content.clone())
        };
        let approved = doc.approved_version == Some(version);
        let stem = format!(
            "{}-v{}{}",
            file_slug(&doc.title),
            version,
            if approved { "" } else { "-DRAFT" }
        );
        if format != "pdf" {
            let path = dir.join(format!("{stem}.md"));
            fs::write(&path, &content)
                .with_context(|| format!("Failed to write {}", path.display()))?;
            println!("{}", path.display());
        }
        if format != "md" {
            let url = format!(
                "{}/{}/pdf?version={}",
                base(),
                urlencoding::encode(&id),
                version
            );
            let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
            if !response.status().is_success() {
                return Err(fail("Failed to render the PDF", response));
            }
            let bytes = response.bytes().context("Failed to read the PDF")?;
            let path = dir.join(format!("{stem}.pdf"));
            fs::write(&path, &bytes)
                .with_context(|| format!("Failed to write {}", path.display()))?;
            println!("{}", path.display());
        }
    }
    Ok(())
}

fn download_zip(m: &ArgMatches, output: Option<PathBuf>) -> Result<()> {
    let include = m
        .get_one::<String>("include")
        .cloned()
        .unwrap_or_else(|| "approved".to_string());
    let format = m
        .get_one::<String>("format")
        .map(String::as_str)
        .unwrap_or("both");
    let mut url = format!("{}/export?include={}&format={}", base(), include, format);
    if let Some(app) = m.get_one::<String>("app") {
        url.push_str(&format!("&applicationId={}", urlencoding::encode(app)));
    }
    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        return Err(fail("Failed to export documents", response));
    }
    let bytes = response.bytes().context("Failed to read the zip")?;
    let path = match output {
        Some(p) if p.is_dir() => p.join(format!("compliance-documents-{include}.zip")),
        Some(p) => p,
        None => Path::new(".").join(format!("compliance-documents-{include}.zip")),
    };
    fs::write(&path, &bytes).with_context(|| format!("Failed to write {}", path.display()))?;
    println!("{} ({} bytes)", path.display(), bytes.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs_cmd() -> Command {
        DocsCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        docs_cmd().debug_assert();
    }

    #[test]
    fn regenerate_takes_ids_or_all_but_not_both() {
        assert!(
            docs_cmd()
                .try_get_matches_from(["docs", "regenerate", "--all"])
                .is_ok()
        );
        assert!(
            docs_cmd()
                .try_get_matches_from(["docs", "regenerate", "a", "b"])
                .is_ok()
        );
        assert!(
            docs_cmd()
                .try_get_matches_from(["docs", "regenerate", "a", "--all"])
                .is_err()
        );
        assert!(
            docs_cmd()
                .try_get_matches_from(["docs", "regenerate", "--all", "--apps", "x"])
                .is_err()
        );
    }

    #[test]
    fn push_takes_ids_or_all_but_not_both() {
        assert!(
            docs_cmd()
                .try_get_matches_from(["docs", "push", "--all"])
                .is_ok()
        );
        assert!(
            docs_cmd()
                .try_get_matches_from(["docs", "push", "a"])
                .is_ok()
        );
        assert!(
            docs_cmd()
                .try_get_matches_from(["docs", "push", "a", "--all"])
                .is_err()
        );
    }

    #[test]
    fn push_result_deserializes() {
        let r: PushResult = serde_json::from_str(
            r#"{"documentId":"d","version":3,"fileName":"x-v3.pdf","controls":[{"controlId":"c","title":"Automated backups","evidenceId":"e"}]}"#,
        )
        .unwrap();
        assert_eq!(r.controls[0].title, "Automated backups");
    }

    #[test]
    fn delete_needs_an_id() {
        assert!(docs_cmd().try_get_matches_from(["docs", "delete"]).is_err());
        assert!(
            docs_cmd()
                .try_get_matches_from(["docs", "delete", "a", "b"])
                .is_ok()
        );
    }

    fn download_matches(args: &[&str]) -> ArgMatches {
        let m = docs_cmd().try_get_matches_from(args).unwrap();
        m.subcommand_matches("download").unwrap().clone()
    }

    #[test]
    fn download_include_needs_all() {
        let ok = download_matches(&["docs", "download", "--all", "--include", "latest"]);
        assert!(check_download_flags(&ok).is_ok());
        let bad = download_matches(&["docs", "download", "a", "--include", "latest"]);
        assert!(check_download_flags(&bad).is_err());
        let plain = download_matches(&["docs", "download", "a"]);
        assert!(check_download_flags(&plain).is_ok());
    }

    #[test]
    fn save_requires_file() {
        assert!(
            docs_cmd()
                .try_get_matches_from(["docs", "save", "a"])
                .is_err()
        );
        assert!(
            docs_cmd()
                .try_get_matches_from(["docs", "save", "a", "--file", "x.md"])
                .is_ok()
        );
    }

    #[test]
    fn file_slug_is_safe() {
        assert_eq!(
            file_slug("Data-Flow and Data Inventory: Organ Alert"),
            "data-flow-and-data-inventory-organ-alert"
        );
    }

    #[test]
    fn catalog_deserializes() {
        let json = r#"{"documents":[{"type":"data-flow-inventory","title":"Data-Flow","summary":"s","scope":"applications","controls":{"soc2":["CC2.1"]},"documents":[{"documentId":"d1","title":"Data-Flow","status":"draft","latestVersion":2,"openInputs":1,"applicationIds":["a1"],"updatedAt":"2026-11-09T00:00:00Z"}]}]}"#;
        let parsed: CatalogResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.documents[0].documents[0].latest_version, 2);
    }
}
