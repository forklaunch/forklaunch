use super::*;

fn billing_registry_fixture() -> String {
    #[derive(ramhorns::Content)]
    struct Context {
        name: &'static str,
        stripe: bool,
    }
    ramhorns::Template::new(include_str!("../../../templates/rust/billing/api/contracts/registry.rs"))
        .unwrap()
        .render(&Context {
            name: "billing",
            stripe: true,
        })
}

#[test]
fn builtin_capability_registry_handles_profile_builders_without_runtime_imports() {
    for source in [
        billing_registry_fixture(),
        include_str!("../../../templates/rust/iam/api/contracts/registry.rs").replace("{{{name}}}", "identity"),
        include_str!("../../../templates/rust/cac/api/contracts/registry.rs").replace("{{{name}}}", "codes"),
    ] {
        let source = format!(
            "// App-owned registry note: keep this formatting.\n{source}\nconst USER_NOTE: &str = \"registry {{ text }}\";\n"
        );
        let added = builtin_capability_registry(&source, true, false).unwrap();
        assert!(added.starts_with("// App-owned registry note: keep this formatting.\n"));
        assert!(added.ends_with("const USER_NOTE: &str = \"registry { text }\";\n"));
        assert!(added.contains(BUILTIN_CONTRACT_EXPORT));
        assert!(!added.contains("crate::capabilities"));
        assert_eq!(
            builtin_capability_registry(&added, true, false).unwrap(),
            added
        );
        let expected = if source.contains("contract_registry_with_profiles") {
            2
        } else {
            1
        };
        assert_eq!(added.matches(BUILTIN_REGISTRY_MARKER).count(), expected);
        let removed = builtin_capability_registry(&added, false, true).unwrap();
        assert!(!removed.contains("platform_events"));
        assert!(removed.contains("// App-owned registry note: keep this formatting."));
        assert!(removed.contains("const USER_NOTE: &str = \"registry { text }\";"));
    }
    assert!(
        builtin_capability_registry(
            "pub fn contract_registry() -> Registry { make_registry() }",
            true,
            false
        )
        .is_err()
    );
    assert!(builtin_capability_registry("pub mod platform_events;", true, false).is_err());
    let app_hook = format!(
        "{BUILTIN_CONTRACT_EXPORT}pub fn contract_registry() -> Registry {{ let mut registry = make_registry(); platform_events::register_contracts(&mut registry); registry }}"
    );
    assert_eq!(
        builtin_capability_registry(&app_hook, true, false).unwrap(),
        app_hook
    );
    assert_eq!(
        builtin_capability_registry(&app_hook, false, false).unwrap(),
        app_hook
    );
    let contract = builtin_capability_contract();
    syn::parse_file(&contract).unwrap();
    assert!(contract.contains("pub occurred_at: String"));
    assert!(contract.contains("[401, 404, 500, 503]"));
    assert!(!contract.contains("forklaunch_blueprint_core"));
    let runtime = builtin_capability_module(
        &render_module_with_layout(&["payments".into(), "email".into()], false).unwrap(),
    )
    .unwrap();
    syn::parse_file(&runtime).unwrap();
    assert!(runtime.contains("pub use crate::api::contracts::registry::platform_events::{contracts, register_contracts};"));
    assert!(!runtime.contains("pub fn contracts()"));
    assert!(runtime.contains(
        "return next.run(Request::from_parts(parts, Body::from(body))).await;"
    ));
    assert!(runtime.contains("nonce_store::store()"));
}

#[test]
fn builtin_capabilities_share_build_contracts_and_preserve_owned_removal() {
    for authored_edits in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let service = root.path().join("src/billing");
        std::fs::create_dir_all(service.join("api/contracts")).unwrap();
        std::fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        let cargo = "[package]\nname = 'billing'\nversion = '0.1.0'\n[dependencies]\nforklaunch-core = { version = '=0.1.0', features = ['openapi'] }\n[build-dependencies]\nforklaunch-core = { version = '=0.1.0', features = ['build'] }\n";
        let lib = "// App exports\npub mod api;\npub mod sdk;\n";
        let sdk = "pub use crate::api::contracts::registry::*;\n";
        let server = "pub fn server() { router.serve_with_default_mcp(listener, registry); }\n";
        let registry = billing_registry_fixture();
        for (path, text) in [
            ("Cargo.toml", cargo),
            ("lib.rs", lib),
            ("sdk.rs", sdk),
            ("server.rs", server),
            ("api/contracts/registry.rs", registry.as_str()),
        ] {
            std::fs::write(service.join(path), text).unwrap();
        }
        std::fs::write(
            root.path().join(".forklaunch/manifest.toml"),
            "app_name = 'test'\nprojects = []\n",
        )
        .unwrap();
        std::fs::write(root.path().join("docker-compose.yaml"), "services:\n  billing:\n    image: test-rust\n    environment:\n      PORT: '8000'\nvolumes: {}\nnetworks: {}\n").unwrap();
        let change = |name: &str, caps: &[&str], adding: bool| {
            let caps = caps
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>();
            let mut edit = CapabilityEdit::new(
                root.path(),
                "test",
                "billing",
                &service,
                caps.clone(),
                caps,
            );
            edit.native = true;
            apply(&mut edit, super::super::find(name).unwrap(), adding).unwrap();
            edit.commit().unwrap();
        };
        let mut preview = CapabilityEdit::new(
            root.path(),
            "test",
            "billing",
            &service,
            vec!["email".into()],
            vec!["email".into()],
        );
        preview.native = true;
        apply(&mut preview, super::super::find("email").unwrap(), true).unwrap();
        assert!(!preview.changed_paths().is_empty());
        assert_eq!(
            std::fs::read_to_string(service.join("api/contracts/registry.rs")).unwrap(),
            registry
        );
        assert_eq!(
            std::fs::read_to_string(service.join("Cargo.toml")).unwrap(),
            cargo
        );
        assert!(!service.join("api/contracts/platform_events.rs").exists());
        change("email", &["email"], true);
        let installed =
            std::fs::read_to_string(service.join("api/contracts/registry.rs")).unwrap();
        assert_eq!(installed.matches(BUILTIN_REGISTRY_MARKER).count(), 2);
        assert_eq!(
            std::fs::read_to_string(service.join("sdk.rs")).unwrap(),
            sdk
        );
        let manifest: toml::Value =
            toml::from_str(&std::fs::read_to_string(service.join("Cargo.toml")).unwrap())
                .unwrap();
        for scope in ["dependencies", "build-dependencies"] {
            assert_eq!(
                manifest[scope]["forklaunch-core"]["version"].as_str(),
                Some("=0.1.0")
            );
            assert!(
                manifest[scope]["forklaunch-core"]["features"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value.as_str() == Some("typed-schema"))
            );
        }
        if authored_edits {
            std::fs::write(service.join("api/contracts/registry.rs"), format!("{installed}\n// App-owned after infra add\npub const APP_MARKER: bool = true;\n")).unwrap();
            let modified = std::fs::read_to_string(service.join("Cargo.toml")).unwrap();
            // Use TOML editing to retain a developer feature even if formatting changes.
            let mut modified: DocumentMut = modified.parse().unwrap();
            modified["build-dependencies"]["forklaunch-core"]["features"]
                .as_array_mut()
                .unwrap()
                .push("auth");
            std::fs::write(service.join("Cargo.toml"), modified.to_string()).unwrap();
        }
        change("sms", &["email", "sms"], true);
        change("email", &["sms"], false);
        assert!(service.join("api/contracts/platform_events.rs").exists());
        change("sms", &[], false);
        let removed =
            std::fs::read_to_string(service.join("api/contracts/registry.rs")).unwrap();
        if authored_edits {
            assert!(
                removed.contains(
                    "// App-owned after infra add\npub const APP_MARKER: bool = true;"
                )
            );
            assert!(!removed.contains("platform_events"));
            let manifest: toml::Value =
                toml::from_str(&std::fs::read_to_string(service.join("Cargo.toml")).unwrap())
                    .unwrap();
            assert!(
                manifest["build-dependencies"]["forklaunch-core"]["features"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value.as_str() == Some("auth"))
            );
            assert_eq!(
                manifest["dependencies"]["forklaunch-core"]["features"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
        } else {
            assert_eq!(removed, registry);
            assert_eq!(
                std::fs::read_to_string(service.join("Cargo.toml")).unwrap(),
                cargo
            );
        }
        assert_eq!(
            std::fs::read_to_string(service.join("server.rs")).unwrap(),
            server
        );
        assert_eq!(
            std::fs::read_to_string(service.join("lib.rs")).unwrap(),
            lib
        );
        assert_eq!(
            std::fs::read_to_string(service.join("sdk.rs")).unwrap(),
            sdk
        );
        assert!(!service.join("api/contracts/platform_events.rs").exists());
        assert!(!service.join("capabilities/mod.rs").exists());
    }
}

#[test]
fn payments_dependency_follows_the_framework_selection() {
    let root = tempfile::tempdir().unwrap();
    let service = root.path().join("src/billing");
    std::fs::create_dir_all(service.join("api/contracts")).unwrap();
    std::fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
    let cargo = "[package]\nname = 'billing'\nversion = '0.1.0'\n[dependencies]\nforklaunch-core = { version = '=0.1.0', features = ['openapi'] }\n[build-dependencies]\nforklaunch-core = { version = '=0.1.0', features = ['build'] }\n";
    for (path, text) in [
        ("Cargo.toml", cargo.to_owned()),
        ("lib.rs", "pub mod api;\n".to_owned()),
        ("server.rs", "pub fn server() { router.serve_with_default_mcp(listener, registry); }\n".to_owned()),
        // A module that does not already depend on Stripe billing.
        ("api/contracts/registry.rs", include_str!("../../../templates/rust/iam/api/contracts/registry.rs").to_owned()),
    ] {
        std::fs::write(service.join(path), text).unwrap();
    }
    std::fs::write(
        root.path().join(".forklaunch/manifest.toml"),
        "app_name = 'test'\nprojects = []\n\n[framework]\nversion = '0.1.3'\nblueprint_version = '0.1.1'\n",
    )
    .unwrap();
    std::fs::write(root.path().join("docker-compose.yaml"), "services:\n  billing:\n    image: test-rust\n    environment:\n      PORT: '8000'\nvolumes: {}\nnetworks: {}\n").unwrap();
    let change = |caps: &[&str], adding: bool| {
        let caps = caps.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>();
        let mut edit = CapabilityEdit::new(root.path(), "test", "billing", &service, caps.clone(), caps);
        edit.native = true;
        apply(&mut edit, super::super::find("payments").unwrap(), adding).unwrap();
        edit.commit().unwrap();
    };
    change(&["payments"], true);
    // Published blueprint crates, never a `.forklaunch/blueprint-rs` copy
    // that a relocated or containerized application cannot see.
    let manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(service.join("Cargo.toml")).unwrap()).unwrap();
    let stripe = &manifest["dependencies"]["forklaunch-billing-stripe"];
    assert_eq!(stripe["version"].as_str(), Some("=0.1.1"));
    assert!(stripe.get("path").is_none());
    change(&[], false);
    assert_eq!(std::fs::read_to_string(service.join("Cargo.toml")).unwrap(), cargo);
}

#[test]
fn inline_capability_host_collects_routes_and_binding_evidence_once() {
    let source = "pub fn routes()->Vec<u8>{let mut routes=Vec::new();routes}\npub fn binding_manifest()->Result<Vec<u8>,String>{let mut declarations=Vec::new();Ok(declarations)}";
    let added = inline_capability_registry(source, true).unwrap();
    assert_eq!(added.matches("controller_contract!").count(), 1);
    assert!(added.contains("platform_events::contract::registry()"));
    assert!(added.contains("platform_events::contract::binding_manifest()?"));
    assert_eq!(inline_capability_registry(&added, true).unwrap(), added);
    let removed = inline_capability_registry(&added, false).unwrap();
    assert!(!removed.contains("platform_events"));
    syn::parse_file(&removed).unwrap();
    let module =
        inline_capability_module(&render_module_with_layout(&["email".into()], true).unwrap());
    assert!(module.contains("ReceivePlatformEvent::bind_authorized"));
    assert!(module.contains("OriginalBodySignedPolicy for CallbackPolicy"));
    syn::parse_file(&module).unwrap();
}

#[test]
fn native_capabilities_stay_inside_the_shared_http_mcp_security_wrapper() {
    let source = include_str!("../../../templates/rust/project/native-service/api/routes/mod.rs");
    let wired = with_native_capability_router(source).unwrap();
    assert!(wired.contains(".with_managed_capabilities()?.finish()"));
    assert_eq!(wired.replace(".with_managed_capabilities()?", ""), source);
    let module = render_module_with_layout(&["email".into()], true).unwrap();
    assert!(module.contains("bind_authorized"));
    assert!(module.contains("async fn authorize_body"));
    assert!(module.contains("nonce_store::store()"));
    assert!(CONTRACT.contains("AccessLevel::Internal"));
    assert!(!CONTRACT.contains("forklaunch_blueprint_core"));
}

#[test]
fn capability_imports_follow_module_docs() {
    let server = include_str!("../../../templates/rust/project/module/server.rs");
    let wired = after_inner_docs(server, "use crate::capabilities::ManagedCapabilitiesRouterExt;\n");
    syn::parse_file(&wired).unwrap();
    assert!(wired.starts_with("//! "));
    assert_eq!(wired.replace("use crate::capabilities::ManagedCapabilitiesRouterExt;\n", ""), server);
    assert_eq!(after_inner_docs("fn main() {}\n", "use a;\n"), "use a;\nfn main() {}\n");
    assert_eq!(after_inner_docs("/*\n * Generated\n */\n\n//! Doc.\nuse b;\n", "use a;\n"),
        "/*\n * Generated\n */\n\n//! Doc.\nuse a;\nuse b;\n");
}

#[test]
fn capabilities_bind_every_generated_listener_entrypoint_and_legacy_mount() {
    for main in [
        include_str!("../../../templates/rust/project/service/src/main.rs"),
        include_str!("../../../templates/rust/project/module/server.rs"),
        "router.into_router_with_default_mcp()?;",
    ] {
        let wired = with_capability_router(main).unwrap();
        assert_eq!(wired.matches(".with_managed_capabilities()").count(), 1);
        assert_eq!(wired.replace(".with_managed_capabilities()", ""), main);
    }
    assert!(with_capability_router("router.into_router()").is_err());
    assert!(
        with_capability_router(
            "a.into_router_with_default_mcp(); b.serve_with_default_mcp(x,y)"
        )
        .is_err()
    );
}

#[test]
fn dependency_features_come_off_as_they_went_on() {
    for before in [
        "[dependencies]\nforklaunch-core = { version = '=0.1.0', features = ['openapi'] }\n",
        "[dependencies]\nforklaunch-core = \"=0.1.0\"\n",
        "[dependencies.forklaunch-core]\nfeatures = [\n    \"typed-schema\",\n    \"otlp\",\n]\nversion = \"=0.1.3\"\n",
    ] {
        let mut cargo: DocumentMut = before.parse().unwrap();
        ensure_dependency_feature(&mut cargo, "dependencies", "forklaunch-core", "instance-gateway").unwrap();
        assert_ne!(cargo.to_string(), before);
        release_dependency_feature(&mut cargo, "dependencies", "forklaunch-core", "instance-gateway").unwrap();
        assert_eq!(cargo.to_string(), before);
    }
    // A feature the developer added alongside stays.
    let mut cargo: DocumentMut = "[dependencies]\nforklaunch-core = \"=0.1.0\"\n".parse().unwrap();
    ensure_dependency_feature(&mut cargo, "dependencies", "forklaunch-core", "instance-gateway").unwrap();
    ensure_dependency_feature(&mut cargo, "dependencies", "forklaunch-core", "stripe-webhook").unwrap();
    release_dependency_feature(&mut cargo, "dependencies", "forklaunch-core", "instance-gateway").unwrap();
    let features = cargo["dependencies"]["forklaunch-core"]["features"].as_array().unwrap();
    assert_eq!(features.iter().filter_map(|value| value.as_str()).collect::<Vec<_>>(), ["stripe-webhook"]);
}

#[test]
fn dependencies_are_released_only_in_their_generated_form() {
    let root = tempfile::tempdir().unwrap();
    let cargo_path = root.path().join("Cargo.toml");
    let before = "[package]\nname = 'billing'\n[dependencies]\nuser = \"1\"\n";
    let add = |cargo: &mut DocumentMut| ensure_dependency(cargo, STRIPE, "../stripe", None);
    let mut edit = CapabilityEdit::new(root.path(), "test", "billing", root.path(), vec![], vec![]);
    let mut cargo: DocumentMut = before.parse().unwrap();
    add(&mut cargo).unwrap();
    release_dependency(&mut edit, &cargo_path, &mut cargo, STRIPE, add).unwrap();
    assert_eq!(cargo.to_string(), before);
    // A changed specification is the developer's.
    let mut cargo: DocumentMut = format!("{before}{STRIPE} = {{ path = \"../stripe\", features = [\"postgres-identity\"] }}\n").parse().unwrap();
    let edited = cargo.to_string();
    release_dependency(&mut edit, &cargo_path, &mut cargo, STRIPE, add).unwrap();
    assert_eq!(cargo.to_string(), edited);
    // As is one other code still uses.
    let cargo: DocumentMut = format!("{before}[features]\nbound = [\"{STRIPE}/postgres-identity\"]\n").parse().unwrap();
    assert!(dependency_in_use(root.path(), &cargo, STRIPE, &[]));
    let cargo: DocumentMut = before.parse().unwrap();
    assert!(!dependency_in_use(root.path(), &cargo, STRIPE, &[]));
    std::fs::create_dir_all(root.path().join("capabilities")).unwrap();
    std::fs::write(root.path().join("capabilities/mod.rs"), "use forklaunch_billing_stripe::client;\n").unwrap();
    assert!(!dependency_in_use(root.path(), &cargo, STRIPE, &[root.path().join("capabilities")]));
    std::fs::write(root.path().join("billing.rs"), "use forklaunch_billing_stripe::client;\n").unwrap();
    assert!(dependency_in_use(root.path(), &cargo, STRIPE, &[root.path().join("capabilities")]));
}

#[test]
fn inline_capability_host_round_trips_its_bytes() {
    for source in [
        "//! Generated private compiler host.\n#[path=\"../../domain/schemas/mod.rs\"]\npub mod schemas;\nforklaunch_core::contract::controller_contract!(health, \"../../api/controllers/health.rs\");\n\npub fn routes()->Vec<u8>{let mut routes=Vec::new();routes.extend(health::contract::registry().routes().iter().cloned());\nroutes}\npub fn binding_manifest()->Result<Vec<u8>,String>{let mut declarations=Vec::new();declarations.extend(health::contract::binding_manifest()?);\nOk(declarations)}",
        "pub fn routes() -> Vec<u8> {\n    let mut routes = Vec::new();\n    routes\n}\npub fn binding_manifest() -> Result<Vec<u8>, String> {\n    let mut declarations = Vec::new();\n    Ok(declarations)\n}\n",
    ] {
        let added = inline_capability_registry(source, true).unwrap();
        assert!(added.contains("platform_events::contract::registry()"));
        assert_eq!(inline_capability_registry(&added, true).unwrap(), added);
        assert_eq!(inline_capability_registry(&added, false).unwrap(), source);
    }
}

#[test]
fn deletion_reads_a_manifest_document_and_releases_only_the_last_gateway() {
    for (another_service, edited_mock) in [(false, false), (true, false), (false, true)] {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        let manifest = if another_service {
            "app_description = \"Document parsing regression\"\n[[projects]]\nname = \"other\"\n[projects.resources]\ncapabilities = [\"sms\"]\n"
        } else {
            "app_description = \"Document parsing regression\"\nprojects = []\n"
        };
        std::fs::write(root.path().join(".forklaunch/manifest.toml"), manifest).unwrap();
        let mut compose: DockerCompose = serde_yml::from_str("volumes: {}\nnetworks: {}\nservices:\n  removed:\n    environment:\n      INSTANCE_ID: local-instance\n  other:\n    environment:\n      SERVICE_PORT: '8123'\n").unwrap();
        let mut mock = generated_gateway_mock(None);
        if edited_mock {
            mock.image = Some("human/mock:fixture".into());
        }
        let env = mock.environment.as_mut().unwrap();
        env.insert("MOCK_EVENTS_URL_EMAIL".into(), "http://removed:8000".into());
        env.insert("MOCK_EVENTS_URL_SMS".into(), "http://other:8123".into());
        compose.services.insert(GATEWAY_MOCK_SERVICE.into(), mock);
        std::fs::write(root.path().join("docker-compose.yaml"), serde_yml::to_string(&compose).unwrap()).unwrap();
        cleanup_deleted_native_service(root.path(), "removed", &["email".into()]).unwrap();
        let compose: DockerCompose = serde_yml::from_str(
            &std::fs::read_to_string(root.path().join("docker-compose.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            compose.services.contains_key(GATEWAY_MOCK_SERVICE),
            another_service || edited_mock
        );
        if another_service || edited_mock {
            let env = compose.services[GATEWAY_MOCK_SERVICE]
                .environment
                .as_ref()
                .unwrap();
            assert!(!env.contains_key("MOCK_EVENTS_URL_EMAIL"));
        }
        if another_service {
            let env = compose.services[GATEWAY_MOCK_SERVICE].environment.as_ref().unwrap();
            assert_eq!(env["MOCK_EVENTS_URL_SMS"], "http://other:8123");
        }
    }
}

#[test]
fn models_render_a_gateway_client_factory_without_an_event_handler() {
    for native_router in [false, true] {
        let alone = render_module_with_layout(&["models".into()], native_router).unwrap();
        syn::parse_file(&alone).unwrap();
        assert!(
            alone.contains("use forklaunch_core::http::model_gateway::ModelGatewayClient;")
        );
        assert!(
            alone.contains("pub fn models_client() -> BlueprintResult<ModelGatewayClient>")
        );
        assert!(!alone.contains("platform_events"), "{alone}");
        assert!(!alone.contains("nonce_store"), "{alone}");
        assert!(alone.starts_with(BEGIN) && alone.ends_with(END));
        let beside =
            render_module_with_layout(&["models".into(), "email".into()], native_router)
                .unwrap();
        assert!(beside.contains("model_gateway::ModelGatewayClient"));
        assert!(beside.contains("pub fn models_client()"));
        // Models have no events: the dispatch table lists email only.
        assert!(beside.contains("\"email\" => email::handle(event).await"));
        assert!(!beside.contains("\"models\""));
        assert!(!beside.contains("pub mod models;"));
    }
    let inline = inline_capability_module(
        &render_module_with_layout(&["models".into(), "sms".into()], true).unwrap(),
    );
    syn::parse_file(&inline).unwrap();
}

#[test]
fn an_unknown_capability_is_an_error_not_a_panic() {
    let error = render_module_with_layout(&["telepathy".into()], true).unwrap_err();
    assert!(error.to_string().contains("telepathy"), "{error}");
}

#[test]
fn renaming_callbacks_preserves_other_services_and_custom_origins() {
    let mut mock: DockerService = serde_yml::from_str("image: node:22-alpine\nenvironment:\n  MOCK_EVENTS_URL_EMAIL: http://old:8123\n  MOCK_EVENTS_URL_SMS: https://old:8443/base\n  MOCK_EVENTS_URL_VOICE: https://old.example:8443\n  OTHER_URL: http://old:8123\n").unwrap();
    rename_callback_targets(&mut mock, "old", "new");
    let env = mock.environment.unwrap();
    assert_eq!(env["MOCK_EVENTS_URL_EMAIL"], "http://new:8123");
    assert_eq!(env["MOCK_EVENTS_URL_SMS"], "https://new:8443/base");
    assert_eq!(env["MOCK_EVENTS_URL_VOICE"], "https://old.example:8443");
    assert_eq!(env["OTHER_URL"], "http://old:8123");
}
