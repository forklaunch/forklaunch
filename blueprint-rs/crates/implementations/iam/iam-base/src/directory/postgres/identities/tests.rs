mod unit_of_work;
use super::*;
use crate::{directory::service::BaseIamDirectory, postgres::PostgresIamService};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use forklaunch_iam_interface::{
    Credentials, User,
    services::{OrganizationService, PermissionService, RoleService, UserService},
};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::Arc;

struct Fixture {
    owner: PgPool,
    pool: PgPool,
    role: String,
    tenant: String,
    store: Arc<PostgresDirectoryStore>,
    auth: PostgresIamService,
}
impl Fixture {
    async fn new() -> Self {
        let url = std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap();
        let owner = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .unwrap();
        PostgresDirectoryStore::migrate(&owner).await.unwrap();
        let role = format!("fl_identity_{}", uuid::Uuid::new_v4().simple());
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
        sqlx::query(sqlx::AssertSqlSafe(format!("GRANT SELECT,INSERT,UPDATE,DELETE ON fl_iam_users,fl_iam_roles,fl_iam_sessions,fl_iam_provider_identities,fl_iam_directory,fl_iam_directory_unique_claims,fl_iam_directory_audit,fl_iam_directory_identities TO {role}"))).execute(&owner).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "GRANT USAGE ON SEQUENCE fl_iam_directory_sequence_seq TO {role}"
        )))
        .execute(&owner)
        .await
        .unwrap();
        let set_role = format!("SET ROLE {role}");
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(move |connection, _| {
                let query = set_role.clone();
                Box::pin(async move {
                    sqlx::query(sqlx::AssertSqlSafe(query))
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        let tenant = role.clone();
        let store = Arc::new(
            PostgresDirectoryStore::new(
                pool.clone(),
                TenantContext::new(&tenant).unwrap(),
                Arc::new(FieldEncryptor::new("directory-identity-test-key").unwrap()),
                AuditContext::actor("identity-operator", "identity-fixture"),
            )
            .await
            .unwrap(),
        );
        let auth = PostgresIamService::new(pool.clone(), TenantContext::new(&tenant).unwrap())
            .await
            .unwrap();
        Self {
            owner,
            pool,
            role,
            tenant,
            store,
            auth,
        }
    }
    async fn directory(&self, email: &str) -> String {
        BaseIamDirectory::new(self.store.clone()).create_user(serde_json::from_value(json!({"email":email,"firstName":"Owned","lastName":"Fixture","password":"ignored-password","roles":[]})).unwrap()).await.unwrap().id
    }
    async fn cleanup(self) {
        self.pool.close().await;
        for table in ["fl_iam_users", "fl_iam_directory", "fl_iam_directory_audit"] {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DELETE FROM {table} WHERE tenant_id=$1 OR tenant_id=$2"
            )))
            .bind(&self.tenant)
            .bind(format!("{}-other", self.tenant))
            .execute(&self.owner)
            .await
            .unwrap();
        }
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP OWNED BY {}", self.role)))
            .execute(&self.owner)
            .await
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE {}", self.role)))
            .execute(&self.owner)
            .await
            .unwrap();
        self.owner.close().await;
    }
}
fn credentials(email: &str) -> Credentials {
    Credentials {
        email: email.into(),
        password: "owned-fixture-password".into(),
    }
}

#[tokio::test]
#[ignore = "requires FORKLAUNCH_TEST_POSTGRES_URL"]
#[allow(clippy::too_many_lines)]
async fn directory_identity_binding_is_explicit_session_verified_and_atomic() {
    let f = Fixture::new().await;
    let directory = f.directory("same@example.test").await;
    let other_directory = f.directory("other@example.test").await;
    f.auth
        .create_user(
            User::new("auth-one", "same@example.test"),
            "owned-fixture-password".into(),
        )
        .await
        .unwrap();
    f.auth
        .create_user(
            User::new("auth-two", "second@example.test"),
            "owned-fixture-password".into(),
        )
        .await
        .unwrap();
    let session = f
        .auth
        .authenticate(credentials("same@example.test"))
        .await
        .unwrap();
    assert_eq!(
        f.auth
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        None,
        "matching email must not bind"
    );
    let mut tx = f.store.begin(false).await.unwrap();
    assert!(
        tx.get(DirectoryKind::User, &directory)
            .await
            .unwrap()
            .unwrap()
            .get("password")
            .is_none()
    );
    tx.commit().await.unwrap();
    assert!(
        f.store
            .bind_auth_identity(&directory, "missing")
            .await
            .is_err()
    );
    assert!(
        f.store
            .bind_auth_identity("missing", "auth-one")
            .await
            .is_err()
    );
    let other_auth = f
        .auth
        .for_tenant(TenantContext::new(format!("{}-other", f.tenant)).unwrap());
    other_auth
        .create_user(
            User::new("auth-one", "same@example.test"),
            "owned-fixture-password".into(),
        )
        .await
        .unwrap();
    let foreign = other_auth
        .authenticate(credentials("same@example.test"))
        .await
        .unwrap();
    assert_eq!(
        f.auth
            .directory_user_for_session(&foreign.id)
            .await
            .unwrap(),
        None
    );
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_iam_directory_audit FROM {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(
        f.store
            .bind_auth_identity(&directory, "auth-one")
            .await
            .is_err()
    );
    assert_eq!(
        f.auth
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        None
    );
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_iam_directory_audit TO {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    let audit_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_audit WHERE tenant_id=$1")
            .bind(&f.tenant)
            .fetch_one(&f.owner)
            .await
            .unwrap();
    for audit in [
        AuditContext::system("missing-actor"),
        AuditContext::actor("  ", "blank-actor"),
    ] {
        let store = f.store.with_audit_context(audit).unwrap();
        assert!(
            store
                .bind_auth_identity(&directory, "auth-one")
                .await
                .is_err()
        );
        assert!(
            store
                .unbind_auth_identity(&directory, "auth-one")
                .await
                .is_err()
        );
        assert!(
            store
                .delete_linked_identity(&directory, "auth-one")
                .await
                .is_err()
        );
    }
    let audit_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_audit WHERE tenant_id=$1")
            .bind(&f.tenant)
            .fetch_one(&f.owner)
            .await
            .unwrap();
    assert_eq!(audit_before, audit_after);
    assert!(f.auth.get_user("auth-one").await.unwrap().is_some());
    assert_eq!(
        f.auth
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        None
    );
    let (left, right) = tokio::join!(
        f.store.bind_auth_identity(&directory, "auth-one"),
        f.store.bind_auth_identity(&directory, "auth-two")
    );
    assert_ne!(left.is_ok(), right.is_ok());
    let winner = if left.is_ok() { "auth-one" } else { "auth-two" };
    assert!(
        f.store
            .unbind_auth_identity(&directory, winner)
            .await
            .unwrap()
    );
    f.store
        .bind_auth_identity(&directory, "auth-one")
        .await
        .unwrap();
    f.store
        .bind_auth_identity(&directory, "auth-one")
        .await
        .unwrap();
    assert!(
        f.store
            .bind_auth_identity(&other_directory, "auth-one")
            .await
            .is_err()
    );
    assert!(
        f.store
            .bind_auth_identity(&directory, "auth-two")
            .await
            .is_err()
    );
    assert_eq!(
        f.auth
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        Some(directory.clone())
    );
    let audit_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_audit WHERE tenant_id=$1")
            .bind(&f.tenant)
            .fetch_one(&f.owner)
            .await
            .unwrap();
    for audit in [
        AuditContext::system("missing-actor"),
        AuditContext::actor("  ", "blank-actor"),
    ] {
        let store = f.store.with_audit_context(audit).unwrap();
        assert!(
            store
                .unbind_auth_identity(&directory, "auth-one")
                .await
                .is_err()
        );
        assert!(
            store
                .delete_linked_identity(&directory, "auth-one")
                .await
                .is_err()
        );
    }
    let audit_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_audit WHERE tenant_id=$1")
            .bind(&f.tenant)
            .fetch_one(&f.owner)
            .await
            .unwrap();
    assert_eq!(audit_before, audit_after);
    assert!(
        f.auth
            .validate_session(&session.id)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        f.auth
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        Some(directory.clone())
    );
    assert_eq!(
        f.auth.directory_user_for_session("auth-one").await.unwrap(),
        None
    );
    assert_eq!(
        other_auth
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        None
    );
    let foreign_tenant = TenantContext::new(format!("{}-other", f.tenant)).unwrap();
    let mut foreign_tx =
        forklaunch_core::persistence::begin_tenant_transaction(&f.pool, &foreign_tenant)
            .await
            .unwrap();
    let hidden: i64 = sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_identities")
        .fetch_one(&mut *foreign_tx)
        .await
        .unwrap();
    assert_eq!(hidden, 0);
    let denied = sqlx::query("INSERT INTO fl_iam_directory_identities(tenant_id,directory_user_id,auth_user_id) VALUES($1,$2,$3)")
        .bind(&f.tenant).bind(&directory).bind("auth-one").execute(&mut *foreign_tx).await.unwrap_err();
    assert_eq!(
        denied.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );
    foreign_tx.rollback().await.unwrap();
    let restarted = PostgresIamService::new(f.pool.clone(), TenantContext::new(&f.tenant).unwrap())
        .await
        .unwrap();
    assert_eq!(
        restarted
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        Some(directory.clone())
    );
    #[cfg(feature = "jwt-sessions")]
    {
        let issuer = crate::jwt_sessions::RsaJwtSessionIssuer::new(
            include_bytes!("../../../jwt_sessions/test-key.pem"),
            "directory-key",
            "https://issuer.fixture.test",
            "directory-fixture",
        )
        .unwrap();
        let jwt_auth = f.auth.clone().with_jwt_session_issuer(Arc::new(issuer));
        let jwt_session = jwt_auth
            .authenticate(credentials("same@example.test"))
            .await
            .unwrap();
        assert_eq!(jwt_session.id.split('.').count(), 3);
        assert_eq!(
            jwt_auth
                .directory_user_for_session(&jwt_session.id)
                .await
                .unwrap(),
            Some(directory.clone())
        );
        assert_eq!(
            jwt_auth
                .directory_user_for_session(&format!("{}x", jwt_session.id))
                .await
                .unwrap(),
            None
        );
    }
    let directory_service = BaseIamDirectory::new(f.store.clone());
    let permission = directory_service
        .create_permission(serde_json::from_value(json!({"slug":"literal:permission"})).unwrap())
        .await
        .unwrap();
    let role = directory_service
        .create_role(
            serde_json::from_value(json!({"name":"literal-role","permissionIds":[permission.id]}))
                .unwrap(),
        )
        .await
        .unwrap();
    let organization = directory_service
        .create_organization(
            serde_json::from_value(json!({"name":"Authorization organization","domain":"authorization.example","subscription":"fixture"})).unwrap(),
        )
        .await
        .unwrap();
    directory_service
        .update_user(
            serde_json::from_value(
                json!({"id":directory,"roles":[role.id],"organization":organization.id}),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let mut tx = f.store.begin(true).await.unwrap();
    let mut row = tx
        .get(DirectoryKind::User, &directory)
        .await
        .unwrap()
        .unwrap();
    row["organization"] = json!(organization.id);
    tx.replace(DirectoryKind::User, row).await.unwrap();
    tx.commit().await.unwrap();
    let snapshot = f
        .store
        .authorization_for_session(&session.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.auth_user_id, "auth-one");
    assert_eq!(snapshot.directory_user_id, directory);
    assert_eq!(snapshot.roles, ["literal-role"]);
    assert_eq!(snapshot.permissions, ["literal:permission"]);
    assert_eq!(snapshot.organization_id, Some(organization.id.clone()));
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_audit WHERE tenant_id=$1 AND operation='read' AND actor_id=$2 AND correlation_id='identity-fixture'")
        .bind(&f.tenant).bind(&directory).fetch_one(&f.owner).await.unwrap();
    assert!(
        audited >= 3,
        "verified snapshot reads retain caller correlation and bound actor"
    );

    directory_service
        .update_role(serde_json::from_value(json!({"id":role.id,"name":"current-role"})).unwrap())
        .await
        .unwrap();
    directory_service
        .delete_permission(&permission.id)
        .await
        .unwrap();
    let current = f
        .store
        .authorization_for_session(&session.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.roles, ["current-role"]);
    assert!(current.permissions.is_empty());
    assert_eq!(snapshot.roles, ["literal-role"]);
    assert_eq!(snapshot.permissions, ["literal:permission"]);
    assert!(
        f.store
            .authorization_for_session("auth-one")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        f.store
            .authorization_for_session(&foreign.id)
            .await
            .unwrap()
            .is_none()
    );
    let mut user = f.auth.get_user("auth-one").await.unwrap().unwrap();
    user.disabled = true;
    f.auth.update_user(user.clone()).await.unwrap();
    assert!(
        f.store
            .authorization_for_session(&session.id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        f.auth
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        None
    );
    user.disabled = false;
    f.auth.update_user(user).await.unwrap();
    sqlx::query(
        "UPDATE fl_iam_sessions SET expires_at=now()-interval '1 second' WHERE tenant_id=$1",
    )
    .bind(&f.tenant)
    .execute(&f.owner)
    .await
    .unwrap();
    assert_eq!(
        f.auth
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        None
    );
    let session = f
        .auth
        .authenticate(credentials("same@example.test"))
        .await
        .unwrap();
    assert!(
        !f.store
            .unbind_auth_identity(&directory, "auth-two")
            .await
            .unwrap()
    );
    assert!(
        f.store
            .unbind_auth_identity(&directory, "auth-one")
            .await
            .unwrap()
    );
    assert!(
        f.auth
            .validate_session(&session.id)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        f.auth
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        None
    );
    f.store
        .bind_auth_identity(&directory, "auth-one")
        .await
        .unwrap();
    f.auth.revoke_session(&session.id).await.unwrap();
    assert!(
        f.store
            .authorization_for_session(&session.id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        f.auth
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        None
    );
    let session = f
        .auth
        .authenticate(credentials("same@example.test"))
        .await
        .unwrap();
    let service = BaseIamDirectory::new(f.store.clone());
    assert!(service.delete_user(&directory, None).await.is_err());
    assert!(service.get_user(&directory).await.is_ok());
    assert!(
        f.store
            .delete_linked_identity(&directory, "auth-two")
            .await
            .is_err()
    );
    f.auth
        .bind_provider_identity("https://provider.fixture.test", "owned-subject", "auth-one")
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_iam_directory_audit FROM {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(
        f.store
            .unbind_auth_identity(&directory, "auth-one")
            .await
            .is_err()
    );
    assert_eq!(
        f.auth
            .directory_user_for_session(&session.id)
            .await
            .unwrap(),
        Some(directory.clone())
    );
    assert!(
        f.store
            .delete_linked_identity(&directory, "auth-one")
            .await
            .is_err()
    );
    assert!(
        f.auth
            .validate_session(&session.id)
            .await
            .unwrap()
            .is_some()
    );
    let directory_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fl_iam_directory WHERE tenant_id=$1 AND kind='user' AND id=$2",
    )
    .bind(&f.tenant)
    .bind(&directory)
    .fetch_one(&f.owner)
    .await
    .unwrap();
    assert_eq!(directory_count, 1);
    let provider_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_provider_identities WHERE tenant_id=$1")
            .bind(&f.tenant)
            .fetch_one(&f.owner)
            .await
            .unwrap();
    assert_eq!(provider_count, 1);
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_iam_directory_audit TO {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    let (login, deleted) = tokio::join!(
        f.auth.authenticate(credentials("same@example.test")),
        f.store.delete_linked_identity(&directory, "auth-one")
    );
    deleted.unwrap();
    if let Ok(login) = login {
        assert!(f.auth.validate_session(&login.id).await.unwrap().is_none());
    }
    assert!(
        f.auth
            .validate_session(&session.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(f.auth.get_user("auth-one").await.unwrap().is_none());
    assert!(service.get_user(&directory).await.is_err());
    assert!(
        f.auth
            .authenticate(credentials("same@example.test"))
            .await
            .is_err()
    );
    let provider_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_provider_identities WHERE tenant_id=$1")
            .bind(&f.tenant)
            .fetch_one(&f.owner)
            .await
            .unwrap();
    assert_eq!(provider_count, 0);
    let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_audit WHERE tenant_id=$1 AND record_id=$2 AND actor_id='identity-operator' AND operation='delete_auth_identity'").bind(&f.tenant).bind(&directory).fetch_one(&f.owner).await.unwrap();
    assert_eq!(audit_count, 1);
    f.store
        .bind_auth_identity(&other_directory, "auth-two")
        .await
        .unwrap();
    f.auth.delete_user("auth-two").await.unwrap();
    service.delete_user(&other_directory, None).await.unwrap();
    let race_directory = f.directory("race@example.test").await;
    f.auth
        .create_user(
            User::new("auth-race", "race@example.test"),
            "owned-fixture-password".into(),
        )
        .await
        .unwrap();
    let (bound, deleted) = tokio::join!(
        f.store.bind_auth_identity(&race_directory, "auth-race"),
        service.delete_user(&race_directory, None)
    );
    assert_ne!(
        bound.is_ok(),
        deleted.is_ok(),
        "bind and ordinary delete must not both succeed"
    );
    if bound.is_ok() {
        f.store
            .delete_linked_identity(&race_directory, "auth-race")
            .await
            .unwrap();
    } else {
        f.auth.delete_user("auth-race").await.unwrap();
    }
    f.cleanup().await;
}
