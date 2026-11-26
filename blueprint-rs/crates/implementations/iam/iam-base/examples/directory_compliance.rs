//! Actual TypeScript compliance comparison with an application-selected policy.
use forklaunch_core::persistence::compliance::ComplianceMetadata;
use forklaunch_iam_base::directory::{
    DirectoryKind, DirectoryStore, DirectoryTransaction,
    compliance::{DirectoryCompliance, DirectoryComplianceService},
    memory::MemoryDirectoryStore,
};
use serde_json::{Value, json};
use std::sync::Arc;
#[allow(dead_code)]
#[derive(ComplianceMetadata)]
#[compliance(entity = "User")]
struct User {
    #[compliance(level = "none")]
    id: String,
    #[compliance(level = "pii")]
    email: String,
    #[compliance(level = "phi", name = "firstName")]
    first_name: String,
}
#[allow(dead_code)]
#[derive(ComplianceMetadata)]
#[compliance(entity = "Role")]
struct Role {
    #[compliance(level = "none")]
    id: String,
    #[compliance(level = "none")]
    owner: String,
    #[compliance(level = "pci")]
    label: String,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Value = serde_json::from_reader(std::io::stdin())?;
    let store = Arc::new(MemoryDirectoryStore::default());
    let mut tx = store.begin(true).await?;
    for (family, kind) in [("User", DirectoryKind::User), ("Role", DirectoryKind::Role)] {
        for row in input[family].as_array().ok_or("rows missing")? {
            tx.insert(kind, row.clone()).await?;
        }
    }
    tx.commit().await?;
    let config = DirectoryCompliance::default()
        .with_entity::<User>(DirectoryKind::User, "id")?
        .with_entity::<Role>(DirectoryKind::Role, "owner")?;
    let service = DirectoryComplianceService::new(store, config);
    let mut results = Vec::new();
    results.push(serde_json::to_value(service.export("subject").await?)?);
    results.push(serde_json::to_value(service.erase("subject").await?)?);
    results.push(serde_json::to_value(service.export("subject").await?)?);
    results.push(serde_json::to_value(service.export("foreign").await?)?);
    results.push(serde_json::to_value(service.erase("subject").await?)?);
    println!("{}", json!(results));
    Ok(())
}
