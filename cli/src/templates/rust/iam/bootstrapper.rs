//! Process lifecycle: contract export, migrations, telemetry, registration and serving.
use crate::{config::ServiceConfig, domain::services::PostgresIamService, error::Result, persistence::database, registrations, server, write_contract_artifacts};
use forklaunch_core::{
    config,
    domain::{AuditContext, BlueprintError},
    persistence::{TenantContext, encryption::FieldEncryptor},
    telemetry::otlp::TelemetryRuntime,
};
use forklaunch_iam_base::{directory::postgres::PostgresDirectoryStore, seed::SeedConfig};
use std::{env, sync::Arc};
use tokio::net::TcpListener;

pub async fn run() -> Result<()> {
    if matches!(config::optional::<String>("FORKLAUNCH_MODE")?.as_deref(), Some("openapi" | "contract")) {
        write_contract_artifacts(
            config::required::<String>("FORKLAUNCH_CONTRACT_OUTPUT")?,
            config::required::<String>("FORKLAUNCH_OPENAPI_OUTPUT")?,
        )?;
        return Ok(());
    }
    // Migrations need only the owner database URL, not runtime settings such as RUST_LOG.
    if env::args().skip(1).eq(["--migrate"]) {
        return database::migrate().await;
    }
    let telemetry = TelemetryRuntime::start(crate::SERVICE_NAME, env!("CARGO_PKG_VERSION")).await?;
    let result = start().await;
    let flushed = telemetry.shutdown().await;
    result?;
    flushed?;
    Ok(())
}

async fn start() -> Result<()> {
    let arguments: Vec<_> = env::args().skip(1).collect();
    if !arguments.is_empty() && arguments != ["--migrate"] && arguments != ["--seed"] {
        return Err("Expected no arguments, --migrate or --seed".into());
    }
    if arguments == ["--migrate"] {
        return database::migrate().await;
    }
    if arguments == ["--seed"] {
        return seed().await;
    }
    let state = registrations::register(ServiceConfig::from_env()?).await?;
    let listener = TcpListener::bind(state.config.bind_addr()).await?;
    println!("{} listening on http://{}", crate::SERVICE_NAME, listener.local_addr()?);
    server::serve(&state, listener, server::shutdown()).await?;
    state.close().await;
    Ok(())
}

/// Creates or updates the roles and login users in `seed.toml` (or the file
/// named by `IAM_SEED_FILE`), reading each password from the variable the file
/// names, then the matching IAM directory records (permissions, roles,
/// organizations and a directory user under each login id), as `POST /user`
/// would. Idempotent; uses the restricted runtime `DATABASE_URL`, `TENANT_ID`
/// and the service's `ENCRYPTION_KEY`.
async fn seed() -> Result<()> {
    let source = match config::optional::<String>("IAM_SEED_FILE")? {
        Some(path) => std::fs::read_to_string(&path).map_err(|_| format!("Unable to read IAM_SEED_FILE {path}"))?,
        None => include_str!("seed.toml").to_owned(),
    };
    let seed = SeedConfig::from_toml(&source)?;
    let tenant = TenantContext::new(config::required::<String>("TENANT_ID")?)?;
    let stored_sessions = config::optional::<String>("IAM_DIRECTORY_AUTHORIZATION")?.as_deref() == Some("stored-session");
    let encryptor = Arc::new(FieldEncryptor::from_env()?);
    let pool = database::connect().await?;
    let result = async {
        let iam = PostgresIamService::new(pool.clone(), tenant.clone()).await?;
        let report = seed.apply(&iam, |name| env::var(name).ok()).await?;
        let actor = format!("{}-seed", crate::SERVICE_NAME);
        let directory = PostgresDirectoryStore::new(pool.clone(), tenant, encryptor, AuditContext::actor(&actor, &actor)).await?;
        seed.apply_directory(&directory, &report).await?;
        if stored_sessions {
            // Stored-session directory routes resolve grants through this link.
            for id in &report.user_ids {
                directory.bind_auth_identity(id, id).await?;
            }
        }
        Ok::<_, BlueprintError>(report)
    }
    .await;
    pool.close().await;
    let report = result?;
    println!(
        "{} seeded roles [{}]; created [{}]; updated [{}]",
        crate::SERVICE_NAME,
        report.roles.join(", "),
        report.created_users.join(", "),
        report.updated_users.join(", ")
    );
    Ok(())
}
