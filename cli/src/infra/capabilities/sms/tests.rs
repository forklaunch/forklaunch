use std::fs;

use super::*;

const REGISTRATIONS: &str =
    "import { number, optional, schemaValidator, SchemaValidator, string } from '@demo/core';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { createConfigInjector, getEnvVar, Lifetime } from '@forklaunch/core/services';

const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: { name: string, version: string },
    value: { name: 'billing', version: '0.1.0' }
  }
});

const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  }
});

const runtimeDependencies = environmentConfig.chain({
  OtelCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    factory: () => new OpenTelemetryCollector('billing')
  }
});

export const createDependencyContainer = () => runtimeDependencies;
";

const SERVER: &str = "import { forklaunchExpress, schemaValidator } from '@demo/core';
import { ci, tokens } from './bootstrapper';

const app = forklaunchExpress(schemaValidator, ci.resolve(tokens.OtelCollector));
app.use(billingRouter);
";

#[test]
fn remove_restores_server_ts_mounted_at_the_top_level() {
    let (_dir, mut edit) = service();
    (SMS.add)(&mut edit).unwrap();
    assert!(staged(&edit, "server.ts").contains("\napp.use(platformEventsRouter);\n"));
    (SMS.remove)(&mut edit).unwrap();
    edit.service_capabilities.clear();
    edit.remove_platform_events("sms").unwrap();
    assert_eq!(staged(&edit, "server.ts"), SERVER);
}

fn service() -> (tempfile::TempDir, CapabilityEdit) {
    let dir = tempfile::tempdir().unwrap();
    let service = dir.path().join("src/modules/billing");
    fs::create_dir_all(&service).unwrap();
    fs::write(service.join("registrations.ts"), REGISTRATIONS).unwrap();
    fs::write(service.join("server.ts"), SERVER).unwrap();
    fs::write(
        dir.path().join("docker-compose.yaml"),
        "services:\n  billing:\n    image: node:22\n    environment:\n      PORT: '8001'\n",
    )
    .unwrap();
    let edit = CapabilityEdit::new(
        dir.path(),
        "demo",
        "billing",
        &service,
        vec!["sms".to_string()],
        vec!["sms".to_string()],
    );
    (dir, edit)
}

fn staged(edit: &CapabilityEdit, rel: &str) -> String {
    edit.read(&edit.service_path.join(rel))
        .unwrap()
        .unwrap_or_default()
}

#[test]
fn add_registers_a_keyless_client_and_an_event_handler() {
    let (_dir, mut edit) = service();
    (SMS.add)(&mut edit).unwrap();
    edit.ensure_gateway_wiring("sms").unwrap();
    let registrations = staged(&edit, "registrations.ts");
    assert!(registrations.contains("SmsClient:"), "{registrations}");
    assert!(registrations.contains("createSmsClient("));
    assert!(registrations.contains("deferRefusal: true"));
    assert!(registrations.contains("INSTANCE_HMAC_KEY:"));
    // The existing imports survive the new ones (merged, not replaced).
    // The existing imports survive the new ones (merged, in the file's
    // quotes, at their sorted place).
    assert!(registrations.contains(
            "import { OpenTelemetryCollector, SmsClient, createSmsClient } from '@forklaunch/core/http';"
        ), "{registrations}");
    assert!(registrations.contains(
        "import { createConfigInjector, getEnvVar, Lifetime } from '@forklaunch/core/services';"
    ));
    for key in ["AWS_ACCESS_KEY_ID", "TWILIO", "PINPOINT"] {
        assert!(!registrations.contains(key), "{key} in registrations");
    }
    let handler = staged(&edit, "api/platformEvents/sms.ts");
    assert!(handler.contains("export async function handle"));
    assert!(handler.contains("'sms.opted_out'"));
    assert!(staged(&edit, "api/platformEvents/index.ts").contains("sms: smsEvents"));
    assert!(staged(&edit, "server.ts").contains("app.use(platformEventsRouter)"));
    assert!(staged(&edit, ".env.local").contains("INSTANCE_ID=local-instance"));
}

#[test]
fn remove_drops_the_client_and_keeps_other_imports() {
    let (_dir, mut edit) = service();
    (SMS.add)(&mut edit).unwrap();
    (SMS.remove)(&mut edit).unwrap();
    let registrations = staged(&edit, "registrations.ts");
    assert!(!registrations.contains("SmsClient"), "{registrations}");
    assert!(!registrations.contains("createSmsClient"));
    assert!(
        registrations.contains("import { OpenTelemetryCollector } from '@forklaunch/core/http';")
    );
    assert_eq!(registrations, REGISTRATIONS);
}
