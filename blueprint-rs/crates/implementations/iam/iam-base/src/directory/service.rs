//! Default mapper behavior and atomic CRUD orchestration for the IAM directory.
use super::{
    DirectoryKind, DirectoryStore, DirectoryTransaction, invalid, not_found, record_id,
    storage_error,
};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_iam_interface::services::{
    CreateOrganizationDto, CreatePermissionDto, CreateRoleDto, CreateUserDto, OrganizationDto,
    OrganizationService, PermissionDto, PermissionService, RoleDto, RoleService,
    UpdateOrganizationDto, UpdatePermissionDto, UpdateRoleDto, UpdateUserDto, UserDto, UserService,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct BaseIamDirectory<S> {
    store: Arc<S>,
    organization_scope: Option<String>,
}
impl<S: DirectoryStore + 'static> BaseIamDirectory<S> {
    #[must_use]
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            organization_scope: None,
        }
    }
    /// Begins a caller-owned atomic directory transaction with this service's
    /// trusted tenant, audit context and organization scope. Commit explicitly;
    /// dropping it or any failed operation prevents partial graph persistence.
    /// # Errors
    /// Returns the store's transaction error.
    pub async fn unit_of_work(
        &self,
    ) -> BlueprintResult<super::unit_of_work::DirectoryUnitOfWork<S::Transaction<'_>>> {
        Ok(super::unit_of_work::DirectoryUnitOfWork::new(
            self.store.begin(true).await?,
            self.organization_scope.clone(),
        ))
    }
    /// Restrict user reads and mutations to a server-resolved organization.
    #[must_use]
    pub fn with_organization_scope(mut self, organization: String) -> Self {
        self.organization_scope = Some(organization);
        self
    }
    fn authorize_row(&self, kind: DirectoryKind, row: &Value) -> BlueprintResult<()> {
        if kind == DirectoryKind::User
            && self
                .organization_scope
                .as_deref()
                .is_some_and(|scope| row["organization"].as_str() != Some(scope))
        {
            return Err(not_found());
        }
        Ok(())
    }
    fn authorize_mutation(&self, row: &Value) -> BlueprintResult<()> {
        if row["organization"].as_str().is_none() {
            return Err(not_found());
        }
        if self
            .organization_scope
            .as_deref()
            .is_some_and(|scope| row["organization"].as_str() != Some(scope))
        {
            return Err(forklaunch_core::domain::BlueprintError::new(
                "iam_directory_forbidden",
                "Forbidden: user belongs to another organization",
            ));
        }
        Ok(())
    }
    #[must_use]
    pub fn store(&self) -> &Arc<S> {
        &self.store
    }
    async fn create<T: DeserializeOwned>(
        &self,
        kind: DirectoryKind,
        requests: Vec<Value>,
    ) -> BlueprintResult<Vec<T>> {
        validate_batch(&requests)?;
        let mut tx = self.store.begin(true).await?;
        let mut rows = Vec::with_capacity(requests.len());
        for request in requests {
            rows.push(super::mapping::create(&mut tx, kind, request).await?);
        }
        let result = hydrate_rows(&mut tx, kind, rows).await?;
        tx.commit().await?;
        Ok(result)
    }
    async fn update<T: DeserializeOwned>(
        &self,
        kind: DirectoryKind,
        requests: Vec<Value>,
    ) -> BlueprintResult<Vec<T>> {
        validate_batch(&requests)?;
        let mut tx = self.store.begin(true).await?;
        let mut rows = Vec::with_capacity(requests.len());
        for request in requests {
            if self.organization_scope.is_some() && kind == DirectoryKind::User {
                let row = tx
                    .get(kind, record_id(&request)?)
                    .await?
                    .ok_or_else(not_found)?;
                self.authorize_mutation(&row)?;
            }
            rows.push(super::mapping::update(&mut tx, kind, request).await?);
        }
        let result = hydrate_rows(&mut tx, kind, rows).await?;
        tx.commit().await?;
        Ok(result)
    }
    async fn get<T: DeserializeOwned>(&self, kind: DirectoryKind, id: &str) -> BlueprintResult<T> {
        let mut tx = self.store.begin(false).await?;
        let row = tx.get(kind, id).await?.ok_or_else(not_found)?;
        self.authorize_row(kind, &row)?;
        let result = decode(super::mapping::hydrate(&mut tx, kind, row).await?)?;
        tx.commit().await?;
        Ok(result)
    }
    async fn list<T: DeserializeOwned>(
        &self,
        kind: DirectoryKind,
        ids: &[String],
        organization: Option<&str>,
    ) -> BlueprintResult<Vec<T>> {
        if ids.len() > 4096 {
            return Err(invalid());
        }
        let mut tx = self.store.begin(false).await?;
        let organization = if kind == DirectoryKind::User {
            self.organization_scope.as_deref().or(organization)
        } else {
            organization
        };
        let rows = tx.list(kind, Some(ids), organization).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(decode(super::mapping::hydrate(&mut tx, kind, row).await?)?);
        }
        tx.commit().await?;
        Ok(result)
    }
    async fn delete(
        &self,
        kind: DirectoryKind,
        ids: &[String],
        organization: Option<&str>,
    ) -> BlueprintResult<()> {
        if ids.len() > 4096 {
            return Err(invalid());
        }
        let mut tx = self.store.begin(true).await?;
        if kind == DirectoryKind::User && self.organization_scope.is_some() {
            for id in ids {
                let row = tx.get(kind, id).await?.ok_or_else(not_found)?;
                self.authorize_mutation(&row)?;
            }
        }
        let organization = if kind == DirectoryKind::User {
            self.organization_scope.as_deref().or(organization)
        } else {
            organization
        };
        let rows = tx.list(kind, Some(ids), organization).await?;
        for row in rows {
            tx.delete(kind, record_id(&row)?).await?;
        }
        tx.commit().await
    }
}
async fn hydrate_rows<T: DeserializeOwned, X: DirectoryTransaction>(
    tx: &mut X,
    kind: DirectoryKind,
    rows: Vec<Value>,
) -> BlueprintResult<Vec<T>> {
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let current = tx
            .get(kind, record_id(&row)?)
            .await?
            .ok_or_else(storage_error)?;
        result.push(decode(super::mapping::hydrate(tx, kind, current).await?)?);
    }
    Ok(result)
}

fn validate_batch(requests: &[Value]) -> BlueprintResult<()> {
    if requests.len() > 4096
        || serde_json::to_vec(requests).map_err(|_| invalid())?.len() > 1024 * 1024
    {
        return Err(invalid());
    }
    Ok(())
}
fn encode<T: Serialize>(request: T) -> BlueprintResult<Value> {
    serde_json::to_value(request).map_err(|_| invalid())
}
fn decode<T: DeserializeOwned>(value: Value) -> BlueprintResult<T> {
    serde_json::from_value(value).map_err(|_| storage_error())
}
fn single<T>(values: Vec<T>) -> BlueprintResult<T> {
    values.into_iter().next().ok_or_else(storage_error)
}

impl<S: DirectoryStore + 'static> PermissionService for BaseIamDirectory<S> {
    async fn create_permission(
        &self,
        request: CreatePermissionDto,
    ) -> BlueprintResult<PermissionDto> {
        single(
            self.create(DirectoryKind::Permission, vec![encode(request)?])
                .await?,
        )
    }
    async fn get_permission(&self, id: &str) -> BlueprintResult<PermissionDto> {
        self.get(DirectoryKind::Permission, id).await
    }
    async fn update_permission(
        &self,
        request: UpdatePermissionDto,
    ) -> BlueprintResult<PermissionDto> {
        single(
            self.update(DirectoryKind::Permission, vec![encode(request)?])
                .await?,
        )
    }
    async fn delete_permission(&self, id: &str) -> BlueprintResult<()> {
        self.delete(DirectoryKind::Permission, &[id.to_owned()], None)
            .await
    }
    async fn create_batch_permissions(
        &self,
        requests: Vec<CreatePermissionDto>,
    ) -> BlueprintResult<Vec<PermissionDto>> {
        self.create(
            DirectoryKind::Permission,
            requests
                .into_iter()
                .map(encode)
                .collect::<BlueprintResult<Vec<_>>>()?,
        )
        .await
    }
    async fn update_batch_permissions(
        &self,
        requests: Vec<UpdatePermissionDto>,
    ) -> BlueprintResult<Vec<PermissionDto>> {
        self.update(
            DirectoryKind::Permission,
            requests
                .into_iter()
                .map(encode)
                .collect::<BlueprintResult<Vec<_>>>()?,
        )
        .await
    }
    async fn get_batch_permissions(&self, ids: &[String]) -> BlueprintResult<Vec<PermissionDto>> {
        self.list(DirectoryKind::Permission, ids, None).await
    }
    async fn delete_batch_permissions(&self, ids: &[String]) -> BlueprintResult<()> {
        self.delete(DirectoryKind::Permission, ids, None).await
    }
}

impl<S: DirectoryStore + 'static> RoleService for BaseIamDirectory<S> {
    async fn create_role(&self, request: CreateRoleDto) -> BlueprintResult<RoleDto> {
        single(
            self.create(DirectoryKind::Role, vec![encode(request)?])
                .await?,
        )
    }
    async fn get_role(&self, id: &str) -> BlueprintResult<RoleDto> {
        self.get(DirectoryKind::Role, id).await
    }
    async fn update_role(&self, request: UpdateRoleDto) -> BlueprintResult<RoleDto> {
        single(
            self.update(DirectoryKind::Role, vec![encode(request)?])
                .await?,
        )
    }
    async fn delete_role(&self, id: &str) -> BlueprintResult<()> {
        self.delete(DirectoryKind::Role, &[id.to_owned()], None)
            .await
    }
    async fn create_batch_roles(
        &self,
        requests: Vec<CreateRoleDto>,
    ) -> BlueprintResult<Vec<RoleDto>> {
        self.create(
            DirectoryKind::Role,
            requests
                .into_iter()
                .map(encode)
                .collect::<BlueprintResult<Vec<_>>>()?,
        )
        .await
    }
    async fn update_batch_roles(
        &self,
        requests: Vec<UpdateRoleDto>,
    ) -> BlueprintResult<Vec<RoleDto>> {
        self.update(
            DirectoryKind::Role,
            requests
                .into_iter()
                .map(encode)
                .collect::<BlueprintResult<Vec<_>>>()?,
        )
        .await
    }
    async fn get_batch_roles(&self, ids: &[String]) -> BlueprintResult<Vec<RoleDto>> {
        self.list(DirectoryKind::Role, ids, None).await
    }
    async fn delete_batch_roles(&self, ids: &[String]) -> BlueprintResult<()> {
        self.delete(DirectoryKind::Role, ids, None).await
    }
}

impl<S: DirectoryStore + 'static> UserService for BaseIamDirectory<S> {
    async fn create_user(&self, request: CreateUserDto) -> BlueprintResult<UserDto> {
        single(
            self.create(DirectoryKind::User, vec![encode(request)?])
                .await?,
        )
    }
    async fn get_user(&self, id: &str) -> BlueprintResult<UserDto> {
        self.get(DirectoryKind::User, id).await
    }
    async fn update_user(&self, request: UpdateUserDto) -> BlueprintResult<UserDto> {
        single(
            self.update(DirectoryKind::User, vec![encode(request)?])
                .await?,
        )
    }
    async fn delete_user(&self, id: &str, organization: Option<&str>) -> BlueprintResult<()> {
        self.delete(DirectoryKind::User, &[id.to_owned()], organization)
            .await
    }
    async fn create_batch_users(
        &self,
        requests: Vec<CreateUserDto>,
    ) -> BlueprintResult<Vec<UserDto>> {
        self.create(
            DirectoryKind::User,
            requests
                .into_iter()
                .map(encode)
                .collect::<BlueprintResult<Vec<_>>>()?,
        )
        .await
    }
    async fn update_batch_users(
        &self,
        requests: Vec<UpdateUserDto>,
    ) -> BlueprintResult<Vec<UserDto>> {
        self.update(
            DirectoryKind::User,
            requests
                .into_iter()
                .map(encode)
                .collect::<BlueprintResult<Vec<_>>>()?,
        )
        .await
    }
    async fn get_batch_users(
        &self,
        ids: &[String],
        organization: Option<&str>,
    ) -> BlueprintResult<Vec<UserDto>> {
        self.list(DirectoryKind::User, ids, organization).await
    }
    async fn delete_batch_users(
        &self,
        ids: &[String],
        organization: Option<&str>,
    ) -> BlueprintResult<()> {
        self.delete(DirectoryKind::User, ids, organization).await
    }
    async fn get_organization_id_by_user_id(&self, id: &str) -> BlueprintResult<Option<String>> {
        let mut tx = self.store.begin(false).await?;
        let organization = tx.get(DirectoryKind::User, id).await?.and_then(|user| {
            user.get("organization")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        tx.commit().await?;
        Ok(organization)
    }
    async fn surface_roles(&self, id: &str) -> BlueprintResult<Vec<RoleDto>> {
        Ok(UserService::get_user(self, id).await?.roles)
    }
    async fn surface_permissions(&self, id: &str) -> BlueprintResult<Vec<PermissionDto>> {
        Ok(UserService::get_user(self, id)
            .await?
            .roles
            .into_iter()
            .flat_map(|role| role.permissions)
            .collect())
    }
}

impl<S: DirectoryStore + 'static> OrganizationService for BaseIamDirectory<S> {
    async fn create_organization(
        &self,
        request: CreateOrganizationDto,
    ) -> BlueprintResult<OrganizationDto> {
        single(
            self.create(DirectoryKind::Organization, vec![encode(request)?])
                .await?,
        )
    }
    async fn get_organization(&self, id: &str) -> BlueprintResult<OrganizationDto> {
        self.get(DirectoryKind::Organization, id).await
    }
    async fn update_organization(
        &self,
        request: UpdateOrganizationDto,
    ) -> BlueprintResult<OrganizationDto> {
        single(
            self.update(DirectoryKind::Organization, vec![encode(request)?])
                .await?,
        )
    }
    async fn delete_organization(&self, id: &str) -> BlueprintResult<()> {
        self.delete(DirectoryKind::Organization, &[id.to_owned()], None)
            .await
    }
}
