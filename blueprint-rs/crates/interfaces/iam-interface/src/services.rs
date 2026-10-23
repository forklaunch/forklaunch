//! Typed asynchronous IAM directory interfaces matching the TypeScript CRUD services.
//! Authentication/session identities remain in the parent module. Stores are
//! bound to a trusted tenant; caller-owned transaction adapters may implement
//! these interfaces without changing the DTO or HTTP contracts.
use chrono::{DateTime, Utc};
use forklaunch_core::domain::BlueprintResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;

fn optional<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePermissionDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub slug: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub add_to_roles_ids: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePermissionDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub slug: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub add_to_roles_ids: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub remove_from_roles_ids: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDto {
    pub id: String,
    pub slug: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub add_to_roles_ids: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoleDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub permission_ids: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRoleDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub permission_ids: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleDto {
    pub id: String,
    pub name: String,
    pub permissions: Vec<PermissionDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub email: String,
    pub password: String,
    pub first_name: String,
    pub last_name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub organization: Option<String>,
    pub roles: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub phone_number: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub subscription: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub email: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub password: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub first_name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub last_name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub roles: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub phone_number: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub subscription: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    pub id: String,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub organization: Option<String>,
    pub roles: Vec<RoleDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub phone_number: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub subscription: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateOrganizationDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub name: String,
    pub domain: String,
    pub subscription: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub logo_url: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOrganizationDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub domain: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub subscription: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub logo_url: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationDto {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub subscription: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub logo_url: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
    pub users: Vec<UserDto>,
    pub status: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub updated_at: Option<DateTime<Utc>>,
}

pub trait PermissionService: Send + Sync {
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn create_permission(
        &self,
        request: CreatePermissionDto,
    ) -> impl Future<Output = BlueprintResult<PermissionDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn get_permission(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<PermissionDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn update_permission(
        &self,
        request: UpdatePermissionDto,
    ) -> impl Future<Output = BlueprintResult<PermissionDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn delete_permission(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn create_batch_permissions(
        &self,
        requests: Vec<CreatePermissionDto>,
    ) -> impl Future<Output = BlueprintResult<Vec<PermissionDto>>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn get_batch_permissions(
        &self,
        ids: &[String],
    ) -> impl Future<Output = BlueprintResult<Vec<PermissionDto>>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn update_batch_permissions(
        &self,
        requests: Vec<UpdatePermissionDto>,
    ) -> impl Future<Output = BlueprintResult<Vec<PermissionDto>>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn delete_batch_permissions(
        &self,
        ids: &[String],
    ) -> impl Future<Output = BlueprintResult<()>> + Send;
}

pub trait RoleService: Send + Sync {
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn create_role(
        &self,
        request: CreateRoleDto,
    ) -> impl Future<Output = BlueprintResult<RoleDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn get_role(&self, id: &str) -> impl Future<Output = BlueprintResult<RoleDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn update_role(
        &self,
        request: UpdateRoleDto,
    ) -> impl Future<Output = BlueprintResult<RoleDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn delete_role(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn create_batch_roles(
        &self,
        requests: Vec<CreateRoleDto>,
    ) -> impl Future<Output = BlueprintResult<Vec<RoleDto>>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn get_batch_roles(
        &self,
        ids: &[String],
    ) -> impl Future<Output = BlueprintResult<Vec<RoleDto>>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn update_batch_roles(
        &self,
        requests: Vec<UpdateRoleDto>,
    ) -> impl Future<Output = BlueprintResult<Vec<RoleDto>>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn delete_batch_roles(
        &self,
        ids: &[String],
    ) -> impl Future<Output = BlueprintResult<()>> + Send;
}

pub trait UserService: Send + Sync {
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn create_user(
        &self,
        request: CreateUserDto,
    ) -> impl Future<Output = BlueprintResult<UserDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn get_user(&self, id: &str) -> impl Future<Output = BlueprintResult<UserDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn update_user(
        &self,
        request: UpdateUserDto,
    ) -> impl Future<Output = BlueprintResult<UserDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn delete_user(
        &self,
        id: &str,
        organization: Option<&str>,
    ) -> impl Future<Output = BlueprintResult<()>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn create_batch_users(
        &self,
        requests: Vec<CreateUserDto>,
    ) -> impl Future<Output = BlueprintResult<Vec<UserDto>>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn get_batch_users(
        &self,
        ids: &[String],
        organization: Option<&str>,
    ) -> impl Future<Output = BlueprintResult<Vec<UserDto>>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn update_batch_users(
        &self,
        requests: Vec<UpdateUserDto>,
    ) -> impl Future<Output = BlueprintResult<Vec<UserDto>>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn delete_batch_users(
        &self,
        ids: &[String],
        organization: Option<&str>,
    ) -> impl Future<Output = BlueprintResult<()>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn get_organization_id_by_user_id(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<Option<String>>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn surface_roles(&self, id: &str)
    -> impl Future<Output = BlueprintResult<Vec<RoleDto>>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn surface_permissions(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<Vec<PermissionDto>>> + Send;
}

pub trait OrganizationService: Send + Sync {
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn create_organization(
        &self,
        request: CreateOrganizationDto,
    ) -> impl Future<Output = BlueprintResult<OrganizationDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn get_organization(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<OrganizationDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn update_organization(
        &self,
        request: UpdateOrganizationDto,
    ) -> impl Future<Output = BlueprintResult<OrganizationDto>> + Send;
    /// # Errors
    /// Returns a domain, validation or tenant-bound storage error.
    fn delete_organization(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
}
