//! Builds the module's stores and HTTP handlers from trusted configuration.
use crate::{config::ServiceConfig, domain::services::PostgresIamService, error::Result, persistence::database, service_info};
use forklaunch_core::{
    auth::oidc::OidcProvider,
    domain::AuditContext,
    persistence::{TenantContext, encryption::FieldEncryptor},
};
use forklaunch_iam_base::{
    browser_sessions::PostgresCookieStore,
    directory::postgres::PostgresDirectoryStore,
    http::{DirectoryAuthorizationMode, IamHttp, IamRoutes, browser::BrowserIam},
    jwt_sessions::RsaJwtSessionIssuer,
};
use sqlx::PgPool;
use std::sync::Arc;

/// The module's routes, collected before mounting.
pub type Routes = IamRoutes<PostgresIamService>;

/// The IAM handle controllers take as their state.
pub type Iam = IamHttp<PostgresIamService>;

#[derive(Clone)]
pub struct AppState {
    pub config: ServiceConfig,
    pub pool: PgPool,
    pub iam: Iam,
    pub browser: Option<BrowserIam>,
}

impl AppState {
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

pub async fn register(config: ServiceConfig) -> Result<AppState> {
    let pool = database::connect().await?;
    let tenant = TenantContext::new(config.tenant_id.clone())?;
    let issuer = RsaJwtSessionIssuer::from_env()?;
    // Other services verify session JWTs against this key at /api/auth/jwks.
    let jwks = issuer.as_ref().map(|issuer| issuer.jwks().clone());
    if config.directory_mode == DirectoryAuthorizationMode::StoredSessions && issuer.is_none() {
        return Err("Stored-session directory authorization requires SESSION_JWT signing configuration".into());
    }
    let sessions = PostgresIamService::new(pool.clone(), tenant.clone()).await?;
    let sessions = Arc::new(match issuer {
        Some(issuer) => sessions.with_jwt_session_issuer(Arc::new(issuer)),
        None => sessions,
    });
    let encryptor = Arc::new(FieldEncryptor::from_env()?);
    let browser = match &config.browser_origin {
        Some(origin) => Some(browser(&config, origin, &pool, &tenant, &sessions, &encryptor).await?),
        None => None,
    };
    let directory = PostgresDirectoryStore::new(pool.clone(), tenant, encryptor, AuditContext::system("iam-startup")).await?;
    let iam = IamHttp::new(sessions, directory, config.module_auth()?, config.directory_mode, service_info());
    let iam = iam.with_cors_origins(&config.cors_origins)?;
    let iam = match jwks {
        Some(jwks) => iam.with_jwks(jwks),
        None => iam,
    };
    Ok(AppState { config, pool, iam, browser })
}

/// Cookie login over the persistent session ledger, with optional OIDC.
async fn browser(
    config: &ServiceConfig,
    origin: &str,
    pool: &PgPool,
    tenant: &TenantContext,
    sessions: &Arc<PostgresIamService>,
    encryptor: &Arc<FieldEncryptor>,
) -> Result<BrowserIam> {
    let cookie_store = PostgresCookieStore::new(pool.clone(), tenant.clone(), crate::SERVICE_NAME, encryptor.clone()).await?;
    let browser = BrowserIam::new(sessions.clone(), cookie_store.clone(), "__Host-iam", origin)?;
    let Some(oidc) = &config.oidc else { return Ok(browser) };
    let provider = OidcProvider::discover(
        &oidc.issuer,
        oidc.client_id.clone(),
        oidc.client_secret.clone(),
        &format!("{origin}/auth/browser/oidc/callback"),
    )
    .await?;
    Ok(browser.with_oidc(sessions.clone(), cookie_store, Arc::new(provider), vec![])?)
}
