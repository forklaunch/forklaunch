use super::*;
use sqlx::postgres::PgPoolOptions;

pub(super) async fn test_pools() -> (PgPool, PgPool, String) {
    let url =
        std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").expect("`PostgreSQL` test URL required");
    let owner = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .unwrap();
    PostgresIamService::migrate(&owner).await.unwrap();
    PostgresIamService::migrate(&owner).await.unwrap();
    let role = format!("fl_parity_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE ROLE {role} NOLOGIN NOSUPERUSER NOBYPASSRLS"
    )))
    .execute(&owner)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "GRANT USAGE ON SCHEMA public TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("GRANT SELECT,INSERT,UPDATE,DELETE ON fl_iam_users,fl_iam_organizations,fl_iam_roles,fl_iam_sessions,fl_iam_provider_identities,fl_iam_browser_sessions,fl_iam_oidc_pending TO {role}"))).execute(&owner).await.unwrap();
    let set_role = format!("SET ROLE {role}");
    let app = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |connection, _| {
            let set_role = set_role.clone();
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(set_role))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    (owner, app, role)
}

#[tokio::test]
#[ignore = "requires FORKLAUNCH_TEST_POSTGRES_URL; exercised in container CI"]
async fn postgres_backend_persists_crud_roles_sessions_and_enforces_tenant_rls() {
    let (owner, pool, role) = test_pools().await;
    let first = PostgresIamService::new(
        pool.clone(),
        TenantContext::new(format!("tenant-a-{}", uuid::Uuid::new_v4().simple())).unwrap(),
    )
    .await
    .unwrap();
    let second = first.for_tenant(
        TenantContext::new(format!("tenant-b-{}", uuid::Uuid::new_v4().simple())).unwrap(),
    );
    crud_and_roles(&first, &second).await;
    sessions(&first, &second).await;
    rls_and_pool_context(&first, &pool).await;
    assert!(first.delete_user("user-1").await.unwrap());
    assert!(!first.delete_user("user-1").await.unwrap());
    assert!(second.get_user("user-1").await.unwrap().is_some());
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP OWNED BY {role}")))
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE {role}")))
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

#[cfg(feature = "jwt-sessions")]
#[tokio::test]
#[ignore = "requires FORKLAUNCH_TEST_POSTGRES_URL; exercised in container CI"]
async fn postgres_jwt_sessions_preserve_persistent_revocation_and_tenant_binding() {
    let (owner, pool, role) = test_pools().await;
    let first = PostgresIamService::new(
        pool.clone(),
        TenantContext::new(format!("jwt-a-{}", uuid::Uuid::new_v4().simple())).unwrap(),
    )
    .await
    .unwrap()
    .with_jwt_session_issuer(Arc::new(crate::jwt_sessions::tests::issuer()));
    let second = first.for_tenant(
        TenantContext::new(format!("jwt-b-{}", uuid::Uuid::new_v4().simple())).unwrap(),
    );
    crud_and_roles(&first, &second).await;
    let session = first
        .authenticate(Credentials {
            email: "test@example.com".into(),
            password: "password-1".into(),
        })
        .await
        .unwrap();
    let claims = crate::jwt_sessions::tests::verify(&session.id);
    assert_eq!(claims["sub"], "user-1");
    assert_eq!(claims["tenant_id"], first.tenant.id());
    assert_eq!(claims["permissions"], serde_json::json!(["users:read"]));
    assert_eq!(claims["exp"], session.expires_at_unix_seconds);
    assert_eq!(
        claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(),
        3600
    );
    let mut tampered = session.id.clone();
    tampered.push('x');
    assert!(first.validate_session(&tampered).await.unwrap().is_none());
    assert!(
        second
            .validate_session(&session.id)
            .await
            .unwrap()
            .is_none()
    );
    // Restarting without an issuer still validates stored sessions: their full
    // digests and current user state, not unverified token claims, are authoritative.
    let restarted = PostgresIamService::new(pool.clone(), first.tenant.clone())
        .await
        .unwrap();
    assert_eq!(
        restarted.validate_session(&session.id).await.unwrap(),
        Some(session.clone())
    );
    assert!(restarted.revoke_session(&session.id).await.unwrap());
    sessions(&first, &second).await;
    rls_and_pool_context(&first, &pool).await;
    let tenants = vec![first.tenant.id().to_owned(), second.tenant.id().to_owned()];
    for table in [
        "fl_iam_sessions",
        "fl_iam_users",
        "fl_iam_roles",
        "fl_iam_organizations",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE tenant_id=ANY($1)"
        )))
        .bind(&tenants)
        .execute(&owner)
        .await
        .unwrap();
    }
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP OWNED BY {role}")))
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE {role}")))
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

async fn crud_and_roles(first: &PostgresIamService, second: &PostgresIamService) {
    first
        .create_user(
            User::new("user-1", " TEST@example.com ").with_role("member"),
            "password-1".to_owned(),
        )
        .await
        .unwrap();
    second
        .create_user(
            User::new("user-1", "test@example.com")
                .with_display_name("Other tenant")
                .with_role("member"),
            "password-2".to_owned(),
        )
        .await
        .unwrap();
    first
        .create_user(
            User::new("user-2", "second@example.com"),
            "password".to_owned(),
        )
        .await
        .unwrap();
    assert!(
        first
            .create_user(
                User::new("user-3", "test@example.com"),
                "password".to_owned()
            )
            .await
            .is_err()
    );
    assert_eq!(
        first
            .find_user_by_email("TEST@EXAMPLE.COM")
            .await
            .unwrap()
            .unwrap()
            .id,
        "user-1"
    );
    assert_ne!(
        first.get_user("user-1").await.unwrap(),
        second.get_user("user-1").await.unwrap()
    );
    let page = first.list_users(PageRequest::first_page(1)).await.unwrap();
    assert_eq!(page.items.len(), 1);
    let next = first
        .list_users(PageRequest {
            limit: 1,
            cursor: page.next_cursor,
        })
        .await
        .unwrap();
    assert_eq!(next.items[0].id, "user-2");
    assert!(next.next_cursor.is_none());
    assert!(first.list_users(PageRequest::first_page(0)).await.is_err());
    first
        .put_role(Role::new("member").with_permission(Permission::new("users", "read")))
        .await
        .unwrap();
    second
        .put_role(Role::new("member").with_permission(Permission::new("billing", "read")))
        .await
        .unwrap();
    assert_eq!(
        first.permissions_for_user("user-1").await.unwrap(),
        vec![Permission::new("users", "read")]
    );
    assert_eq!(
        second.permissions_for_user("user-1").await.unwrap(),
        vec![Permission::new("billing", "read")]
    );
    assert!(first.roles_for_user("missing").await.unwrap().is_empty());
    organizations(first, second).await;
    let restarted = PostgresIamService::new(first.pool.clone(), first.tenant.clone())
        .await
        .unwrap();
    assert!(restarted.get_user("user-1").await.unwrap().is_some());
}

async fn organizations(first: &PostgresIamService, second: &PostgresIamService) {
    let organization = Organization::new("org-1", "First", "first");
    first
        .create_organization(organization.clone())
        .await
        .unwrap();
    assert!(second.get_organization("org-1").await.unwrap().is_none());
    assert_eq!(
        first
            .list_organizations(PageRequest::first_page(10))
            .await
            .unwrap()
            .items,
        vec![organization.clone()]
    );
    let mut changed = organization;
    changed.name = "Updated".to_owned();
    assert!(first.update_organization(changed.clone()).await.unwrap());
    assert_eq!(
        first.get_organization("org-1").await.unwrap(),
        Some(changed)
    );
    assert!(first.delete_organization("org-1").await.unwrap());
}

async fn sessions(first: &PostgresIamService, second: &PostgresIamService) {
    let credentials = || Credentials {
        email: "test@example.com".to_owned(),
        password: "password-1".to_owned(),
    };
    for input in [
        Credentials {
            email: "unknown@example.com".to_owned(),
            password: "password-1".to_owned(),
        },
        Credentials {
            email: "test@example.com".to_owned(),
            password: "wrong".to_owned(),
        },
    ] {
        assert_eq!(
            first.authenticate(input).await.unwrap_err().code,
            "iam_invalid_credentials"
        );
    }
    let session = first.authenticate(credentials()).await.unwrap();
    let another = first.authenticate(credentials()).await.unwrap();
    assert_ne!(session.id, another.id);
    assert_eq!(
        first.validate_session(&session.id).await.unwrap(),
        Some(session.clone())
    );
    assert!(
        second
            .validate_session(&session.id)
            .await
            .unwrap()
            .is_none()
    );
    let mut tx = first.transaction().await.unwrap();
    let hash: (String,) = sqlx::query_as(
        "SELECT token_hash FROM fl_iam_sessions WHERE tenant_id=$1 AND token_hash=$2",
    )
    .bind(first.tenant.id())
    .bind(token_hash(&session.id))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_ne!(hash.0, session.id);
    assert_eq!(hash.0, token_hash(&session.id));
    tx.commit().await.unwrap();
    assert!(first.revoke_session(&session.id).await.unwrap());
    assert!(first.validate_session(&session.id).await.unwrap().is_none());
    assert!(
        first
            .set_password("user-1", "new-password".to_owned())
            .await
            .unwrap()
    );
    assert!(first.validate_session(&another.id).await.unwrap().is_none());
    assert!(first.authenticate(credentials()).await.is_err());
    assert!(
        first
            .authenticate(Credentials {
                email: "test@example.com".to_owned(),
                password: "new-password".to_owned()
            })
            .await
            .is_ok()
    );
    first
        .set_password("user-1", "password-1".to_owned())
        .await
        .unwrap();
    let another = first.authenticate(credentials()).await.unwrap();
    let mut user = first.get_user("user-1").await.unwrap().unwrap();
    user.disabled = true;
    first.update_user(user.clone()).await.unwrap();
    assert!(first.authenticate(credentials()).await.is_err());
    assert!(first.validate_session(&another.id).await.unwrap().is_none());
    user.disabled = false;
    first.update_user(user).await.unwrap();
    let mut tx = first.transaction().await.unwrap();
    sqlx::query(
        "UPDATE fl_iam_sessions SET expires_at=now()-interval '1 second' WHERE tenant_id=$1",
    )
    .bind(first.tenant.id())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(first.validate_session(&another.id).await.unwrap().is_none());
    assert_eq!(first.purge_expired_sessions().await.unwrap(), 1);
    let session = first.authenticate(credentials()).await.unwrap();
    assert!(first.delete_user("user-1").await.unwrap());
    assert!(first.validate_session(&session.id).await.unwrap().is_none());
    first
        .create_user(
            User::new("user-1", "test@example.com"),
            "password-1".to_owned(),
        )
        .await
        .unwrap();
}

async fn rls_and_pool_context(first: &PostgresIamService, pool: &PgPool) {
    let current: (Option<String>,) =
        sqlx::query_as("SELECT NULLIF(current_setting('forklaunch.tenant_id', true), '')")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(current.0, None);
    let visible: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM fl_iam_users")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(visible.0, 0, "RLS must reject reads without a tenant");
    let mut tx = first.transaction().await.unwrap();
    let tenants: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT tenant_id FROM fl_iam_users")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(tenants, vec![(first.tenant.id().to_owned(),)]);
    let illegal = sqlx::query("INSERT INTO fl_iam_users (tenant_id,id,email,user_json,password_hash) VALUES ('tenant-b','injected','injected@example.com','{}','unused')").execute(&mut *tx).await;
    assert!(illegal.is_err(), "RLS must reject cross-tenant writes");
    tx.rollback().await.unwrap();
    let current: (Option<String>,) =
        sqlx::query_as("SELECT NULLIF(current_setting('forklaunch.tenant_id', true), '')")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        current.0, None,
        "tenant context must not leak through pooled connections"
    );
}
