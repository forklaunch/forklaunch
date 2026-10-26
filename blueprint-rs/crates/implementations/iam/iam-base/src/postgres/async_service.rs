//! Trait implementations keep the persistent backend usable through public IAM boundaries.
use super::PostgresIamService;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::domain::Page;
use forklaunch_core::domain::PageRequest;
use forklaunch_iam_interface::{
    AsyncIamRepository, AsyncIamService, AsyncIamSessions, Credentials, Organization, Permission,
    Role, Session, User,
};

impl AsyncIamService for PostgresIamService {
    async fn get_user(&self, id: &str) -> BlueprintResult<Option<User>> {
        PostgresIamService::get_user(self, id).await
    }
    async fn find_user_by_email(&self, email: &str) -> BlueprintResult<Option<User>> {
        PostgresIamService::find_user_by_email(self, email).await
    }
    async fn list_users(&self, request: PageRequest) -> BlueprintResult<Page<User>> {
        PostgresIamService::list_users(self, request).await
    }
    async fn authenticate(&self, credentials: Credentials) -> BlueprintResult<Session> {
        PostgresIamService::authenticate(self, credentials).await
    }
    async fn permissions_for_user(&self, user_id: &str) -> BlueprintResult<Vec<Permission>> {
        PostgresIamService::permissions_for_user(self, user_id).await
    }
    async fn get_organization(&self, id: &str) -> BlueprintResult<Option<Organization>> {
        PostgresIamService::get_organization(self, id).await
    }
    async fn list_organizations(
        &self,
        request: PageRequest,
    ) -> BlueprintResult<Page<Organization>> {
        PostgresIamService::list_organizations(self, request).await
    }
    async fn roles_for_user(&self, user_id: &str) -> BlueprintResult<Vec<Role>> {
        PostgresIamService::roles_for_user(self, user_id).await
    }
}

impl AsyncIamSessions for PostgresIamService {
    async fn validate_session(&self, token: &str) -> BlueprintResult<Option<Session>> {
        PostgresIamService::validate_session(self, token).await
    }
    async fn revoke_session(&self, token: &str) -> BlueprintResult<bool> {
        PostgresIamService::revoke_session(self, token).await
    }
    async fn purge_expired_sessions(&self) -> BlueprintResult<u64> {
        PostgresIamService::purge_expired_sessions(self).await
    }
}

impl AsyncIamRepository for PostgresIamService {
    async fn create_user(&self, user: User, password: String) -> BlueprintResult<User> {
        PostgresIamService::create_user(self, user, password).await
    }
    async fn update_user(&self, user: User) -> BlueprintResult<bool> {
        PostgresIamService::update_user(self, user).await
    }
    async fn delete_user(&self, id: &str) -> BlueprintResult<bool> {
        PostgresIamService::delete_user(self, id).await
    }
    async fn set_password(&self, user_id: &str, password: String) -> BlueprintResult<bool> {
        PostgresIamService::set_password(self, user_id, password).await
    }
    async fn create_organization(&self, organization: Organization) -> BlueprintResult<()> {
        PostgresIamService::create_organization(self, organization).await
    }
    async fn update_organization(&self, organization: Organization) -> BlueprintResult<bool> {
        PostgresIamService::update_organization(self, organization).await
    }
    async fn delete_organization(&self, id: &str) -> BlueprintResult<bool> {
        PostgresIamService::delete_organization(self, id).await
    }
    async fn put_role(&self, role: Role) -> BlueprintResult<()> {
        PostgresIamService::put_role(self, role).await
    }
}
