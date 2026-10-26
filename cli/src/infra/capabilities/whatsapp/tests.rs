use std::fs;

use super::*;

const REGISTRATIONS: &str =
    "import { number, optional, schemaValidator, SchemaValidator, string } from '@demo/core';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { createConfigInjector, getEnvVar, Lifetime } from '@forklaunch/core/services';

const configInjector = createConfigInjector(SchemaValidator(), {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: { name: string, version: string },
    value: { name: 'billing', version: '0.1.0' }
  }
});

const environmentConfig = configInjector.chain({
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  }
});

const runtimeDependencies = environmentConfig.chain({
  OtelCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    factory: () => new OpenTelemetryCollector('x', 'info')
  }
});

const serviceDependencies = runtimeDependencies.chain({});
";

const SERVER: &str = "import { forklaunchExpress } from '@demo/core';
import { ci, tokens } from './bootstrapper';

const app = forklaunchExpress(schemaValidator, openTelemetryCollector);
app.use(billingRouter);
";

fn scratch() -> (tempfile::TempDir, CapabilityEdit) {
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
        vec!["whatsapp".to_string()],
        vec!["whatsapp".to_string()],
    );
    (dir, edit)
}

#[test]
fn add_registers_a_keyless_client_and_an_event_handler() {
    let (dir, mut edit) = scratch();
    add(&mut edit).unwrap();
    edit.ensure_gateway_wiring("whatsapp").unwrap();
    edit.commit().unwrap();
    let service = dir.path().join("src/modules/billing");
    let registrations = fs::read_to_string(service.join("registrations.ts")).unwrap();
    assert!(registrations.contains("WhatsAppClient:"), "{registrations}");
    assert!(registrations.contains("createWhatsAppClient({"));
    assert!(registrations.contains("hmacKey: INSTANCE_HMAC_KEY"));
    // Both framework imports kept; the factory merged into the http one.
    assert!(registrations.contains("OpenTelemetryCollector"));
    assert!(
        regex::Regex::new(
            r#"import \{[^}]*createConfigInjector[^}]*\} from '@forklaunch/core/services'"#
        )
        .unwrap()
        .is_match(&registrations),
        "{registrations}"
    );
    assert!(registrations.contains("PLATFORM_GATEWAY_URL:"));
    assert!(
            regex::Regex::new(r#"import \{[^}]*OpenTelemetryCollector[^}]*createWhatsAppClient[^}]*\} from '@forklaunch/core/http'"#)
                .unwrap()
                .is_match(&registrations),
            "{registrations}"
        );
    assert!(
        regex::Regex::new(r#"\btype\b[^}]*\} from '@demo/core'"#)
            .unwrap()
            .is_match(&registrations)
    );
    // No vendor credential anywhere.
    assert!(!registrations.contains("WHATSAPP_TOKEN"));
    assert!(!registrations.contains("META_"));
    let handler = fs::read_to_string(service.join("api/platformEvents/whatsapp.ts")).unwrap();
    assert!(handler.contains("'whatsapp.received'"));
    assert!(handler.contains("'whatsapp.status'"));
    let index = fs::read_to_string(service.join("api/platformEvents/index.ts")).unwrap();
    assert!(index.contains("whatsapp: whatsappEvents"));
    let server = fs::read_to_string(service.join("server.ts")).unwrap();
    assert!(server.contains("app.use(platformEventsRouter);"));
}

#[test]
fn remove_undoes_the_registration_and_imports() {
    let (dir, mut edit) = scratch();
    add(&mut edit).unwrap();
    edit.commit().unwrap();
    let service = dir.path().join("src/modules/billing");
    let mut edit = CapabilityEdit::new(dir.path(), "demo", "billing", &service, vec![], vec![]);
    remove(&mut edit).unwrap();
    edit.commit().unwrap();
    let registrations = fs::read_to_string(service.join("registrations.ts")).unwrap();
    assert!(!registrations.contains("WhatsAppClient"), "{registrations}");
    assert!(!registrations.contains("createWhatsAppClient"));
    assert!(registrations.contains("OpenTelemetryCollector"));
    assert!(
        !regex::Regex::new(r"\btype,")
            .unwrap()
            .is_match(&registrations),
        "{registrations}"
    );
}

#[test]
fn removing_the_last_event_feature_unmounts_the_events_router() {
    let (dir, mut edit) = scratch();
    add(&mut edit).unwrap();
    edit.commit().unwrap();
    let service = dir.path().join("src/modules/billing");
    let mut edit = CapabilityEdit::new(dir.path(), "demo", "billing", &service, vec![], vec![]);
    remove(&mut edit).unwrap();
    edit.remove_platform_events("whatsapp").unwrap();
    edit.release_gateway_wiring("whatsapp").unwrap();
    edit.commit().unwrap();
    assert_eq!(
        fs::read_to_string(service.join("server.ts")).unwrap(),
        SERVER
    );
    // The registration, its imports and the gateway settings add wrote
    // are gone: byte for byte the file it was.
    assert_eq!(
        fs::read_to_string(service.join("registrations.ts")).unwrap(),
        REGISTRATIONS
    );
    assert!(!service.join("api/platformEvents/whatsapp.ts").exists());
    assert!(!service.join("api/routes/platformEvents.routes.ts").exists());
}

#[test]
fn type_import_is_kept_while_something_else_uses_it() {
    let text = "import { type, string } from '@demo/core';\nconst x = { t: type<unknown>() };\n";
    use crate::infra::in_place::binding_used as name_used_outside_imports;
    assert!(name_used_outside_imports(text, "type"));
    let text = "import {\n  type,\n  string\n} from '@demo/core';\nconst x = { t: string };\n";
    assert!(!name_used_outside_imports(text, "type"));
}

#[test]
fn registered_with_events_and_its_resource_type() {
    let capability = super::super::find("whatsapp").expect("registered");
    assert_eq!(capability.resource_type, "whatsapp");
    assert_eq!(capability.registration_key, "WhatsAppClient");
    assert!(capability.receives_events);
}
