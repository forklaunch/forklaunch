use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::CliCommand;
use crate::{
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        env::{parse_env_file_items, EnvFileItem},
        http_client,
        validate::{require_auth, require_integration, require_manifest},
    },
};

/// Reconstruct env file content from parsed items for pushing to the platform.
/// Section headers are preserved as-is; key-value pairs are formatted as `KEY=value`
/// (multiline values are double-quoted).
pub(crate) fn reconstruct_env_content(items: Vec<EnvFileItem>) -> String {
    items
        .into_iter()
        .map(|item| match item {
            EnvFileItem::SectionHeader(line) => format!("{}\n", line),
            EnvFileItem::KeyValue(key, value) => {
                if value.contains('\n') {
                    format!("{}=\"{}\"\n", key, value)
                } else {
                    format!("{}={}\n", key, value)
                }
            }
        })
        .collect::<String>()
}

#[derive(Debug)]
pub(crate) struct PushCommand;

impl PushCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for PushCommand {
    fn command(&self) -> Command {
        command(
            "push",
            "Push environment configuration to the forklaunch platform",
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .required(true)
                .help("Region (e.g. us-east-1)"),
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .required(true)
                .help("Environment name (e.g. production, staging)"),
        )
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .required(false)
                .help("Input file path (defaults to <environment>.env)"),
        )
        .arg(
            Arg::new("base_path")
                .long("path")
                .short('p')
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let app = require_integration(&manifest)?;

        let region = matches
            .get_one::<String>("region")
            .expect("region is required");
        let environment = matches
            .get_one::<String>("environment")
            .expect("environment is required");

        let input = format!("{}.env", environment);
        let input = matches.get_one::<String>("input").unwrap_or(&input);

        let url = format!(
            "{}/config/push",
            get_platform_management_api_url()
        );

        let items = parse_env_file_items(std::path::Path::new(input))
            .with_context(|| format!("Failed to parse file {}. Please check file permissions.", input))?;

        let content = reconstruct_env_content(items);

        let body = serde_json::json!({
            "applicationId": app,
            "region": region,
            "environment": environment,
            "content": content
        });

        let response =
            http_client::post(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        match response.status() {
            reqwest::StatusCode::OK => {
                log_ok!(stdout, "Config pushed successfully for {} ({})", environment, region);
            }
            _ => {
                let err_text = response.text()?;
                log_error!(stdout, "Failed to push config: {}", err_text);
                anyhow::bail!("Failed to push config: {}", err_text);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    /// Helper: parse an env file and reconstruct content, simulating the push path.
    fn parse_and_reconstruct(path: &std::path::Path) -> String {
        let items = parse_env_file_items(path).unwrap();
        reconstruct_env_content(items)
    }

    /// Helper: extract key names from reconstructed content (ignoring section headers).
    fn extract_keys(content: &str) -> Vec<String> {
        content
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .filter_map(|line| line.split_once('=').map(|(k, _)| k.to_string()))
            .collect()
    }

    #[test]
    fn test_push_content_only_includes_present_vars() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("production.env");

        // Initial file with 5 vars
        fs::write(
            &path,
            "# application\n\
             DB_HOST=db.example.com\n\
             DB_PORT=5432\n\
             GOOGLE_CLOUD_LOCATION=us-central1\n\
             GOOGLE_CLOUD_API_KEY=abc123\n\
             REDIS_URL=redis://localhost:6379\n",
        )
        .unwrap();

        let content = parse_and_reconstruct(&path);
        let keys = extract_keys(&content);
        assert_eq!(keys.len(), 5);
        assert!(keys.contains(&"GOOGLE_CLOUD_LOCATION".to_string()));
        assert!(keys.contains(&"GOOGLE_CLOUD_API_KEY".to_string()));

        // User deletes two vars from the env file (simulates deletion before re-push)
        fs::write(
            &path,
            "# application\n\
             DB_HOST=db.example.com\n\
             DB_PORT=5432\n\
             REDIS_URL=redis://localhost:6379\n",
        )
        .unwrap();

        let content = parse_and_reconstruct(&path);
        let keys = extract_keys(&content);

        // Deleted vars must NOT appear in push content
        assert_eq!(keys.len(), 3);
        assert!(!keys.contains(&"GOOGLE_CLOUD_LOCATION".to_string()));
        assert!(!keys.contains(&"GOOGLE_CLOUD_API_KEY".to_string()));

        // Remaining vars must still be present
        assert!(keys.contains(&"DB_HOST".to_string()));
        assert!(keys.contains(&"DB_PORT".to_string()));
        assert!(keys.contains(&"REDIS_URL".to_string()));
    }

    #[test]
    fn test_push_content_preserves_section_headers() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("staging.env");

        fs::write(
            &path,
            "# application\n\
             APP_NAME=myapp\n\
             # my-service (svc-123)\n\
             DB_HOST=db.example.com\n\
             # video-processor-worker (wkr-456)\n\
             QUEUE_URL=sqs://queue\n",
        )
        .unwrap();

        let content = parse_and_reconstruct(&path);

        // Section headers preserved exactly
        assert!(content.contains("# application\n"));
        assert!(content.contains("# my-service (svc-123)\n"));
        assert!(content.contains("# video-processor-worker (wkr-456)\n"));

        // All key-value pairs present
        assert!(content.contains("APP_NAME=myapp\n"));
        assert!(content.contains("DB_HOST=db.example.com\n"));
        assert!(content.contains("QUEUE_URL=sqs://queue\n"));
    }

    #[test]
    fn test_push_content_multiline_values_are_quoted() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("production.env");

        fs::write(
            &path,
            "SIMPLE=hello\n\
             CERT=\"-----BEGIN CERTIFICATE-----\n\
             abc123\n\
             -----END CERTIFICATE-----\"\n",
        )
        .unwrap();

        let content = parse_and_reconstruct(&path);

        // Simple value: no quotes
        assert!(content.contains("SIMPLE=hello\n"));

        // Multiline value: must be double-quoted in reconstructed output
        assert!(content.contains("CERT=\"-----BEGIN CERTIFICATE-----\nabc123\n-----END CERTIFICATE-----\"\n"));
    }

    #[test]
    fn test_push_content_empty_file_produces_empty_string() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("empty.env");

        fs::write(&path, "").unwrap();

        let content = parse_and_reconstruct(&path);
        assert!(content.is_empty());
    }

    #[test]
    fn test_push_content_nonexistent_file_produces_empty_string() {
        let content = parse_and_reconstruct(std::path::Path::new("/nonexistent/file.env"));
        assert!(content.is_empty());
    }

    #[test]
    fn test_push_re_adding_deleted_var_includes_it() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("production.env");

        // Step 1: file with 3 vars
        fs::write(
            &path,
            "# application\nA=1\nB=2\nC=3\n",
        )
        .unwrap();

        let keys = extract_keys(&parse_and_reconstruct(&path));
        assert_eq!(keys.len(), 3);

        // Step 2: user removes B
        fs::write(
            &path,
            "# application\nA=1\nC=3\n",
        )
        .unwrap();

        let keys = extract_keys(&parse_and_reconstruct(&path));
        assert_eq!(keys.len(), 2);
        assert!(!keys.contains(&"B".to_string()));

        // Step 3: user explicitly re-adds B (this is an intentional restore)
        fs::write(
            &path,
            "# application\nA=1\nB=new_value\nC=3\n",
        )
        .unwrap();

        let keys = extract_keys(&parse_and_reconstruct(&path));
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"B".to_string()));

        let content = parse_and_reconstruct(&path);
        assert!(content.contains("B=new_value\n"));
    }

    #[test]
    fn test_push_json_body_structure() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("production.env");

        fs::write(
            &path,
            "# application\nDB_HOST=localhost\n",
        )
        .unwrap();

        let items = parse_env_file_items(&path).unwrap();
        let content = reconstruct_env_content(items);

        let body = serde_json::json!({
            "applicationId": "app-123",
            "region": "us-east-1",
            "environment": "production",
            "content": content
        });

        assert_eq!(body["applicationId"], "app-123");
        assert_eq!(body["region"], "us-east-1");
        assert_eq!(body["environment"], "production");

        let content_str = body["content"].as_str().unwrap();
        assert!(content_str.contains("# application\n"));
        assert!(content_str.contains("DB_HOST=localhost\n"));
    }
}
