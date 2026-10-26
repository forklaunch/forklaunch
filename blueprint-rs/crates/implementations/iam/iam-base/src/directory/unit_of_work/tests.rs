use super::*;
use crate::directory::{DirectoryStore, memory::MemoryDirectoryStore, service::BaseIamDirectory};
use serde_json::json;
use std::sync::Arc;

pub(crate) struct Mapper;
pub(crate) struct Context {
    pub organization: String,
    pub prefix: String,
    pub reject: bool,
}
#[derive(Debug, PartialEq)]
pub(crate) struct Profile {
    pub id: String,
    pub display: String,
    pub grant: String,
}
impl DirectoryMapper<Users> for Mapper {
    type Create = String;
    type Update = (String, String);
    type Output = Profile;
    type Context = Context;
    async fn create<T: DirectoryTransaction>(
        &self,
        email: String,
        work: &mut DirectoryUnitOfWork<T>,
        context: &Context,
    ) -> BlueprintResult<CreateUserDto> {
        let organization = work.get::<Organizations>(&context.organization).await?;
        let permission = work
            .create::<Permissions>(
                serde_json::from_value(json!({"slug":format!("{}:read",context.prefix)})).unwrap(),
            )
            .await?;
        let role = work
            .create::<Roles>(
                serde_json::from_value(json!({"name":"Custom","permissionIds":[permission.id]}))
                    .unwrap(),
            )
            .await?;
        Ok(serde_json::from_value(json!({"email":email,"password":"NEVER-STORED","firstName":"Custom","lastName":"Member","organization":organization.id,"roles":[role.id],"providerFields":{"applicationLabel":context.prefix}})).unwrap())
    }
    async fn update<T: DirectoryTransaction>(
        &self,
        input: (String, String),
        work: &mut DirectoryUnitOfWork<T>,
        _: &Context,
    ) -> BlueprintResult<UpdateUserDto> {
        let row = work.get::<Users>(&input.0).await?;
        Ok(serde_json::from_value(
            json!({"id":row.id,"firstName":input.1,"password":"ALSO-IGNORED"}),
        )
        .unwrap())
    }
    async fn project(&self, entity: UserDto, context: &Context) -> BlueprintResult<Profile> {
        assert_ne!(context.prefix, "panic", "owned mapper panic");
        if context.reject {
            return Err(invalid());
        }
        Ok(Profile {
            id: entity.id,
            display: format!("{}:{}", context.prefix, entity.first_name),
            grant: entity.roles[0].permissions[0].slug.clone(),
        })
    }
}
pub(crate) fn organization(label: &str) -> CreateOrganizationDto {
    serde_json::from_value(json!({"name":label,"domain":"fixture.test","subscription":label}))
        .unwrap()
}

/// Reused against the production `PostgreSQL` store with its restricted role.
pub(crate) async fn mapped_graph<S: DirectoryStore + 'static>(
    service: &BaseIamDirectory<S>,
) -> String {
    let mut work = service.unit_of_work().await.unwrap();
    let org = work
        .create::<Organizations>(organization("mapped-org"))
        .await
        .unwrap();
    let context = Context {
        organization: org.id,
        prefix: "custom".into(),
        reject: false,
    };
    let profile = work
        .mapped_create::<Users, _>(&Mapper, "custom@example.test".into(), &context)
        .await
        .unwrap();
    assert_eq!(profile.display, "custom:Custom");
    assert_eq!(profile.grant, "custom:read");
    let updated = work
        .mapped_update::<Users, _>(&Mapper, (profile.id.clone(), "Updated".into()), &context)
        .await
        .unwrap();
    assert_eq!(updated.display, "custom:Updated");
    let projected = work
        .mapped_get::<Users, _>(&Mapper, &profile.id, &context)
        .await
        .unwrap();
    assert_eq!(updated, projected);
    work.commit().await.unwrap();
    profile.id
}

#[tokio::test]
async fn mapper_composes_typed_graph_and_custom_context() {
    let store = Arc::new(MemoryDirectoryStore::default());
    let service = BaseIamDirectory::new(store.clone());
    let user = mapped_graph(&service).await;
    let mut tx = store.begin(false).await.unwrap();
    let row = tx.get(DirectoryKind::User, &user).await.unwrap().unwrap();
    assert!(row.get("password").is_none());
    assert_eq!(row["providerFields"]["applicationLabel"], "custom");
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn failed_projection_and_caught_core_error_poison_all_staged_writes() {
    let store = Arc::new(MemoryDirectoryStore::default());
    let service = BaseIamDirectory::new(store.clone());
    let mut work = service.unit_of_work().await.unwrap();
    let org = work
        .create::<Organizations>(organization("rollback-org"))
        .await
        .unwrap();
    let context = Context {
        organization: org.id.clone(),
        prefix: "rollback".into(),
        reject: true,
    };
    assert!(
        work.mapped_create::<Users, _>(&Mapper, "rollback@example.test".into(), &context)
            .await
            .is_err()
    );
    assert!(work.commit().await.is_err());
    let mut tx = store.begin(false).await.unwrap();
    for kind in [
        DirectoryKind::User,
        DirectoryKind::Role,
        DirectoryKind::Permission,
        DirectoryKind::Organization,
    ] {
        assert!(tx.list(kind, None, None).await.unwrap().is_empty());
    }
    tx.commit().await.unwrap();
    let mut work = service.unit_of_work().await.unwrap();
    work.create::<Organizations>(organization("caught-error"))
        .await
        .unwrap();
    assert!(work.get::<Users>("missing").await.is_err());
    assert!(
        work.create::<Organizations>(organization("must-not-run"))
            .await
            .is_err()
    );
    assert!(work.commit().await.is_err());
}

#[tokio::test]
async fn scope_cannot_be_changed_by_custom_mapper_or_core_crud() {
    let store = Arc::new(MemoryDirectoryStore::default());
    let service = BaseIamDirectory::new(store.clone());
    let user = mapped_graph(&service).await;
    let scoped =
        BaseIamDirectory::new(store.clone()).with_organization_scope("different-org".into());
    let mut work = scoped.unit_of_work().await.unwrap();
    assert!(work.get::<Users>(&user).await.is_err());
    assert!(work.commit().await.is_err());
    let mut work = scoped.unit_of_work().await.unwrap();
    assert!(work.delete::<Users>(&user).await.is_err());
    assert!(work.commit().await.is_err());
    let mut work = scoped.unit_of_work().await.unwrap();
    assert!(
        work.update::<Users>(
            serde_json::from_value(json!({"id":user,"firstName":"Denied"})).unwrap()
        )
        .await
        .is_err()
    );
    assert!(work.commit().await.is_err());
    let mut work = scoped.unit_of_work().await.unwrap();
    let dto=serde_json::from_value(json!({"email":"scope@example.test","password":"ignored","firstName":"Scope","lastName":"Test","roles":[],"organization":"another-org"})).unwrap();
    assert!(work.create::<Users>(dto).await.is_err());
    assert!(work.commit().await.is_err());
}

struct InterruptedMapper;
impl DirectoryMapper<Organizations> for InterruptedMapper {
    type Create = ();
    type Update = ();
    type Output = OrganizationDto;
    type Context = ();
    async fn create<T: DirectoryTransaction>(
        &self,
        (): (),
        work: &mut DirectoryUnitOfWork<T>,
        (): &(),
    ) -> BlueprintResult<CreateOrganizationDto> {
        work.create::<Organizations>(organization("interrupted-staged"))
            .await?;
        std::future::pending().await
    }
    async fn update<T: DirectoryTransaction>(
        &self,
        (): (),
        _: &mut DirectoryUnitOfWork<T>,
        (): &(),
    ) -> BlueprintResult<UpdateOrganizationDto> {
        Err(invalid())
    }
    async fn project(&self, value: OrganizationDto, (): &()) -> BlueprintResult<OrganizationDto> {
        Ok(value)
    }
}
#[tokio::test]
async fn canceled_mapper_future_poison_prevents_partial_commit() {
    let store = Arc::new(MemoryDirectoryStore::default());
    let service = BaseIamDirectory::new(store.clone());
    let mut work = service.unit_of_work().await.unwrap();
    // Poll once: the in-memory graph write completes, then the hook is pending.
    let mut future = Box::pin(work.mapped_create::<Organizations, _>(&InterruptedMapper, (), &()));
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    assert!(future.as_mut().poll(&mut context).is_pending());
    drop(future);
    assert!(work.commit().await.is_err());
    let mut tx = store.begin(false).await.unwrap();
    assert!(
        tx.list(DirectoryKind::Organization, None, None)
            .await
            .unwrap()
            .is_empty()
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn caught_mapper_panic_prevents_commit_and_invalid_email_is_rejected() {
    let store = Arc::new(MemoryDirectoryStore::default());
    let service = BaseIamDirectory::new(store.clone());
    let user = mapped_graph(&service).await;
    let mut work = service.unit_of_work().await.unwrap();
    let organization = work
        .create::<Organizations>(organization("panic-staged"))
        .await
        .unwrap();
    let context = Context {
        organization: organization.id.clone(),
        prefix: "panic".into(),
        reject: false,
    };
    let mut future = Box::pin(work.mapped_get::<Users, _>(&Mapper, &user, &context));
    let waker = std::task::Waker::noop();
    let mut poll_context = std::task::Context::from_waker(waker);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| future
            .as_mut()
            .poll(&mut poll_context)))
        .is_err()
    );
    drop(future);
    assert!(work.commit().await.is_err());
    let mut read = store.begin(false).await.unwrap();
    assert!(
        read.get(DirectoryKind::Organization, &organization.id)
            .await
            .unwrap()
            .is_none()
    );
    read.commit().await.unwrap();
    let mut work = service.unit_of_work().await.unwrap();
    let dto=serde_json::from_value(json!({"email":"invalid","password":"ignored","firstName":"Invalid","lastName":"Email","roles":[]})).unwrap();
    assert!(work.create::<Users>(dto).await.is_err());
    assert!(work.commit().await.is_err());
}

#[tokio::test]
async fn bounded_filters_preserve_order_scope_and_projection_rollback() {
    use crate::directory::filter::{DirectoryFilter as F, Predicate as P, UserField};
    let store = Arc::new(MemoryDirectoryStore::default());
    let service = BaseIamDirectory::new(store.clone());
    let user = mapped_graph(&service).await;
    let mut work = service.unit_of_work().await.unwrap();
    let filter = F::Compare {
        field: UserField::Email,
        test: P::IsNotNull,
    };
    let ids = vec![user.to_uppercase(), user.clone()];
    let rows = work.list::<Users>(Some(&ids), &filter).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, user);
    assert_eq!(
        work.list::<Users>(None, &F::All(vec![]))
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        work.list::<Users>(None, &F::Any(vec![]))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        work.list::<Users>(None, &F::Not(Box::new(F::Any(vec![]))))
            .await
            .unwrap()
            .len(),
        1
    );
    let context = Context {
        organization: rows[0].organization.clone().unwrap(),
        prefix: "list".into(),
        reject: false,
    };
    let rows = work
        .mapped_list::<Users, _>(&Mapper, None, &filter, &context)
        .await
        .unwrap();
    assert_eq!(rows[0].display, "list:Updated");
    work.commit().await.unwrap();
    let scoped = BaseIamDirectory::new(store.clone())
        .with_organization_scope(uuid::Uuid::new_v4().to_string());
    let mut work = scoped.unit_of_work().await.unwrap();
    let broad = F::Any(vec![
        filter.clone(),
        F::Not(Box::new(F::Compare {
            field: UserField::Id,
            test: P::Eq(uuid::Uuid::new_v4().to_string()),
        })),
    ]);
    assert!(work.list::<Users>(None, &broad).await.unwrap().is_empty());
    assert!(
        work.list::<Users>(None, &F::All(vec![]))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        work.list::<Users>(None, &F::Not(Box::new(F::Any(vec![]))))
            .await
            .unwrap()
            .is_empty()
    );
    work.commit().await.unwrap();
    let mut work = service.unit_of_work().await.unwrap();
    let org = work
        .create::<Organizations>(organization("list-projection-rollback"))
        .await
        .unwrap();
    let context = Context {
        reject: true,
        ..context
    };
    assert!(
        work.mapped_list::<Users, _>(&Mapper, None, &F::All(vec![]), &context)
            .await
            .is_err()
    );
    assert!(work.commit().await.is_err());
    let mut tx = store.begin(false).await.unwrap();
    assert!(
        tx.get(DirectoryKind::Organization, &org.id)
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn scan_limit_is_explicit_even_when_no_rows_would_match() {
    use crate::directory::filter::{DirectoryFilter as F, PermissionField, Predicate as P};
    let store = Arc::new(MemoryDirectoryStore::default());
    let mut tx = store.begin(true).await.unwrap();
    for index in 0..4097 {
        tx.insert(
            DirectoryKind::Permission,
            json!({"id":uuid::Uuid::from_u128(index).to_string(),"slug":"present"}),
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let service = BaseIamDirectory::new(store);
    let mut work = service.unit_of_work().await.unwrap();
    let result = work
        .list::<Permissions>(
            None,
            &F::Compare {
                field: PermissionField::Slug,
                test: P::Eq("absent".into()),
            },
        )
        .await;
    assert_eq!(result.err().unwrap().code, "iam_directory_scan_limit");
    assert!(work.commit().await.is_err());
}

struct NoReads;
impl DirectoryTransaction for NoReads {
    async fn get(&mut self, _: DirectoryKind, _: &str) -> BlueprintResult<Option<Value>> {
        panic!("invalid filter read storage")
    }
    async fn list(
        &mut self,
        _: DirectoryKind,
        _: Option<&[String]>,
        _: Option<&str>,
    ) -> BlueprintResult<Vec<Value>> {
        panic!("invalid filter scanned storage")
    }
    async fn insert(&mut self, _: DirectoryKind, _: Value) -> BlueprintResult<()> {
        panic!("unexpected write")
    }
    async fn replace(&mut self, _: DirectoryKind, _: Value) -> BlueprintResult<()> {
        panic!("unexpected write")
    }
    async fn delete(&mut self, _: DirectoryKind, _: &str) -> BlueprintResult<()> {
        panic!("unexpected write")
    }
    async fn commit(self) -> BlueprintResult<()> {
        Ok(())
    }
}
#[tokio::test]
async fn malformed_or_overbudget_predicates_fail_before_storage_reads() {
    use crate::directory::filter::{DirectoryFilter as F, Predicate as P, UserField};
    let leaf = F::Compare {
        field: UserField::Email,
        test: P::Eq("ok".into()),
    };
    let mut deep = leaf.clone();
    for _ in 0..18 {
        deep = F::Not(Box::new(deep));
    }
    for filter in [
        deep,
        F::Any(vec![leaf; 257]),
        F::Compare {
            field: UserField::Email,
            test: P::Eq("x".repeat(65537)),
        },
        F::Compare {
            field: UserField::Id,
            test: P::Eq("invalid-uuid".into()),
        },
        F::Related {
            field: UserField::Email,
            ids: vec![],
        },
        F::Compare {
            field: UserField::RoleIds,
            test: P::IsNull,
        },
        F::Compare {
            field: UserField::Email,
            test: P::In(vec!["a".into(); 4097]),
        },
    ] {
        let mut work = DirectoryUnitOfWork::new(NoReads, None);
        assert!(work.list::<Users>(None, &filter).await.is_err());
        assert!(work.commit().await.is_err());
    }
    let mut work = DirectoryUnitOfWork::new(NoReads, None);
    assert!(
        work.list::<Users>(
            Some(&["bad-id".into()]),
            &F::Compare {
                field: UserField::Email,
                test: P::IsNull
            }
        )
        .await
        .is_err()
    );
    assert!(work.commit().await.is_err());
    assert!(
        serde_json::from_value::<F<UserField>>(
            json!({"compare":{"field":"password","test":{"eq":"secret"}}})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<F<UserField>>(
            json!({"compare":{"field":"email","test":{"like":"%"}}})
        )
        .is_err()
    );
}
