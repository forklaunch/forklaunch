use super::*;
use forklaunch_core::persistence::TenantContext;

#[tokio::test]
#[ignore = "requires FORKLAUNCH_TEST_POSTGRES_URL"]
async fn postgres_provider_bindings_are_explicit_tenant_bound_and_revocable() {
    let (owner, pool, role) = super::super::tests::test_pools().await;
    let first = PostgresIamService::new(
        pool.clone(),
        TenantContext::new(format!("provider-a-{}", uuid::Uuid::new_v4())).unwrap(),
    )
    .await
    .unwrap();
    let second = first
        .for_tenant(TenantContext::new(format!("provider-b-{}", uuid::Uuid::new_v4())).unwrap());
    for store in [&first, &second] {
        store
            .create_user(User::new("one", "same@example.com"), "password".into())
            .await
            .unwrap();
        store
            .create_user(User::new("two", "second@example.com"), "password".into())
            .await
            .unwrap();
    }
    check_bindings(&first, &second).await;
    let issuer = "https://id.example.com";
    let session = first
        .session_for_provider_identity(issuer, "subject")
        .await
        .unwrap();
    assert_eq!(session.user_id, "one");
    assert!(
        second
            .validate_session(&session.id)
            .await
            .unwrap()
            .is_none()
    );
    let restarted = PostgresIamService::new(pool.clone(), first.tenant.clone())
        .await
        .unwrap();
    assert!(
        restarted
            .validate_session(&session.id)
            .await
            .unwrap()
            .is_some()
    );
    restarted.revoke_session(&session.id).await.unwrap();
    assert!(first.validate_session(&session.id).await.unwrap().is_none());
    let mut user = first.get_user("one").await.unwrap().unwrap();
    user.disabled = true;
    first.update_user(user).await.unwrap();
    assert!(
        first
            .session_for_provider_identity(issuer, "subject")
            .await
            .is_err()
    );
    assert!(
        !first
            .unbind_provider_identity(issuer, "subject", "two")
            .await
            .unwrap()
    );
    assert!(
        first
            .unbind_provider_identity(issuer, "subject", "one")
            .await
            .unwrap()
    );
    assert!(
        first
            .session_for_provider_identity(issuer, "subject")
            .await
            .is_err()
    );
    assert_eq!(
        second
            .session_for_provider_identity(issuer, "subject")
            .await
            .unwrap()
            .user_id,
        "two"
    );
    check_rls_and_delete(&first, &second, issuer).await;
    for store in [&first, &second] {
        store.delete_user("one").await.unwrap();
        store.delete_user("two").await.unwrap();
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

#[test]
fn provider_identity_input_is_bounded_and_exact() {
    assert!(validate_identity("", "subject").is_err());
    assert!(validate_identity("issuer", "\nsubject").is_err());
    assert!(validate_identity("issuer", &"x".repeat(1025)).is_err());
    assert!(validate_identity("https://issuer.example/path", "CaseSensitive subject").is_ok());
}

#[cfg(all(feature = "oidc-sessions", feature = "jwt-sessions"))]
mod oidc;

async fn check_bindings(first: &PostgresIamService, second: &PostgresIamService) {
    let issuer = "https://id.example.com";
    assert!(
        first
            .session_for_provider_identity(issuer, "same@example.com")
            .await
            .is_err()
    );
    assert!(
        first
            .bind_provider_identity(issuer, "subject", "missing")
            .await
            .is_err()
    );
    first
        .bind_provider_identity(issuer, "subject", "one")
        .await
        .unwrap();
    first
        .bind_provider_identity(issuer, "subject", "one")
        .await
        .unwrap();
    assert!(
        first
            .bind_provider_identity(issuer, "subject", "two")
            .await
            .is_err()
    );
    assert!(
        second
            .session_for_provider_identity(issuer, "subject")
            .await
            .is_err()
    );
    assert!(
        first
            .session_for_provider_identity("https://other.example.com", "subject")
            .await
            .is_err()
    );
    assert!(
        first
            .session_for_provider_identity(issuer, "Subject")
            .await
            .is_err()
    );
    second
        .bind_provider_identity(issuer, "subject", "two")
        .await
        .unwrap();
}

async fn check_rls_and_delete(
    first: &PostgresIamService,
    second: &PostgresIamService,
    issuer: &str,
) {
    let mut tx = first.transaction().await.unwrap();
    let visible: (i64,) = sqlx::query_as("SELECT count(*) FROM fl_iam_provider_identities")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(visible.0, 0);
    let denied =
        sqlx::query("INSERT INTO fl_iam_provider_identities VALUES ($1,$2,'injected','one')")
            .bind(second.tenant.id())
            .bind(issuer)
            .execute(&mut *tx)
            .await;
    assert!(denied.is_err());
    tx.rollback().await.unwrap();
    second.delete_user("two").await.unwrap();
    assert!(
        second
            .session_for_provider_identity(issuer, "subject")
            .await
            .is_err()
    );
}
