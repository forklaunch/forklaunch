//! Async IAM boundaries for network services and persistent implementations.
//! Returned futures are Send so generic consumers work with Tokio and Axum.
use crate::{Credentials, Organization, Permission, Role, Session, User};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::domain::Page;
use forklaunch_core::domain::PageRequest;
use std::future::Future;

/// Asynchronous counterpart of the synchronous reference IAM service.
pub trait AsyncIamService: Send + Sync {
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn get_user(&self, id: &str) -> impl Future<Output = BlueprintResult<Option<User>>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn find_user_by_email(
        &self,
        email: &str,
    ) -> impl Future<Output = BlueprintResult<Option<User>>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn list_users(
        &self,
        request: PageRequest,
    ) -> impl Future<Output = BlueprintResult<Page<User>>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn authenticate(
        &self,
        credentials: Credentials,
    ) -> impl Future<Output = BlueprintResult<Session>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn permissions_for_user(
        &self,
        user_id: &str,
    ) -> impl Future<Output = BlueprintResult<Vec<Permission>>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn get_organization(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<Option<Organization>>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn list_organizations(
        &self,
        request: PageRequest,
    ) -> impl Future<Output = BlueprintResult<Page<Organization>>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn roles_for_user(
        &self,
        user_id: &str,
    ) -> impl Future<Output = BlueprintResult<Vec<Role>>> + Send;
}

/// Persistent session validation, revocation, and expiry cleanup.
pub trait AsyncIamSessions: AsyncIamService {
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn validate_session(
        &self,
        token: &str,
    ) -> impl Future<Output = BlueprintResult<Option<Session>>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn revoke_session(&self, token: &str) -> impl Future<Output = BlueprintResult<bool>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn purge_expired_sessions(&self) -> impl Future<Output = BlueprintResult<u64>> + Send;
}

/// Administrative mutations; authorization belongs to the calling service.
pub trait AsyncIamRepository: AsyncIamService {
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn create_user(
        &self,
        user: User,
        password: String,
    ) -> impl Future<Output = BlueprintResult<User>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn update_user(&self, user: User) -> impl Future<Output = BlueprintResult<bool>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn delete_user(&self, id: &str) -> impl Future<Output = BlueprintResult<bool>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn set_password(
        &self,
        user_id: &str,
        password: String,
    ) -> impl Future<Output = BlueprintResult<bool>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn create_organization(
        &self,
        organization: Organization,
    ) -> impl Future<Output = BlueprintResult<()>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn update_organization(
        &self,
        organization: Organization,
    ) -> impl Future<Output = BlueprintResult<bool>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn delete_organization(&self, id: &str) -> impl Future<Output = BlueprintResult<bool>> + Send;
    /// # Errors
    /// Returns a domain or storage error when the operation fails.
    fn put_role(&self, role: Role) -> impl Future<Output = BlueprintResult<()>> + Send;
}
