use super::*;

#[test]
fn module_exports_follow_rustfmt_module_order() {
    let mut lib = "pub mod application;\npub mod config;\npub mod domain;\npub mod registrations;\npub mod server;\npub use contracts as sdk;\n".to_owned();
    for id in ["s3", "redis"] {
        let export = format!("#[path = \"infrastructure/{id}.rs\"]\npub mod {id}_infrastructure;\n");
        insert_module_export(&mut lib, &export, &format!("{id}_infrastructure"));
    }
    assert_eq!(lib, "pub mod application;\npub mod config;\npub mod domain;\n#[path = \"infrastructure/redis.rs\"]\npub mod redis_infrastructure;\npub mod registrations;\n#[path = \"infrastructure/s3.rs\"]\npub mod s3_infrastructure;\npub mod server;\npub use contracts as sdk;\n");
}

#[test]
fn shared_backends_leave_only_in_their_generated_form() {
    let root = tempfile::tempdir().unwrap();
    let mut generated = DockerCompose::default();
    add_redis_to_docker_compose("app", &mut generated, &mut IndexMap::new(), 0).unwrap();
    let mut edit = CapabilityEdit::new(root.path(), "app", "alpha", root.path(), vec![], vec![]);
    let mut compose = generated.clone();
    release_backend(&mut edit, &mut compose, &generated, "redis").unwrap();
    assert!(!compose.services.contains_key("redis"));
    let mut edited = generated.clone();
    edited.services.get_mut("redis").unwrap().image = Some("human/redis".into());
    release_backend(&mut edit, &mut edited, &generated, "redis").unwrap();
    assert!(edited.services.contains_key("redis"));
    let mut referenced = generated.clone();
    let user = crate::core::docker::DockerService {
        environment: Some([("CACHE".to_owned(), "redis://redis:6379/1".to_owned())].into_iter().collect()),
        ..Default::default()
    };
    referenced.services.insert("user".into(), user);
    release_backend(&mut edit, &mut referenced, &generated, "redis").unwrap();
    assert!(referenced.services.contains_key("redis"));
    // `init --infrastructure` writes the same backend with `environment: {}`.
    let mut initialized = generated.clone();
    initialized.services.get_mut("redis").unwrap().environment = Some(Default::default());
    release_backend(&mut edit, &mut initialized, &generated, "redis").unwrap();
    assert!(!initialized.services.contains_key("redis"));
}
