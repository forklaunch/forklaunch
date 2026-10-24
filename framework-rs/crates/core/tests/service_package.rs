#![cfg(feature = "build")]
use forklaunch_core::contract::{BodyContract, ContractRegistry, RouteContract, ServiceContract};
use std::{fs, process::Command};

#[test]
fn compiled_package_exposes_both_real_service_clients() {
    let Ok(compiler) = std::env::var("FORKLAUNCH_TYPESCRIPT_COMPILER") else {
        return;
    };
    let root = std::env::temp_dir().join(format!("sdk-root-typescript-{}", std::process::id()));
    for name in ["notes", "billing"] {
        let mut registry = ContractRegistry::for_service(ServiceContract::new(name, "1"));
        registry.register(
            RouteContract::get("Health", "Health", "/health")
                .with_response(200, Some(BodyContract::json("Health"))),
        );
        forklaunch_core::build::typescript(&registry, name, root.join(name)).unwrap();
        fs::write(
            root.join(name).join(".forklaunch-generated"),
            "forklaunch.sdk.http.v1\n",
        )
        .unwrap();
    }
    forklaunch_core::build::service_package::refresh(&root).unwrap();
    fs::write(root.join("consumer.ts"), "import {sdk, createForkLaunchSdk, NotesSdk, BillingSdk} from './index.js';\nconst n = sdk.notes; const b = createForkLaunchSdk().billing;\nconst nn = NotesSdk.createForkLaunchSdk().notes; const bb = BillingSdk.createForkLaunchSdk().billing;\nif (!n || !b || !nn || !bb || Object.keys(sdk).sort().join(',') !== 'billing,notes') throw Error('missing clients');\n").unwrap();
    // Install only dependencies emitted by the package generator. HTTP validators
    // now have real runtime dependencies; ambient node_modules must not mask them.
    let install = Command::new("npm")
        .current_dir(&root)
        .args(["install", "--ignore-scripts", "--no-audit", "--no-fund"])
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let result = Command::new(compiler)
        .current_dir(&root)
        .args([
            "--strict",
            "--target",
            "ES2022",
            "--module",
            "NodeNext",
            "--moduleResolution",
            "NodeNext",
            "--lib",
            "ES2022,DOM,DOM.Iterable",
            "--outDir",
            "dist",
            "consumer.ts",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let result = Command::new("node")
        .current_dir(&root)
        .arg("dist/consumer.js")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn websocket_package_clean_install_and_strict_build() {
    if std::env::var_os("FORKLAUNCH_SDK_PACKAGE_INSTALL_TEST").is_none() {
        return;
    }
    let root = std::env::temp_dir().join(format!("sdk-socket-package-{}", std::process::id()));
    fs::create_dir_all(root.join("notes")).unwrap();
    fs::write(
        root.join("notes/.forklaunch-generated"),
        "forklaunch.sdk.http.v1\n",
    )
    .unwrap();
    let mut registry = ContractRegistry::for_service(ServiceContract::new("notes", "1"));
    registry.register(
        RouteContract::get("Health", "Health", "/health")
            .with_response(200, Some(BodyContract::json("Health"))),
    );
    forklaunch_core::build::typescript(&registry, "notes", root.join("notes")).unwrap();
    forklaunch_core::build::service_package::refresh(&root).unwrap();
    fs::write(root.join("tsconfig.json"), r#"{"compilerOptions":{"strict":true,"skipLibCheck":false,"noEmit":true,"target":"ES2022","module":"NodeNext","moduleResolution":"NodeNext","lib":["ES2022","DOM","DOM.Iterable"],"types":[]},"include":["**/*.ts"]}"#).unwrap();
    let contract = forklaunch_core::ws::SocketContract::new("/presence")
        .with_client_message("presence", forklaunch_core::ws::SocketSchemaEntry::new(serde_json::json!({"type":"object","properties":{"noteId":{"type":"string"}},"required":["noteId"],"additionalProperties":false})))
        .with_server_message("presence", forklaunch_core::ws::SocketSchemaEntry::new(serde_json::json!({"type":"string"})));
    forklaunch_core::build::typescript_websocket(
        &contract,
        "PresenceClient",
        root.join("notes/presence.ts"),
        root.join("notes/socket-contract.json"),
    )
    .unwrap();
    // Repeated ordinary HTTP generation must preserve socket dependency declarations.
    forklaunch_core::build::service_package::refresh(&root).unwrap();
    for args in [
        vec!["install", "--ignore-scripts", "--no-frozen-lockfile"],
        vec!["build"],
    ] {
        let result = Command::new("pnpm")
            .current_dir(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "fixture {}: {}{}",
            root.display(),
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fs::remove_dir_all(root).unwrap();
}
