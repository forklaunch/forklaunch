use {{crate_module}}::contract_registry;

#[test]
fn durable_worker_contract_excludes_example_and_internal_http_routes() {
    let registry = contract_registry();
    assert!(registry.routes().is_empty());
    assert!(registry.into_artifact().to_json().contains("framework-rs"));
}
