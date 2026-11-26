use crate::common::{FrameworkError, FrameworkResult};
use driver::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};
use std::{net::IpAddr, time::Duration};

#[derive(Clone, Copy, Debug)]
pub struct PostgresConnectionPolicy {
    /// Explicit exception for an isolated local development network.
    pub allow_plaintext_development: bool,
    pub max_connections: u32,
}

impl Default for PostgresConnectionPolicy {
    fn default() -> Self {
        Self {
            allow_plaintext_development: false,
            max_connections: 8,
        }
    }
}

impl PostgresConnectionPolicy {
    /// Read the required `DATABASE_ALLOW_PLAINTEXT_DEVELOPMENT` setting: `true`
    /// only on an isolated local network, `false` (verified TLS) everywhere else.
    /// # Errors
    /// Rejects a missing or malformed boolean value and invalid pool sizes.
    pub fn from_env(max_connections: u32) -> FrameworkResult<Self> {
        let allow: String = crate::config::required("DATABASE_ALLOW_PLAINTEXT_DEVELOPMENT")?;
        let allow_plaintext_development = match allow.as_str() {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => return Err(configuration()),
        };
        let policy = Self {
            allow_plaintext_development,
            max_connections,
        };
        validate_policy(policy)?;
        Ok(policy)
    }
}

fn validate_policy(policy: PostgresConnectionPolicy) -> FrameworkResult<()> {
    if policy.max_connections == 0 || policy.max_connections > 128 {
        return Err(configuration());
    }
    Ok(())
}

fn configuration() -> FrameworkError {
    FrameworkError::new(
        "postgres_configuration",
        "Invalid PostgreSQL connection policy",
    )
}

fn options(endpoint: &str, policy: PostgresConnectionPolicy) -> FrameworkResult<PgConnectOptions> {
    validate_policy(policy)?;
    let url = url::Url::parse(endpoint).map_err(|_| configuration())?;
    if !matches!(url.scheme(), "postgres" | "postgresql")
        || url.host().is_none()
        || url.fragment().is_some()
    {
        return Err(configuration());
    }
    let mut mode_seen = false;
    for (key, _) in url.query_pairs() {
        match key.as_ref() {
            "host" | "hostaddr" | "port" => return Err(configuration()),
            "sslmode" | "ssl-mode" => {
                if mode_seen {
                    return Err(configuration());
                }
                mode_seen = true;
            }
            _ => {}
        }
    }
    let options: PgConnectOptions = endpoint.parse().map_err(|_| configuration())?;
    validate_options(&options, policy)?;
    Ok(options)
}

fn validate_options(
    options: &PgConnectOptions,
    policy: PostgresConnectionPolicy,
) -> FrameworkResult<()> {
    let host = options.get_host().trim_matches(['[', ']']);
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
    if options.get_socket().is_some()
        || (!loopback
            && !policy.allow_plaintext_development
            && !matches!(options.get_ssl_mode(), PgSslMode::VerifyFull))
    {
        return Err(configuration());
    }
    Ok(())
}

/// Opens an application pool with verified remote TLS and restricted roles.
/// Every new physical connection rejects superuser/RLS-bypass/create-role
/// privileges for both the login session and active role, including reachable
/// privileged role memberships. SET ROLE cannot mask a privileged login. Migrations
/// must still use FORCE ROW LEVEL SECURITY and a separate table owner.
///
/// # Errors
/// Returns sanitized configuration or connection errors, never DSN credentials.
pub async fn connect_runtime_postgres(
    endpoint: &str,
    policy: PostgresConnectionPolicy,
) -> FrameworkResult<PgPool> {
    let options = options(endpoint, policy)?;
    runtime_pool_options(policy)
        .connect_with(options)
        .await
        .map_err(|_| {
            FrameworkError::new(
                "postgres_connection",
                "Unable to establish a restricted database connection",
            )
        })
}

fn runtime_pool_options(policy: PostgresConnectionPolicy) -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(policy.max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .after_connect(|connection, _| Box::pin(async move {
            let privileged: bool = driver::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_roles WHERE (rolsuper OR rolbypassrls OR rolcreaterole) AND (rolname=current_user OR rolname=session_user OR pg_has_role(current_user,oid,'MEMBER') OR pg_has_role(session_user,oid,'MEMBER')))")
                .fetch_one(connection).await?;
            if privileged {
                return Err(driver::Error::Protocol("Runtime database role must not bypass tenant isolation".into()));
            }
            Ok(())
        }))
}

/// Begin a transaction with a tenant scope that cannot survive pool reuse.
/// # Errors
/// Returns a sanitized error when the transaction or scope cannot be established.
pub async fn begin_tenant_transaction<'a>(
    pool: &'a PgPool,
    tenant: &crate::persistence::TenantContext,
) -> FrameworkResult<driver::Transaction<'a, driver::Postgres>> {
    let error = |_| FrameworkError::new("persistence_error", "Unable to begin tenant transaction");
    let mut transaction = pool.begin().await.map_err(error)?;
    driver::query("SELECT set_config('forklaunch.tenant_id', $1, true)")
        .bind(tenant.id())
        .execute(&mut *transaction)
        .await
        .map_err(error)?;
    Ok(transaction)
}

/// Create a separately bounded audit pool using an existing pool's connection
/// options. Transport is rechecked and every connection checks runtime roles.
/// This is lazy so constructing application state never blocks an executor.
/// # Errors
/// Rejects invalid pool sizing or unverified remote transport.
pub fn clone_runtime_postgres_pool(
    pool: &PgPool,
    policy: PostgresConnectionPolicy,
) -> FrameworkResult<PgPool> {
    validate_policy(policy)?;
    let options = (*pool.connect_options()).clone();
    validate_options(&options, policy)?;
    Ok(runtime_pool_options(policy).connect_lazy_with(options))
}

/// Opens a short-lived migration-owner pool with the same transport policy.
/// Keep this credential out of the running service and close the pool when the
/// service-owned migration command finishes.
///
/// # Errors
/// Rejects unsafe transport configuration or unavailable databases.
pub async fn connect_migration_postgres(
    endpoint: &str,
    policy: PostgresConnectionPolicy,
) -> FrameworkResult<PgPool> {
    let options = options(endpoint, policy)?;
    PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
        .map_err(|_| {
            FrameworkError::new(
                "postgres_connection",
                "Unable to establish a migration database connection",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_tls_and_driver_routing_are_checked_before_connecting() {
        let policy = PostgresConnectionPolicy::default();
        for endpoint in [
            "postgres://db.example/app",
            "postgres://db.example/app?sslmode=require",
            "postgres://db.example/app?sslmode=verify-ca",
            "postgres://localhost/app?host=db.example",
            "postgres://localhost/app?sslmode=disable&ssl-mode=verify-full",
            "http://localhost/app",
            "postgres://localhost/app#fragment",
        ] {
            assert!(options(endpoint, policy).is_err(), "{endpoint}");
        }
        for endpoint in [
            "postgres://db.example/app?sslmode=verify-full",
            "postgres://127.0.0.1/app",
            "postgres://[::1]/app",
        ] {
            assert!(options(endpoint, policy).is_ok(), "{endpoint}");
        }
        assert!(
            options(
                "postgres://postgres/app",
                PostgresConnectionPolicy {
                    allow_plaintext_development: true,
                    ..policy
                }
            )
            .is_ok()
        );
        assert!(
            options(
                "postgres://localhost/app",
                PostgresConnectionPolicy {
                    max_connections: 0,
                    ..policy
                }
            )
            .is_err()
        );
    }
}
