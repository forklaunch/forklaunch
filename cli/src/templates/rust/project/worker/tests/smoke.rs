use {{crate_module}}::{
    build_context, config::WorkerConfig, contract_registry, jobs::pending_jobs, run_once,
};

#[test]
fn generated_worker_processes_jobs() {
    let context = build_context(WorkerConfig::default()).expect("context should build");
    let jobs = pending_jobs(&context);
    let report = run_once(&context).expect("worker should process one batch");

    assert_eq!(jobs.len(), 1);
    assert_eq!(report.processed, 1);
    assert!(report.message.contains(&context.config().queue_name));
}

#[test]
fn generated_worker_exposes_framework_contracts() {
    let registry = contract_registry();
    let artifact = registry.clone().into_artifact().to_json();

    assert!(artifact.contains("framework-rs"));
    assert!(artifact.contains("Process Worker Jobs"));
    assert!(registry.routes().iter().any(|route| route.path == "/jobs/process"));
}
