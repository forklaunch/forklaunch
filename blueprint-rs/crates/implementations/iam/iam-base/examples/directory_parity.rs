//! Actual TypeScript IAM mapper/service comparison harness.
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_iam_base::directory::{memory::MemoryDirectoryStore, service::BaseIamDirectory};
use forklaunch_iam_interface::services::{
    OrganizationService, PermissionService, RoleService, UserService,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Value = serde_json::from_reader(std::io::stdin())?;
    if input["contracts"] == true {
        println!(
            "{}",
            forklaunch_iam_interface::module_contracts::registry()
                .into_artifact()
                .to_json()
        );
        return Ok(());
    }
    let service = BaseIamDirectory::new(Arc::new(MemoryDirectoryStore::default()));
    let mut aliases = BTreeMap::new();
    let mut results = Vec::new();
    for action in input["actions"].as_array().ok_or("Actions required")? {
        let mut action = action.clone();
        resolve(&mut action, &aliases);
        results.push(match run(&service, &action).await {
            Ok(result) => {
                if let Some(label) = action["label"].as_str() {
                    aliases.insert(
                        label.to_owned(),
                        result["id"]
                            .as_str()
                            .ok_or("Created ID required")?
                            .to_owned(),
                    );
                }
                if let Some(labels) = action["labels"].as_array() {
                    for (label, row) in labels
                        .iter()
                        .zip(result.as_array().ok_or("Created rows required")?)
                    {
                        aliases.insert(
                            label.as_str().ok_or("Label required")?.to_owned(),
                            row["id"].as_str().ok_or("Created ID required")?.to_owned(),
                        );
                    }
                }
                json!({"ok":result})
            }
            Err(_) => json!({"error":true}),
        });
    }
    for result in &mut results {
        normalize(result, &aliases);
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
fn resolve(value: &mut Value, aliases: &BTreeMap<String, String>) {
    match value {
        Value::String(value) => {
            if let Some(id) = value.strip_prefix('@').and_then(|label| aliases.get(label)) {
                *value = id.clone();
            }
        }
        Value::Array(values) => values.iter_mut().for_each(|value| resolve(value, aliases)),
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| resolve(value, aliases)),
        _ => {}
    }
}
fn normalize(value: &mut Value, aliases: &BTreeMap<String, String>) {
    match value {
        Value::String(value) => {
            if let Some((label, _)) = aliases.iter().find(|(_, id)| *id == value) {
                *value = format!("@{label}");
            }
        }
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| normalize(value, aliases)),
        Value::Object(values) => {
            values.remove("createdAt");
            values.remove("updatedAt");
            values
                .values_mut()
                .for_each(|value| normalize(value, aliases));
        }
        _ => {}
    }
}
#[allow(clippy::too_many_lines)]
async fn run(
    service: &BaseIamDirectory<MemoryDirectoryStore>,
    action: &Value,
) -> BlueprintResult<Value> {
    let input = action.get("input").cloned().unwrap_or_else(|| json!({}));
    let id = input["id"].as_str().unwrap_or("");
    let ids: Vec<String> = input
        .get("ids")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| invalid())?
        .unwrap_or_default();
    let organization = input["organization"].as_str();
    macro_rules! request {
        ($method:ident) => {
            serde_json::to_value(
                service
                    .$method(serde_json::from_value(input.clone()).map_err(|_| invalid())?)
                    .await?,
            )
            .map_err(|_| invalid())
        };
    }
    macro_rules! get {
        ($method:ident) => {
            serde_json::to_value(service.$method(id).await?).map_err(|_| invalid())
        };
    }
    macro_rules! batch {
        ($method:ident) => {
            serde_json::to_value(service.$method(&ids).await?).map_err(|_| invalid())
        };
    }
    match (
        action["service"].as_str().unwrap_or(""),
        action["method"].as_str().unwrap_or(""),
    ) {
        ("Permission", "createPermission") => request!(create_permission),
        ("Permission", "getPermission") => get!(get_permission),
        ("Permission", "updatePermission") => request!(update_permission),
        ("Permission", "deletePermission") => get!(delete_permission),
        ("Permission", "createBatchPermissions") => request!(create_batch_permissions),
        ("Permission", "getBatchPermissions") => batch!(get_batch_permissions),
        ("Permission", "updateBatchPermissions") => request!(update_batch_permissions),
        ("Permission", "deleteBatchPermissions") => batch!(delete_batch_permissions),
        ("Role", "createRole") => request!(create_role),
        ("Role", "getRole") => get!(get_role),
        ("Role", "updateRole") => request!(update_role),
        ("Role", "deleteRole") => get!(delete_role),
        ("Role", "createBatchRoles") => request!(create_batch_roles),
        ("Role", "getBatchRoles") => batch!(get_batch_roles),
        ("Role", "updateBatchRoles") => request!(update_batch_roles),
        ("Role", "deleteBatchRoles") => batch!(delete_batch_roles),
        ("User", "createUser") => request!(create_user),
        ("User", "getUser") => get!(get_user),
        ("User", "updateUser") => request!(update_user),
        ("User", "deleteUser") => {
            serde_json::to_value(service.delete_user(id, organization).await?)
                .map_err(|_| invalid())
        }
        ("User", "createBatchUsers") => request!(create_batch_users),
        ("User", "getBatchUsers") => {
            serde_json::to_value(service.get_batch_users(&ids, organization).await?)
                .map_err(|_| invalid())
        }
        ("User", "updateBatchUsers") => request!(update_batch_users),
        ("User", "deleteBatchUsers") => {
            serde_json::to_value(service.delete_batch_users(&ids, organization).await?)
                .map_err(|_| invalid())
        }
        ("Organization", "createOrganization") => request!(create_organization),
        ("Organization", "getOrganization") => get!(get_organization),
        ("Organization", "updateOrganization") => request!(update_organization),
        ("Organization", "deleteOrganization") => get!(delete_organization),
        ("User", "getOrganizationIdByUserId") => get!(get_organization_id_by_user_id),
        ("User", "surfaceRoles") => get!(surface_roles),
        ("User", "surfacePermissions") => get!(surface_permissions),
        _ => Err(invalid()),
    }
}
fn invalid() -> BlueprintError {
    BlueprintError::new("fixture_invalid", "Invalid IAM fixture input")
}
