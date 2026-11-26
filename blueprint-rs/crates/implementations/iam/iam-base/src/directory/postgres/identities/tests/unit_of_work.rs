use super::*;
use crate::directory::filter::{DirectoryFilter, Predicate, UserField};
use crate::directory::unit_of_work::{
    Organizations, Users,
    tests::{Context, Mapper, mapped_graph, organization},
};

#[tokio::test]
#[allow(clippy::too_many_lines)]
#[ignore = "requires owned PostgreSQL with role-creation privileges"]
async fn directory_unit_of_work_restricted_postgres_graph_audit_rollback() {
    let f = Fixture::new().await;
    let service = BaseIamDirectory::new(f.store.clone());
    let user = mapped_graph(&service).await;
    let mut read = f.store.begin(false).await.unwrap();
    let row = read.get(DirectoryKind::User, &user).await.unwrap().unwrap();
    assert!(row.get("password").is_none());
    assert_eq!(row["providerFields"]["applicationLabel"], "custom");
    read.commit().await.unwrap();
    let audits:i64=sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_audit WHERE tenant_id=$1 AND actor_id='identity-operator' AND correlation_id='identity-fixture'")
        .bind(&f.tenant).fetch_one(&f.owner).await.unwrap();
    assert!(audits >= 8);
    let credentials: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_users WHERE tenant_id=$1")
            .bind(&f.tenant)
            .fetch_one(&f.owner)
            .await
            .unwrap();
    let bindings: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_identities WHERE tenant_id=$1")
            .bind(&f.tenant)
            .fetch_one(&f.owner)
            .await
            .unwrap();
    assert_eq!((credentials, bindings), (0, 0));
    // Encrypted candidate fields are decrypted only inside the tenant-bound
    // transaction; equality remains case-sensitive and successful reads audit.
    let mut work = service.unit_of_work().await.unwrap();
    let matched = work
        .list::<Users>(
            None,
            &DirectoryFilter::Compare {
                field: UserField::Email,
                test: Predicate::Eq("custom@example.test".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].id, user);
    assert_eq!(
        work.list::<Users>(None, &DirectoryFilter::All(vec![]))
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        work.list::<Users>(None, &DirectoryFilter::Any(vec![]))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        work.list::<Users>(
            None,
            &DirectoryFilter::Any(vec![
                DirectoryFilter::All(vec![]),
                DirectoryFilter::Compare {
                    field: UserField::Email,
                    test: Predicate::Eq("missing@example.test".into())
                },
            ])
        )
        .await
        .unwrap()
        .is_empty()
    );
    work.commit().await.unwrap();
    let prior_filter = counts(&f).await;
    let mut work = service.unit_of_work().await.unwrap();
    assert!(
        work.list::<Users>(
            None,
            &DirectoryFilter::Compare {
                field: UserField::Email,
                test: Predicate::Eq("CUSTOM@example.test".into())
            }
        )
        .await
        .unwrap()
        .is_empty()
    );
    work.commit().await.unwrap();
    let after_filter = counts(&f).await;
    assert_eq!(after_filter.0, prior_filter.0);
    assert!(
        after_filter.1 > prior_filter.1,
        "even filtered-out candidates retain read audits"
    );
    let ciphertext: String = sqlx::query_scalar(
        "SELECT data->>'email' FROM fl_iam_directory WHERE tenant_id=$1 AND kind='user' AND id=$2",
    )
    .bind(&f.tenant)
    .bind(&user)
    .fetch_one(&f.owner)
    .await
    .unwrap();
    assert_ne!(ciphertext, "custom@example.test");
    let before = counts(&f).await;
    let mut work = service.unit_of_work().await.unwrap();
    let org = work
        .create::<Organizations>(organization("rollback-org"))
        .await
        .unwrap();
    let context = Context {
        organization: org.id,
        prefix: "rollback".into(),
        reject: true,
    };
    assert!(
        work.mapped_create::<Users, _>(&Mapper, "rollback@example.test".into(), &context)
            .await
            .is_err()
    );
    assert!(work.commit().await.is_err());
    assert_eq!(
        counts(&f).await,
        before,
        "failed output projection rolls back all four families and audits"
    );
    let mut work = service.unit_of_work().await.unwrap();
    work.create::<Organizations>(organization("drop-org"))
        .await
        .unwrap();
    drop(work);
    assert_eq!(counts(&f).await, before, "drop without commit rolls back");
    let mut work = service.unit_of_work().await.unwrap();
    work.create::<Organizations>(organization("caught-org"))
        .await
        .unwrap();
    assert!(work.get::<Users>("missing").await.is_err());
    assert!(work.commit().await.is_err());
    assert_eq!(
        counts(&f).await,
        before,
        "caught read error cannot commit prior mutations"
    );
    // Another trusted tenant cannot see or change the committed directory user.
    let other = PostgresDirectoryStore::new(
        f.pool.clone(),
        TenantContext::new(format!("{}-other", f.tenant)).unwrap(),
        Arc::new(FieldEncryptor::new("directory-identity-test-key").unwrap()),
        AuditContext::actor("foreign-operator", "foreign-correlation"),
    )
    .await
    .unwrap();
    let foreign = BaseIamDirectory::new(Arc::new(other));
    let mut work = foreign.unit_of_work().await.unwrap();
    assert!(work.get::<Users>(&user).await.is_err());
    assert!(work.commit().await.is_err());
    assert_eq!(counts(&f).await, before);
    // A failed mapper cannot bypass explicit identity deletion restrictions.
    f.auth
        .create_user(
            User::new("auth-linked", "custom@example.test"),
            "credential-password".into(),
        )
        .await
        .unwrap();
    f.store
        .bind_auth_identity(&user, "auth-linked")
        .await
        .unwrap();
    let bound = counts(&f).await;
    let mut work = service.unit_of_work().await.unwrap();
    work.create::<Organizations>(organization("bound-delete"))
        .await
        .unwrap();
    assert!(work.delete::<Users>(&user).await.is_err());
    assert!(work.commit().await.is_err());
    assert_eq!(counts(&f).await, bound);
    drop(service);
    f.cleanup().await;
}
async fn counts(f: &Fixture) -> (i64, i64) {
    let rows = sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory WHERE tenant_id=$1")
        .bind(&f.tenant)
        .fetch_one(&f.owner)
        .await
        .unwrap();
    let audits =
        sqlx::query_scalar("SELECT count(*) FROM fl_iam_directory_audit WHERE tenant_id=$1")
            .bind(&f.tenant)
            .fetch_one(&f.owner)
            .await
            .unwrap();
    (rows, audits)
}
