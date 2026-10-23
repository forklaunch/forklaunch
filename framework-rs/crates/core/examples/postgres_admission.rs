//! Disposable database proof; accepts only the named loopback fixture database.
use database::{
    PostgresConnectionPolicy, begin_tenant_transaction, clone_runtime_postgres_pool,
    connect_migration_postgres, connect_runtime_postgres,
};
#[cfg(not(feature = "postgres09"))]
use forklaunch_core::persistence as database;
#[cfg(feature = "postgres09")]
use forklaunch_core::persistence::postgres09 as database;
use forklaunch_core::persistence::{TenantContext, postgres_tenant_rls_sql};
use sqlx as driver;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::var("FORKLAUNCH_ADMISSION_DATABASE_URL")?;
    let url = url::Url::parse(&endpoint)?;
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert_eq!(url.path(), "/forklaunch_admission_proof");
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let policy = PostgresConnectionPolicy { max_connections: 1, ..Default::default() };
        let admin = connect_migration_postgres(&endpoint, policy).await?;
        driver::raw_sql("CREATE ROLE safe_app LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEROLE; CREATE ROLE unsafe_app LOGIN NOSUPERUSER NOBYPASSRLS; CREATE ROLE bypass_group NOLOGIN BYPASSRLS; GRANT bypass_group TO unsafe_app; GRANT safe_app TO unsafe_app; CREATE TABLE records(tenant_id text NOT NULL, value text NOT NULL); GRANT SELECT,INSERT ON records TO safe_app;")
            .execute(&admin).await?;
        let rls_sql = postgres_tenant_rls_sql("records")?;
        #[cfg(not(feature = "postgres09"))]
        driver::raw_sql(&rls_sql).execute(&admin).await?;
        #[cfg(feature = "postgres09")]
        driver::raw_sql(driver::AssertSqlSafe(rls_sql)).execute(&admin).await?;
        assert!(connect_runtime_postgres(&endpoint, policy).await.is_err(), "superuser accepted");
        let mut restricted = url.clone();
        restricted.set_username("unsafe_app").unwrap();
        assert!(connect_runtime_postgres(restricted.as_str(), policy).await.is_err(), "privileged membership accepted");
        for login in ["postgres", "unsafe_app"] {
            let mut masked = url.clone();
            masked.set_username(login).unwrap();
            masked.query_pairs_mut().append_pair("options", "-c role=safe_app");
            // Verify the fixture genuinely starts with a restricted current role
            // while retaining the privileged session identity.
            let disguised = connect_migration_postgres(masked.as_str(), policy).await?;
            let identities: (String, String) = driver::query_as("SELECT current_user::text,session_user::text").fetch_one(&disguised).await?;
            assert_eq!(identities, ("safe_app".into(), login.into()));
            assert!(connect_runtime_postgres(masked.as_str(), policy).await.is_err(), "privileged session accepted through SET ROLE");
            let audit = clone_runtime_postgres_pool(&disguised, policy)?;
            assert!(audit.acquire().await.is_err(), "privileged session accepted in cloned audit pool");
            audit.close().await;
            disguised.close().await;
        }
        restricted.set_username("safe_app").unwrap();
        let pool = connect_runtime_postgres(restricted.as_str(), policy).await?;
        let tenant = TenantContext::new("tenant-a")?;
        let mut tx = begin_tenant_transaction(&pool, &tenant).await?;
        driver::query("INSERT INTO records VALUES('tenant-a','fixture')").execute(&mut *tx).await?;
        tx.commit().await?;
        let unscoped: i64 = driver::query_scalar("SELECT count(*) FROM records").fetch_one(&pool).await?;
        assert_eq!(unscoped, 0, "pooled transaction scope leaked");
        let mut tx = begin_tenant_transaction(&pool, &tenant).await?;
        let count: i64 = driver::query_scalar("SELECT count(*) FROM records").fetch_one(&mut *tx).await?;
        assert_eq!(count, 1);
        assert!(driver::query("INSERT INTO records VALUES('tenant-b','denied')").execute(&mut *tx).await.is_err());
        tx.rollback().await?;
        pool.close().await;
        admin.close().await;
        println!("PASS runtime superuser/membership/masked-session denial, migration-owner connection, forced RLS and pooled scope reset");
        Ok(())
    })
}
