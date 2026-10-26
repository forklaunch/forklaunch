//! Idempotent bootstrap of roles and login users from a TOML seed file, run by
//! a generated service as `iam --seed`. Rerunning updates roles, profiles and
//! passwords in place; nothing is deleted. Passwords are never stored in the
//! file: each user names the process variable that holds its password.
//!
//! [`SeedConfig::apply`] writes the login records (`fl_iam_users`, roles).
//! [`SeedConfig::apply_directory`] then writes the IAM directory records that
//! `POST /user` would: permissions, roles, organizations and a directory user
//! whose id is the login user's id, so the seeded user surfaces roles and can
//! use the JWT-protected directory routes.
//!
//! ```toml
//! [[roles]]
//! name = "admin"
//! permissions = ["platform:read", "platform:write"]
//!
//! [[organizations]]
//! id = "seed-organization"
//! name = "Default organization"
//! domain = "example.com"
//! subscription = "seed-subscription"
//!
//! [[users]]
//! id = "seed-admin"
//! email = "admin@example.com"
//! display_name = "Administrator"
//! roles = ["admin"]
//! organization = "seed-organization"
//! password_env = "IAM_SEED_ADMIN_PASSWORD"
//! ```
use crate::directory::{DirectoryKind, DirectoryStore, DirectoryTransaction, mapping, record_id};
use forklaunch_core::domain::{BlueprintError, BlueprintResult};
use forklaunch_iam_interface::{AsyncIamRepository, Permission, Role, User};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

/// The roles and users a seed run creates or updates.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedConfig {
    #[serde(default)]
    pub roles: Vec<SeedRole>,
    #[serde(default)]
    pub users: Vec<SeedUser>,
    /// Directory organizations, created with exactly these ids.
    #[serde(default)]
    pub organizations: Vec<SeedOrganization>,
}

/// A directory organization. `id` is kept, so reruns update it in place.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedOrganization {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub subscription: String,
}

/// A role and its `resource:action` permission slugs.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedRole {
    pub name: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// A login user. `id` is used only when the email is new; an existing user
/// with the same email keeps its id.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedUser {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    /// Process variable holding the password; required and non-empty at seed time.
    pub password_env: String,
    /// Directory first name; defaults to `display_name`, else the email's local part.
    pub first_name: Option<String>,
    /// Directory last name; defaults to empty.
    pub last_name: Option<String>,
    /// Id of an organization listed in this file.
    pub organization: Option<String>,
}

/// What a seed run did, for the operator's log. Never contains passwords.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeedReport {
    pub roles: Vec<String>,
    pub created_users: Vec<String>,
    pub updated_users: Vec<String>,
    /// Login user id of each seed user, in file order. An existing user keeps
    /// its id, so this can differ from the file's `id`.
    pub user_ids: Vec<String>,
}

fn invalid(message: impl Into<String>) -> BlueprintError {
    BlueprintError::new("iam_seed_invalid", message)
}

impl SeedConfig {
    /// Parses and validates a seed file.
    ///
    /// # Errors
    /// Rejects malformed TOML, unknown fields, blank names, permission slugs
    /// without `resource:action` and duplicate emails. User roles may name roles
    /// that already exist in storage, so they are not cross-checked.
    pub fn from_toml(source: &str) -> BlueprintResult<Self> {
        let config: Self = toml::from_str(source)
            .map_err(|error| invalid(format!("invalid seed file: {error}")))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> BlueprintResult<()> {
        let blank = |value: &str| value.trim().is_empty();
        for role in &self.roles {
            if blank(&role.name) {
                return Err(invalid("seed role names cannot be blank"));
            }
            for permission in &role.permissions {
                if split_permission(permission).is_none() {
                    return Err(invalid(format!(
                        "seed permission {permission:?} must be resource:action"
                    )));
                }
            }
        }
        let mut organizations = std::collections::HashSet::new();
        for organization in &self.organizations {
            if [
                &organization.id,
                &organization.name,
                &organization.domain,
                &organization.subscription,
            ]
            .iter()
            .any(|value| blank(value))
            {
                return Err(invalid(
                    "seed organizations need id, name, domain and subscription",
                ));
            }
            if !organizations.insert(organization.id.trim()) {
                return Err(invalid(format!(
                    "seed organization {} is listed twice",
                    organization.id
                )));
            }
        }
        let mut emails = std::collections::HashSet::new();
        for user in &self.users {
            if blank(&user.id) || blank(&user.email) || blank(&user.password_env) {
                return Err(invalid("seed users need id, email and password_env"));
            }
            if let Some(organization) = &user.organization
                && !organizations.contains(organization.trim())
            {
                return Err(invalid(format!(
                    "seed user {} names organization {organization}, which is not listed",
                    user.email
                )));
            }
            if !emails.insert(user.email.to_ascii_lowercase()) {
                return Err(invalid(format!(
                    "seed email {} is listed twice",
                    user.email
                )));
            }
        }
        Ok(())
    }

    /// Creates or updates every role, then every user, reading each password
    /// through `password`. Idempotent.
    ///
    /// # Errors
    /// Fails before writing anything when a password variable is unset or
    /// empty; storage errors stop the run (earlier writes remain).
    pub async fn apply<R, F>(&self, iam: &R, password: F) -> BlueprintResult<SeedReport>
    where
        R: AsyncIamRepository,
        F: Fn(&str) -> Option<String>,
    {
        let mut passwords = Vec::with_capacity(self.users.len());
        for user in &self.users {
            let value = password(&user.password_env)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| invalid(format!("{} is not set or is empty", user.password_env)))?;
            passwords.push(value);
        }
        let mut report = SeedReport::default();
        for seed in &self.roles {
            let mut role = Role::new(seed.name.trim());
            role.permissions = seed
                .permissions
                .iter()
                .filter_map(|slug| split_permission(slug))
                .map(|(resource, action)| Permission::new(resource, action))
                .collect();
            iam.put_role(role).await?;
            report.roles.push(seed.name.trim().to_owned());
        }
        for (seed, password) in self.users.iter().zip(passwords) {
            let mut user = User::new(seed.id.trim(), seed.email.trim());
            user.display_name.clone_from(&seed.display_name);
            user.roles.clone_from(&seed.roles);
            if let Some(existing) = iam.find_user_by_email(seed.email.trim()).await? {
                user.id.clone_from(&existing.id);
                iam.update_user(user).await?;
                iam.set_password(&existing.id, password).await?;
                report.updated_users.push(seed.email.clone());
                report.user_ids.push(existing.id);
            } else {
                let id = user.id.clone();
                iam.create_user(user, password).await?;
                report.created_users.push(seed.email.clone());
                report.user_ids.push(id);
            }
        }
        Ok(report)
    }

    /// Writes the directory records `POST /user` creates for every seed user,
    /// in one transaction: a permission per slug, a role per seed role, each
    /// organization, and a directory user whose id is the login id from
    /// `report` (from [`Self::apply`]). Existing records are matched by slug,
    /// role name or id and updated in place, so reruns are idempotent. User
    /// roles that are not directory roles are skipped.
    ///
    /// # Errors
    /// Fails when `report` is not from this file, and on storage errors and
    /// conflicts (for example a different directory user already holding the
    /// seed email). Nothing is written on failure.
    pub async fn apply_directory<S: DirectoryStore>(
        &self,
        store: &S,
        report: &SeedReport,
    ) -> BlueprintResult<()> {
        if report.user_ids.len() != self.users.len() {
            return Err(invalid("seed report does not match this seed file"));
        }
        let mut tx = store.begin(true).await?;
        let permissions = self.seed_permissions(&mut tx).await?;
        let roles = self.seed_roles(&mut tx, &permissions).await?;
        for organization in &self.organizations {
            let id = organization.id.trim();
            let fields = json!({
                "name": organization.name.trim(),
                "domain": organization.domain.trim(),
                "subscription": organization.subscription.trim(),
            });
            upsert(&mut tx, DirectoryKind::Organization, id, fields).await?;
        }
        for (seed, id) in self.users.iter().zip(&report.user_ids) {
            let email = seed.email.trim();
            let first_name = seed
                .first_name
                .as_deref()
                .or(seed.display_name.as_deref())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| email.split('@').next().unwrap_or(email));
            let user_roles: Vec<&String> = seed
                .roles
                .iter()
                .filter_map(|name| roles.get(name.trim()))
                .collect();
            let mut fields = json!({
                "email": email,
                "firstName": first_name,
                "lastName": seed.last_name.as_deref().map_or("", str::trim),
                "roles": user_roles,
            });
            if let Some(organization) = &seed.organization {
                fields["organization"] = json!(organization.trim());
            }
            upsert(&mut tx, DirectoryKind::User, id, fields).await?;
        }
        tx.commit().await
    }

    /// Directory permission id for every slug the seed roles use.
    async fn seed_permissions<T: DirectoryTransaction>(
        &self,
        tx: &mut T,
    ) -> BlueprintResult<HashMap<String, String>> {
        let mut by_slug = existing_by(tx, DirectoryKind::Permission, "slug").await?;
        for role in &self.roles {
            for slug in &role.permissions {
                let slug = slug.trim();
                if !by_slug.contains_key(slug) {
                    let row = mapping::create_with_id(
                        tx,
                        DirectoryKind::Permission,
                        json!({ "slug": slug }),
                        None,
                    )
                    .await?;
                    by_slug.insert(slug.to_owned(), record_id(&row)?.to_owned());
                }
            }
        }
        Ok(by_slug)
    }

    /// Directory role id for every role name, after creating or updating the
    /// seed roles with exactly their seed permissions.
    async fn seed_roles<T: DirectoryTransaction>(
        &self,
        tx: &mut T,
        permissions: &HashMap<String, String>,
    ) -> BlueprintResult<HashMap<String, String>> {
        let mut by_name = existing_by(tx, DirectoryKind::Role, "name").await?;
        for role in &self.roles {
            let name = role.name.trim();
            let mut permission_ids: Vec<&String> = Vec::new();
            for slug in &role.permissions {
                let id = &permissions[slug.trim()];
                if !permission_ids.contains(&id) {
                    permission_ids.push(id);
                }
            }
            if let Some(id) = by_name.get(name) {
                mapping::update(
                    tx,
                    DirectoryKind::Role,
                    json!({ "id": id, "permissionIds": permission_ids }),
                )
                .await?;
            } else {
                let row = mapping::create_with_id(
                    tx,
                    DirectoryKind::Role,
                    json!({ "name": name, "permissionIds": permission_ids }),
                    None,
                )
                .await?;
                by_name.insert(name.to_owned(), record_id(&row)?.to_owned());
            }
        }
        Ok(by_name)
    }
}

/// Existing rows of `kind` keyed by `field`; the oldest row wins a duplicate.
async fn existing_by<T: DirectoryTransaction>(
    tx: &mut T,
    kind: DirectoryKind,
    field: &str,
) -> BlueprintResult<HashMap<String, String>> {
    let mut result = HashMap::new();
    for row in tx.list(kind, None, None).await? {
        if let Some(key) = row.get(field).and_then(Value::as_str) {
            result
                .entry(key.to_owned())
                .or_insert(record_id(&row)?.to_owned());
        }
    }
    Ok(result)
}

/// Creates `kind` with exactly `id`, or updates the existing row's `fields`.
async fn upsert<T: DirectoryTransaction>(
    tx: &mut T,
    kind: DirectoryKind,
    id: &str,
    mut fields: Value,
) -> BlueprintResult<()> {
    if tx.get(kind, id).await?.is_some() {
        fields["id"] = json!(id);
        mapping::update(tx, kind, fields).await?;
    } else {
        mapping::create_with_id(tx, kind, fields, Some(id)).await?;
    }
    Ok(())
}

fn split_permission(slug: &str) -> Option<(&str, &str)> {
    let (resource, action) = slug.split_once(':')?;
    (!resource.trim().is_empty() && !action.trim().is_empty())
        .then(|| (resource.trim(), action.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_files_are_validated_before_any_write() {
        let config = SeedConfig::from_toml(
            "[[roles]]\nname='admin'\npermissions=['platform:read','platform:write']\n\
             [[users]]\nid='a'\nemail='a@example.test'\nroles=['admin']\npassword_env='A_PASSWORD'\n",
        )
        .unwrap();
        assert_eq!(config.roles[0].permissions.len(), 2);
        for invalid in [
            "[[roles]]\nname=' '\n",
            "[[roles]]\nname='x'\npermissions=['platform']\n",
            "[[users]]\nid='a'\nemail='a@x'\npassword_env=''\n",
            "[[users]]\nid='a'\nemail='a@x'\npassword_env='P'\n[[users]]\nid='b'\nemail='A@x'\npassword_env='P'\n",
            "unknown = 1\n",
        ] {
            assert!(SeedConfig::from_toml(invalid).is_err(), "{invalid}");
        }
        // Users may only name organizations the file lists.
        assert!(
            SeedConfig::from_toml(
                "[[users]]\nid='a'\nemail='a@x.test'\npassword_env='P'\norganization='missing'\n"
            )
            .is_err()
        );
        assert!(
            SeedConfig::from_toml(
                "[[organizations]]\nid='o'\nname='O'\ndomain='o.test'\nsubscription=' '\n"
            )
            .is_err()
        );
    }

    const SEED: &str = "[[roles]]\nname='admin'\npermissions=['platform:read','platform:write']\n\
        [[roles]]\nname='viewer'\npermissions=['platform:read']\n\
        [[organizations]]\nid='seed-organization'\nname='Default'\ndomain='example.com'\nsubscription='seed-subscription'\n\
        [[users]]\nid='seed-admin'\nemail='admin@example.com'\ndisplay_name='Administrator'\nroles=['admin','login-only']\n\
        organization='seed-organization'\npassword_env='P'\n";

    fn report(ids: &[&str]) -> SeedReport {
        SeedReport {
            user_ids: ids.iter().map(|id| (*id).to_owned()).collect(),
            ..SeedReport::default()
        }
    }

    /// The seeded admin gets the same directory records `POST /user` creates,
    /// under its login id, so surfacing its roles works; reruns change nothing.
    #[tokio::test]
    async fn seeding_creates_directory_records_under_the_login_id_idempotently() {
        use crate::directory::{memory::MemoryDirectoryStore, service::BaseIamDirectory};
        use forklaunch_iam_interface::services::{OrganizationService, UserService};
        let config = SeedConfig::from_toml(SEED).unwrap();
        let store = std::sync::Arc::new(MemoryDirectoryStore::default());
        let directory = BaseIamDirectory::new(store.clone());
        // A role created through the API before seeding is adopted by name.
        let mut tx = store.begin(true).await.unwrap();
        let viewer =
            mapping::create_with_id(&mut tx, DirectoryKind::Role, json!({"name":"viewer"}), None)
                .await
                .unwrap();
        tx.commit().await.unwrap();

        let counts = || async {
            let mut tx = store.begin(false).await.unwrap();
            let mut counts = Vec::new();
            for kind in [
                DirectoryKind::Permission,
                DirectoryKind::Role,
                DirectoryKind::User,
                DirectoryKind::Organization,
            ] {
                counts.push(tx.list(kind, None, None).await.unwrap().len());
            }
            counts
        };
        for _ in 0..2 {
            config
                .apply_directory(store.as_ref(), &report(&["seed-admin"]))
                .await
                .unwrap();
            assert_eq!(counts().await, [2, 2, 1, 1]);
        }

        let roles = directory.surface_roles("seed-admin").await.unwrap();
        assert_eq!(roles.len(), 1, "login-only roles are not directory roles");
        assert_eq!(roles[0].name, "admin");
        let mut slugs: Vec<_> = directory
            .surface_permissions("seed-admin")
            .await
            .unwrap()
            .into_iter()
            .map(|permission| permission.slug)
            .collect();
        slugs.sort();
        assert_eq!(slugs, ["platform:read", "platform:write"]);
        let user = directory.get_user("seed-admin").await.unwrap();
        assert_eq!(user.email, "admin@example.com");
        assert_eq!(user.first_name, "Administrator");
        assert_eq!(user.organization.as_deref(), Some("seed-organization"));
        // JWT directory routes resolve organization scope from the subject.
        assert_eq!(
            directory
                .get_organization_id_by_user_id("seed-admin")
                .await
                .unwrap()
                .as_deref(),
            Some("seed-organization")
        );
        let organization = directory
            .get_organization("seed-organization")
            .await
            .unwrap();
        assert_eq!(organization.users.len(), 1);
        let mut tx = store.begin(false).await.unwrap();
        let viewer = tx
            .get(DirectoryKind::Role, record_id(&viewer).unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(viewer["permissions"].as_array().unwrap().len(), 1);

        // The report must come from this file.
        assert!(
            config
                .apply_directory(store.as_ref(), &report(&[]))
                .await
                .is_err()
        );
    }
}
