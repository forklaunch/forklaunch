use std::fs;

use super::*;

const REGISTRATIONS: &str = r#"import { number, optional, SchemaValidator, string } from "@demo/core";
import { OpenTelemetryCollector } from "@forklaunch/core/http";
import { createConfigInjector, getEnvVar, Lifetime } from "@forklaunch/core/services";

const configInjector = createConfigInjector(SchemaValidator(), {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: { name: string, version: string },
    value: { name: "billing", version: "0.1.0" }
  }
});

const environmentConfig = configInjector.chain({
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar("PORT"))
  }
});

const runtimeDependencies = environmentConfig.chain({
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    value: new OpenTelemetryCollector("billing")
  }
});
"#;

const SERVER: &str = "import { forklaunchExpress } from '@demo/core';\nimport { ci } from './bootstrapper';\n\nconst app = forklaunchExpress();\napp.use(billingRouter);\n";

fn service() -> (tempfile::TempDir, CapabilityEdit) {
    let dir = tempfile::tempdir().unwrap();
    let service = dir.path().join("src/modules/billing");
    fs::create_dir_all(&service).unwrap();
    fs::write(service.join("registrations.ts"), REGISTRATIONS).unwrap();
    fs::write(service.join("server.ts"), SERVER).unwrap();
    let edit = CapabilityEdit::new(
        dir.path(),
        "demo",
        "billing",
        &service,
        vec!["voice".to_string()],
        vec!["voice".to_string()],
    );
    (dir, edit)
}

#[test]
fn imports_merge_into_the_existing_source_and_come_back_out() {
    use super::super::{add_named_imports, remove_named_imports};
    let text = "import { OpenTelemetryCollector } from \"@forklaunch/core/http\";\nconst a = 1;\n";
    let added = add_named_imports(text, IMPORT_SOURCE, IMPORT_NAMES);
    assert!(added.contains(
            "import { OpenTelemetryCollector, VoiceClient, createVoiceClient } from \"@forklaunch/core/http\";"
        ));
    assert_eq!(
        add_named_imports(&added, IMPORT_SOURCE, IMPORT_NAMES),
        added
    );
    assert_eq!(
        remove_named_imports(&added, IMPORT_SOURCE, IMPORT_NAMES),
        text
    );
    // Only ours: the whole import goes.
    let alone = add_named_imports("const a = 1;\n", IMPORT_SOURCE, IMPORT_NAMES);
    assert_eq!(
        remove_named_imports(&alone, IMPORT_SOURCE, IMPORT_NAMES),
        "const a = 1;\n"
    );
}

#[test]
fn add_registers_the_client_keyless_and_writes_the_event_handler() {
    let (_dir, mut edit) = service();
    add(&mut edit).unwrap();
    let registrations = edit.read(&edit.registrations_path()).unwrap().unwrap();
    assert!(registrations.contains("VoiceClient:"));
    assert!(registrations.contains("createVoiceClient("));
    assert!(registrations.contains("import { OpenTelemetryCollector, VoiceClient, createVoiceClient } from \"@forklaunch/core/http\";"));
    assert!(registrations.contains("hmacKey: INSTANCE_HMAC_KEY"));
    for forbidden in [
        "AWS_ACCESS_KEY_ID",
        "CONNECT_INSTANCE_ID",
        "client-connect",
        "TWILIO",
    ] {
        assert!(
            !registrations.contains(forbidden),
            "{forbidden} in registrations"
        );
    }
    let handler = edit
        .read(&edit.service_path.join("api/platformEvents/voice.ts"))
        .unwrap()
        .unwrap();
    assert!(handler.contains("export async function handle(event: PlatformEvent)"));
    assert!(handler.contains("voice.call.ended"));
    let index = edit
        .read(&edit.service_path.join("api/platformEvents/index.ts"))
        .unwrap()
        .unwrap();
    assert!(index.contains("voice: voiceEvents"));
    let server = edit
        .read(&edit.service_path.join("server.ts"))
        .unwrap()
        .unwrap();
    assert!(server.contains("platformEventsRouter"));
}

#[test]
fn gateway_wiring_merges_into_the_services_import_instead_of_replacing_it() {
    let (_dir, mut edit) = service();
    add(&mut edit).unwrap();
    edit.ensure_gateway_wiring("voice").unwrap();
    let registrations = edit.read(&edit.registrations_path()).unwrap().unwrap();
    assert!(registrations.contains(
        "import { createConfigInjector, getEnvVar, Lifetime } from \"@forklaunch/core/services\";"
    ));
    assert!(registrations.contains("INSTANCE_HMAC_KEY:"));
}

#[test]
fn remove_takes_the_registration_and_import_back_out() {
    let (_dir, mut edit) = service();
    add(&mut edit).unwrap();
    // As `infra remove` sees it: voice is no longer among the capabilities.
    edit.service_capabilities.clear();
    edit.app_capabilities.clear();
    remove(&mut edit).unwrap();
    edit.remove_platform_events("voice").unwrap();
    let server = edit
        .read(&edit.service_path.join("server.ts"))
        .unwrap()
        .unwrap();
    assert_eq!(server, SERVER, "server.ts back to what it was");
    let registrations = edit.read(&edit.registrations_path()).unwrap().unwrap();
    assert!(!registrations.contains("VoiceClient"));
    assert!(!registrations.contains("createVoiceClient"));
    assert!(registrations.contains("OpenTelemetryCollector"));
    assert_eq!(
        registrations, REGISTRATIONS,
        "registrations.ts back to what it was"
    );
}

#[test]
fn voice_is_registered_as_a_capability() {
    let found = super::super::find("voice").expect("voice registered");
    assert_eq!(found.resource_type, "voice");
    assert!(found.receives_events);
    assert_eq!(found.registration_key, "VoiceClient");
}
