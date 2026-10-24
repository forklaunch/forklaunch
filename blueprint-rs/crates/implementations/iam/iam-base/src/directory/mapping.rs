//! Application-configured mapper rules, including intentionally ignored fields.
use super::{DirectoryKind, DirectoryTransaction, invalid, not_found, record_id, storage_error};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_iam_interface::services::{OrganizationDto, PermissionDto, RoleDto, UserDto};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

pub(super) async fn create<T: DirectoryTransaction>(
    tx: &mut T,
    kind: DirectoryKind,
    request: Value,
) -> BlueprintResult<Value> {
    create_with_id(tx, kind, request, None).await
}

/// `create`, keeping a caller-chosen id instead of generating one. Request DTO
/// ids are always discarded; only trusted callers (the seed) supply `id`.
pub(crate) async fn create_with_id<T: DirectoryTransaction>(
    tx: &mut T,
    kind: DirectoryKind,
    request: Value,
    id: Option<&str>,
) -> BlueprintResult<Value> {
    let mut row = request.as_object().cloned().ok_or_else(invalid)?;
    validate_email(kind, &row)?;
    let add = ids(row.remove("addToRolesIds"))?;
    row.remove("id");
    let id = id.map_or_else(|| uuid::Uuid::new_v4().to_string(), str::to_owned);
    row.insert("id".into(), json!(id));
    let now = chrono::Utc::now().to_rfc3339();
    row.insert("createdAt".into(), json!(now));
    row.insert("updatedAt".into(), json!(now));
    match kind {
        DirectoryKind::Permission => {
            row.remove("providerFields");
        }
        DirectoryKind::Role => {
            row.remove("providerFields");
            let wanted = ids(row.remove("permissionIds"))?;
            row.insert(
                "permissions".into(),
                existing(tx, DirectoryKind::Permission, &wanted).await?,
            );
        }
        DirectoryKind::User => {
            row.remove("password");
            let wanted = ids(row.remove("roles"))?;
            row.insert(
                "roles".into(),
                existing(tx, DirectoryKind::Role, &wanted).await?,
            );
            if let Some(id) = row.get("organization").and_then(Value::as_str)
                && tx.get(DirectoryKind::Organization, id).await?.is_none()
            {
                row.remove("organization");
            }
            row.entry("providerFields").or_insert(Value::Null);
        }
        DirectoryKind::Organization => {
            row.insert("status".into(), json!("active"));
            row.entry("providerFields").or_insert(Value::Null);
        }
    }
    let row = Value::Object(row);
    tx.insert(kind, row.clone()).await?;
    if kind == DirectoryKind::Permission {
        change_roles(tx, &id, &add, &[]).await?;
    }
    Ok(row)
}

pub(crate) async fn update<T: DirectoryTransaction>(
    tx: &mut T,
    kind: DirectoryKind,
    request: Value,
) -> BlueprintResult<Value> {
    let id = record_id(&request)?.to_owned();
    let old = tx.get(kind, &id).await?.ok_or_else(not_found)?;
    let mut row = old.as_object().cloned().ok_or_else(storage_error)?;
    let mut patch = request.as_object().cloned().ok_or_else(invalid)?;
    validate_email(kind, &patch)?;
    let add = ids(patch.remove("addToRolesIds"))?;
    let remove = ids(patch.remove("removeFromRolesIds"))?;
    if matches!(kind, DirectoryKind::Permission | DirectoryKind::Role) {
        patch.remove("providerFields");
    }
    if kind == DirectoryKind::Role
        && let Some(wanted) = patch.remove("permissionIds")
    {
        patch.insert(
            "permissions".into(),
            existing(tx, DirectoryKind::Permission, &ids(Some(wanted))?).await?,
        );
    }
    if kind == DirectoryKind::User {
        patch.remove("password");
        if let Some(wanted) = patch.remove("roles") {
            patch.insert(
                "roles".into(),
                existing(tx, DirectoryKind::Role, &ids(Some(wanted))?).await?,
            );
        }
    }
    row.extend(patch);
    row.insert("updatedAt".into(), json!(chrono::Utc::now().to_rfc3339()));
    let row = Value::Object(row);
    tx.replace(kind, row.clone()).await?;
    if kind == DirectoryKind::Permission {
        change_roles(tx, &id, &add, &remove).await?;
    }
    Ok(row)
}

fn validate_email(
    kind: DirectoryKind,
    fields: &serde_json::Map<String, Value>,
) -> BlueprintResult<()> {
    // Match the configured Zod IAM mapper, rather than accepting a broader
    // mailbox grammar than the TypeScript application validates.
    static EMAIL: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^(?:[A-Za-z0-9_'+\-]+\.)*[A-Za-z0-9_'+\-]*[A-Za-z0-9_+-]@(?:[A-Za-z0-9][A-Za-z0-9\-]*\.)+[A-Za-z]{2,}$").expect("static email pattern")
    });
    if kind == DirectoryKind::User
        && let Some(email) = fields.get("email")
        && !email.as_str().is_some_and(|email| EMAIL.is_match(email))
    {
        return Err(invalid());
    }
    Ok(())
}

fn ids(value: Option<Value>) -> BlueprintResult<Vec<String>> {
    let values: Vec<String> = value.map_or_else(
        || Ok(Vec::new()),
        |value| serde_json::from_value(value).map_err(|_| invalid()),
    )?;
    if values.len() > 4096 || values.iter().any(|id| id.is_empty() || id.len() > 256) {
        return Err(invalid());
    }
    Ok(values)
}
async fn existing<T: DirectoryTransaction>(
    tx: &mut T,
    kind: DirectoryKind,
    wanted: &[String],
) -> BlueprintResult<Value> {
    let rows = tx.list(kind, Some(wanted), None).await?;
    rows.iter()
        .map(|row| record_id(row).map(|id| json!(id)))
        .collect::<BlueprintResult<Vec<_>>>()
        .map(Value::Array)
}
async fn change_roles<T: DirectoryTransaction>(
    tx: &mut T,
    permission: &str,
    add: &[String],
    remove: &[String],
) -> BlueprintResult<()> {
    let all = add.iter().chain(remove).cloned().collect::<Vec<_>>();
    for mut role in tx.list(DirectoryKind::Role, Some(&all), None).await? {
        let id = record_id(&role)?.to_owned();
        let mut permissions = ids(role.get("permissions").cloned())?;
        if add.contains(&id) && !permissions.iter().any(|id| id == permission) {
            permissions.push(permission.to_owned());
        }
        if remove.contains(&id) {
            permissions.retain(|id| id != permission);
        }
        role["permissions"] = json!(permissions);
        tx.replace(DirectoryKind::Role, role).await?;
    }
    Ok(())
}

pub(super) async fn hydrate<T: DirectoryTransaction>(
    tx: &mut T,
    kind: DirectoryKind,
    row: Value,
) -> BlueprintResult<Value> {
    match kind {
        DirectoryKind::Permission => normalize::<PermissionDto>(row),
        DirectoryKind::Role => role(tx, row).await,
        DirectoryKind::User => user(tx, row).await,
        DirectoryKind::Organization => organization(tx, row).await,
    }
}
async fn related<T: DirectoryTransaction>(
    tx: &mut T,
    kind: DirectoryKind,
    wanted: &[String],
) -> BlueprintResult<Vec<Value>> {
    let rows = tx.list(kind, Some(wanted), None).await?;
    let mut by_id = rows
        .into_iter()
        .map(|row| Ok((record_id(&row)?.to_owned(), row)))
        .collect::<BlueprintResult<std::collections::BTreeMap<_, _>>>()?;
    Ok(wanted.iter().filter_map(|id| by_id.remove(id)).collect())
}
async fn role<T: DirectoryTransaction>(tx: &mut T, mut row: Value) -> BlueprintResult<Value> {
    let wanted = ids(row.get("permissions").cloned())?;
    row["permissions"] = related(tx, DirectoryKind::Permission, &wanted)
        .await?
        .into_iter()
        .map(normalize::<PermissionDto>)
        .collect::<BlueprintResult<Vec<_>>>()
        .map(Value::Array)?;
    normalize::<RoleDto>(row)
}
async fn user<T: DirectoryTransaction>(tx: &mut T, mut row: Value) -> BlueprintResult<Value> {
    let wanted = ids(row.get("roles").cloned())?;
    let roles = related(tx, DirectoryKind::Role, &wanted).await?;
    let mut values = Vec::with_capacity(roles.len());
    for value in roles {
        values.push(role(tx, value).await?);
    }
    row["roles"] = Value::Array(values);
    let object = row.as_object_mut().ok_or_else(storage_error)?;
    for field in ["organization", "phoneNumber", "subscription"] {
        if object.get(field).is_some_and(Value::is_null) {
            object.remove(field);
        }
    }
    normalize::<UserDto>(row)
}
async fn organization<T: DirectoryTransaction>(
    tx: &mut T,
    mut row: Value,
) -> BlueprintResult<Value> {
    let id = record_id(&row)?.to_owned();
    let users = tx.list(DirectoryKind::User, None, Some(&id)).await?;
    let mut values = Vec::with_capacity(users.len());
    for value in users {
        values.push(user(tx, value).await?);
    }
    row["users"] = Value::Array(values);
    let object = row.as_object_mut().ok_or_else(storage_error)?;
    if object
        .get("logoUrl")
        .is_some_and(|value| value.is_null() || value == "")
    {
        object.remove("logoUrl");
    }
    normalize::<OrganizationDto>(row)
}
fn normalize<T: DeserializeOwned + Serialize>(value: Value) -> BlueprintResult<Value> {
    let dto: T = serde_json::from_value(value).map_err(|_| storage_error())?;
    serde_json::to_value(dto).map_err(|_| storage_error())
}
