#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(feature = "browser-sessions")]
pub mod browser_sessions;
pub mod credentials;
pub mod directory;
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "jwt-sessions")]
pub mod jwt_sessions;
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "seed")]
pub mod seed;

use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::domain::Page;
use forklaunch_core::domain::PageRequest;
use forklaunch_iam_interface::{
    Credentials, IamService, Organization, Permission, Role, Session, User, invalid_credentials,
};

#[derive(Debug, Default)]
pub struct InMemoryIamService {
    users: Vec<User>,
    passwords_by_email: BTreeMap<String, String>,
    permissions_by_user_id: BTreeMap<String, Vec<Permission>>,
    organizations: Vec<Organization>,
    roles_by_user_id: BTreeMap<String, Vec<Role>>,
}

impl InMemoryIamService {
    #[must_use]
    pub fn new(users: Vec<User>) -> Self {
        Self {
            users,
            passwords_by_email: BTreeMap::new(),
            permissions_by_user_id: BTreeMap::new(),
            organizations: Vec::new(),
            roles_by_user_id: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_organization(mut self, organization: Organization) -> Self {
        self.organizations.push(organization);
        self
    }

    #[must_use]
    pub fn with_role(mut self, user_id: impl Into<String>, role: Role) -> Self {
        self.roles_by_user_id
            .entry(user_id.into())
            .or_default()
            .push(role);
        self
    }

    /// Stores a salted Argon2id password hash rather than the raw credential.
    ///
    /// # Errors
    /// Returns an error if secure randomness or password hashing fails.
    pub fn with_password(
        mut self,
        email: impl Into<String>,
        password: impl AsRef<str>,
    ) -> BlueprintResult<Self> {
        self.passwords_by_email
            .insert(email.into(), credentials::hash_password(password.as_ref())?);
        Ok(self)
    }

    #[must_use]
    pub fn with_permission(mut self, user_id: impl Into<String>, permission: Permission) -> Self {
        self.permissions_by_user_id
            .entry(user_id.into())
            .or_default()
            .push(permission);
        self
    }
}

impl IamService for InMemoryIamService {
    fn get_user(&self, id: &str) -> BlueprintResult<Option<User>> {
        Ok(self.users.iter().find(|user| user.id == id).cloned())
    }

    fn find_user_by_email(&self, email: &str) -> BlueprintResult<Option<User>> {
        Ok(self.users.iter().find(|user| user.email == email).cloned())
    }

    fn list_users(&self, request: PageRequest) -> BlueprintResult<Page<User>> {
        let start = request
            .cursor
            .as_ref()
            .and_then(|cursor| cursor.parse::<usize>().ok())
            .unwrap_or(0)
            .min(self.users.len());
        let end = start.saturating_add(request.limit).min(self.users.len());
        let mut page = Page::new(self.users[start..end].to_vec());

        if end < self.users.len() {
            page = page.with_next_cursor(end.to_string());
        }

        Ok(page)
    }

    fn authenticate(&self, credentials: Credentials) -> BlueprintResult<Session> {
        let expected_password = self
            .passwords_by_email
            .get(&credentials.email)
            .ok_or_else(invalid_credentials)?;
        if !credentials::verify_password(&credentials.password, expected_password) {
            return Err(invalid_credentials());
        }

        let user = self
            .find_user_by_email(&credentials.email)?
            .ok_or_else(invalid_credentials)?;

        if user.disabled {
            return Err(invalid_credentials());
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| invalid_credentials())?
            .as_secs();
        Ok(Session {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id,
            expires_at_unix_seconds: now.saturating_add(3600),
        })
    }

    fn permissions_for_user(&self, user_id: &str) -> BlueprintResult<Vec<Permission>> {
        Ok(self
            .permissions_by_user_id
            .get(user_id)
            .cloned()
            .unwrap_or_default())
    }

    fn get_organization(&self, id: &str) -> BlueprintResult<Option<Organization>> {
        Ok(self
            .organizations
            .iter()
            .find(|organization| organization.id == id)
            .cloned())
    }

    fn list_organizations(&self, request: PageRequest) -> BlueprintResult<Page<Organization>> {
        let start = request
            .cursor
            .as_ref()
            .and_then(|cursor| cursor.parse::<usize>().ok())
            .unwrap_or(0)
            .min(self.organizations.len());
        let end = start
            .saturating_add(request.limit)
            .min(self.organizations.len());
        let mut page = Page::new(self.organizations[start..end].to_vec());

        if end < self.organizations.len() {
            page = page.with_next_cursor(end.to_string());
        }

        Ok(page)
    }

    fn roles_for_user(&self, user_id: &str) -> BlueprintResult<Vec<Role>> {
        Ok(self
            .roles_by_user_id
            .get(user_id)
            .cloned()
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod security_tests;
