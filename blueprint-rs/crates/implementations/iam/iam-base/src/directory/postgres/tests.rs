use super::*;
use crate::directory::service::BaseIamDirectory;
use forklaunch_iam_interface::services::{
    OrganizationService, PermissionService, RoleService, UserService,
};
use serde::de::DeserializeOwned;
use sqlx::postgres::PgPoolOptions;
fn dto<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

#[tokio::test]
#[ignore = "requires FORKLAUNCH_TEST_POSTGRES_URL"]
#[allow(clippy::too_many_lines)]
async fn encrypted_directory_transactions_tenant_rls_rotation_and_relationships() {
    let url = std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap();
    let owner = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .unwrap();
    PostgresDirectoryStore::migrate(&owner).await.unwrap();
    PostgresDirectoryStore::migrate(&owner).await.unwrap();
    let role = format!("fl_directory_{}", uuid::Uuid::new_v4().simple());
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
    sqlx::query(sqlx::AssertSqlSafe(format!("GRANT SELECT,INSERT,UPDATE,DELETE ON fl_iam_directory,fl_iam_directory_unique_claims,fl_iam_directory_audit TO {role}"))).execute(&owner).await.unwrap();
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
    let tenant = TenantContext::new(format!("{role}-a")).unwrap();
    let other = TenantContext::new(format!("{role}-b")).unwrap();
    let key = Arc::new(FieldEncryptor::new("directory-fixture-old-key").unwrap());
    assert!(
        PostgresDirectoryStore::new(
            owner.clone(),
            tenant.clone(),
            key.clone(),
            AuditContext::system("owner-rejected")
        )
        .await
        .is_err()
    );
    let store = PostgresDirectoryStore::new(
        pool.clone(),
        tenant.clone(),
        key.clone(),
        AuditContext::actor("operator", "directory-fixture"),
    )
    .await
    .unwrap();
    assert!(store.with_audit_context(AuditContext::system("")).is_err());
    let second = PostgresDirectoryStore::new(
        pool.clone(),
        other.clone(),
        key.clone(),
        AuditContext::actor("other-operator", "other-fixture"),
    )
    .await
    .unwrap();
    crate::directory::compliance::tests::exercise(
        Arc::new(store.clone()),
        Arc::new(second.clone()),
    )
    .await;
    let service = BaseIamDirectory::new(Arc::new(store.clone()));
    let other_service = BaseIamDirectory::new(Arc::new(second.clone()));
    let permission = service
        .create_permission(dto(json!({"slug":"platform.read"})))
        .await
        .unwrap();
    let role_row = service
        .create_role(dto(
            json!({"name":"Readers","permissionIds":[permission.id]}),
        ))
        .await
        .unwrap();
    let organization=service.create_organization(dto(json!({"name":"Private Organization","domain":"private.fixture.test","subscription":"private-subscription","providerFields":{"private":true}}))).await.unwrap();
    let create = json!({"email":"private@fixture.test","password":"never-stored","firstName":"Private","lastName":"Member","organization":organization.id,"roles":[role_row.id],"providerFields":{"secret":"provider-private"}});
    let user = service.create_user(dto(create.clone())).await.unwrap();
    assert_eq!(
        service.get_user(&user.id).await.unwrap().roles[0].permissions[0].id,
        permission.id
    );
    assert_eq!(
        service
            .get_organization(&organization.id)
            .await
            .unwrap()
            .users[0]
            .id,
        user.id
    );
    assert!(other_service.get_user(&user.id).await.is_err());
    other_service
        .create_user(dto(create.clone()))
        .await
        .unwrap();
    let mut tx = second.begin(false).await.unwrap();
    assert!(
        tx.get(DirectoryKind::Organization, &organization.id)
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();
    let raw: Value = sqlx::query_scalar(
        "SELECT data FROM fl_iam_directory WHERE tenant_id=$1 AND kind='user' AND id=$2",
    )
    .bind(tenant.id())
    .bind(&user.id)
    .fetch_one(&owner)
    .await
    .unwrap();
    for secret in [
        "private@fixture.test",
        "Private",
        "Member",
        "provider-private",
        "never-stored",
    ] {
        assert!(!raw.to_string().contains(secret));
    }
    assert!(raw.get("password").is_none());
    let before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_audit WHERE tenant_id=$1")
            .bind(tenant.id())
            .fetch_one(&owner)
            .await
            .unwrap();
    let first_new = json!({"email":"batch@fixture.test","password":"unused","firstName":"Batch","lastName":"Member","roles":[]});
    assert!(
        service
            .create_batch_users(vec![dto(first_new.clone()), dto(create.clone())])
            .await
            .is_err()
    );
    let after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_audit WHERE tenant_id=$1")
            .bind(tenant.id())
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(
        before, after,
        "failed batch must roll back audit writes too"
    );
    service.create_user(dto(first_new)).await.unwrap();
    let rotated_key = Arc::new(
        FieldEncryptor::new("directory-fixture-new-key")
            .unwrap()
            .with_previous_keys(&["directory-fixture-old-key".into()])
            .unwrap(),
    );
    let rotated = PostgresDirectoryStore::new(
        pool.clone(),
        tenant.clone(),
        rotated_key,
        AuditContext::actor("operator", "rotation-fixture"),
    )
    .await
    .unwrap();
    let rotated_service = BaseIamDirectory::new(Arc::new(rotated.clone()));
    assert_eq!(
        rotated_service.get_user(&user.id).await.unwrap().email,
        "private@fixture.test"
    );
    assert!(
        rotated_service.create_user(dto(create)).await.is_err(),
        "old blind indexes must still enforce uniqueness"
    );
    rotated_service
        .update_user(dto(json!({"id":user.id,"firstName":"Rotated"})))
        .await
        .unwrap();
    let mut read = store.begin(false).await.unwrap();
    assert!(
        read.insert(
            DirectoryKind::Permission,
            json!({"id":"denied","slug":"denied"})
        )
        .await
        .is_err()
    );
    read.commit().await.unwrap();
    {
        let mut dropped = rotated.begin(true).await.unwrap();
        dropped
            .insert(
                DirectoryKind::Permission,
                json!({"id":"dropped","slug":"never committed"}),
            )
            .await
            .unwrap();
    }
    let mut tx = rotated.begin(false).await.unwrap();
    assert!(
        tx.get(DirectoryKind::Permission, "dropped")
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();
    let (a,b)=tokio::join!(rotated_service.create_user(dto(json!({"email":"race@fixture.test","password":"unused","firstName":"Race","lastName":"One","roles":[]}))),rotated_service.create_user(dto(json!({"email":"race@fixture.test","password":"unused","firstName":"Race","lastName":"Two","roles":[]}))));
    assert_ne!(a.is_ok(), b.is_ok());
    rotated_service
        .delete_permission(&permission.id)
        .await
        .unwrap();
    assert!(
        rotated_service
            .get_role(&role_row.id)
            .await
            .unwrap()
            .permissions
            .is_empty()
    );
    rotated_service.delete_role(&role_row.id).await.unwrap();
    assert!(
        rotated_service
            .get_user(&user.id)
            .await
            .unwrap()
            .roles
            .is_empty()
    );
    rotated_service
        .delete_organization(&organization.id)
        .await
        .unwrap();
    assert!(
        rotated_service
            .get_user(&user.id)
            .await
            .unwrap()
            .organization
            .is_none()
    );
    let unscoped: i64 = sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        unscoped, 0,
        "transaction-local tenant must not leak into the pool"
    );
    // Swap a valid ciphertext between fields: authenticated envelopes reject it.
    sqlx::query("UPDATE fl_iam_directory SET data=jsonb_set(data,'{lastName}',data->'firstName') WHERE tenant_id=$1 AND kind='user' AND id=$2").bind(tenant.id()).bind(&user.id).execute(&owner).await.unwrap();
    assert!(rotated_service.get_user(&user.id).await.is_err());
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fl_iam_directory_audit WHERE tenant_id=$1 AND actor_id='operator'",
    )
    .bind(tenant.id())
    .fetch_one(&owner)
    .await
    .unwrap();
    assert!(audit > 0);
    for table in ["fl_iam_directory", "fl_iam_directory_audit"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE tenant_id=$1 OR tenant_id=$2"
        )))
        .bind(tenant.id())
        .bind(other.id())
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
}
