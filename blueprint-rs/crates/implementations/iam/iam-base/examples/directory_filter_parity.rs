//! JSON fixture bridge for comparisons against actual `MikroORM` `PostgreSQL`.
use forklaunch_iam_base::directory::{
    DirectoryKind, DirectoryStore, DirectoryTransaction,
    filter::DirectoryFilter,
    memory::MemoryDirectoryStore,
    service::BaseIamDirectory,
    unit_of_work::{Organizations, Permissions, Roles, Users},
};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    sync::Arc,
};
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let store = Arc::new(MemoryDirectoryStore::default());
    let mut tx = store.begin(true).await.unwrap();
    for (key, kind) in [
        ("organizations", DirectoryKind::Organization),
        ("permissions", DirectoryKind::Permission),
        ("roles", DirectoryKind::Role),
        ("users", DirectoryKind::User),
    ] {
        for row in input[key].as_array().unwrap() {
            tx.insert(kind, row.clone()).await.unwrap();
        }
    }
    tx.commit().await.unwrap();
    let mut output = Vec::new();
    for case in input["cases"].as_array().unwrap() {
        let mut service = BaseIamDirectory::new(store.clone());
        if let Some(scope) = case["scope"].as_str() {
            service = service.with_organization_scope(scope.into());
        }
        let mut work = service.unit_of_work().await.unwrap();
        let ids: Option<Vec<String>> = case
            .get("ids")
            .map(|ids| serde_json::from_value(ids.clone()).unwrap());
        macro_rules! run {($family:ty)=>{{
            match serde_json::from_value::<DirectoryFilter<<$family as forklaunch_iam_base::directory::unit_of_work::DirectoryEntity>::Field>>(case["filter"].clone()) {
                Ok(filter)=>match work.list::<$family>(ids.as_deref(),&filter).await {Ok(rows)=>json!({"ids":rows.into_iter().map(|row|row.id).collect::<Vec<_>>()}),Err(error)=>json!({"error":error.code})},
                Err(_)=>json!({"error":"unsupported_filter"}),
            }
        }}}
        output.push(match case["kind"].as_str().unwrap() {
            "user" => run!(Users),
            "role" => run!(Roles),
            "permission" => run!(Permissions),
            "organization" => run!(Organizations),
            _ => panic!("fixture family"),
        });
    }
    println!("{}", serde_json::to_string(&output).unwrap());
}
