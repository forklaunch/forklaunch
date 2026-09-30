//! `sms`: texts sent through the platform's AWS End User Messaging SMS
//! gateway. The service holds no AWS credential and no phone number; it
//! signs each send with its instance key, and the platform sends from the
//! product's origination pool under a per-instance configuration set, so
//! delivery receipts and replies come back as `sms.*` platform events.

use anyhow::Result;

use super::{Capability, CapabilityEdit};

pub(crate) static SMS: Capability = Capability {
    id: "sms",
    resource_type: "sms",
    summary: "text messages through the platform (AWS End User Messaging SMS), with delivery and reply events",
    registration_key: "SmsClient",
    receives_events: true,
    add,
    remove,
};

const IMPORT_SOURCE: &str = "@forklaunch/core/http";
const IMPORT_LINE: &str = "import { createSmsClient, SmsClient } from \"@forklaunch/core/http\";";

/// Keyless: built from the managed env contract. Outside managed mode (no
/// gateway settings) the client refuses on the first send instead of at boot.
const RUNTIME_BLOCK: &str = "const configInjector = createConfigInjector(SchemaValidator(), {
    SmsClient: {
        lifetime: Lifetime.Singleton,
        type: SmsClient,
        factory: ({ PLATFORM_GATEWAY_URL, INSTANCE_ID, INSTANCE_HMAC_KEY }) =>
            createSmsClient({
                gatewayUrl: PLATFORM_GATEWAY_URL,
                instanceId: INSTANCE_ID,
                hmacKey: INSTANCE_HMAC_KEY,
                deferRefusal: true
            })
    }
});";

pub(crate) const HANDLER_STUB: &str = "import type {
  PlatformEvent,
  SmsDeliveredData,
  SmsFailedData,
  SmsOptedOutData,
  SmsReceivedData
} from '@forklaunch/core/http';

/**
 * SMS events from the ForkLaunch platform (AWS End User Messaging SMS),
 * already verified as signed with this instance's key.
 *
 * Deliveries can repeat: dedupe on `event.id` before acting. Never log
 * `data.body` of a received text — it is whatever the person typed.
 */
export async function handle(event: PlatformEvent): Promise<void> {
  switch (event.type) {
    case 'sms.delivered': {
      const data = event.data as unknown as SmsDeliveredData;
      // The carrier confirmed delivery of data.messageId.
      void data;
      return;
    }
    case 'sms.failed': {
      const data = event.data as unknown as SmsFailedData;
      // data.messageId was not delivered (data.reason); fall back to email?
      void data;
      return;
    }
    case 'sms.received': {
      const data = event.data as unknown as SmsReceivedData;
      // A reply from data.from. STOP/HELP are handled by the platform and the
      // carrier; data.keyword says when the text was one.
      void data;
      return;
    }
    case 'sms.opted_out': {
      const data = event.data as unknown as SmsOptedOutData;
      // data.phone texted STOP: the platform now refuses sends to it (422).
      // Record it so the product stops offering texts to that person.
      void data;
      return;
    }
    default:
      return;
  }
}
";

fn add(edit: &mut CapabilityEdit) -> Result<()> {
    // The factory reads the gateway settings `infra add` wires after this.
    edit.inject_registration(IMPORT_LINE, IMPORT_SOURCE, None, Some(RUNTIME_BLOCK))?;
    edit.ensure_platform_events("sms", HANDLER_STUB)?;
    Ok(())
}

fn remove(edit: &mut CapabilityEdit) -> Result<()> {
    edit.remove_registration(None, &[], &["SmsClient"])?;
    remove_import_specifiers(edit, &["createSmsClient", "SmsClient"])?;
    Ok(())
}

/// Drop our specifiers from the `@forklaunch/core/http` import, keeping the
/// others the file imports from it (OpenTelemetryCollector, …).
fn remove_import_specifiers(edit: &mut CapabilityEdit, names: &[&str]) -> Result<()> {
    let path = edit.registrations_path();
    let Some(text) = edit.read(&path)? else {
        return Ok(());
    };
    let Some(updated) = strip_specifiers(&text, IMPORT_SOURCE, names) else {
        return Ok(());
    };
    edit.write(path, updated);
    Ok(())
}

/// The text with `names` removed from the import of `source` (the whole
/// import when nothing is left), or None when nothing changed.
pub(crate) fn strip_specifiers(text: &str, source: &str, names: &[&str]) -> Option<String> {
    let pattern = regex::Regex::new(&format!(
        r#"import\s*\{{([^}}]*)\}}\s*from\s*['"]{}['"];?[ \t]*\n?"#,
        regex::escape(source)
    ))
    .ok()?;
    let found = pattern.captures(text)?;
    let whole = found.get(0)?;
    let kept: Vec<&str> = found[1]
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !names.contains(s))
        .collect();
    let original: Vec<&str> = found[1]
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if kept.len() == original.len() {
        return None;
    }
    let replacement = if kept.is_empty() {
        String::new()
    } else {
        format!("import {{ {} }} from \"{}\";\n", kept.join(", "), source)
    };
    Some(format!(
        "{}{}{}",
        &text[..whole.start()],
        replacement,
        &text[whole.end()..]
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const REGISTRATIONS: &str = "import { number, optional, schemaValidator, SchemaValidator, string } from '@demo/core';
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
        edit.read(&edit.service_path.join(rel)).unwrap().unwrap_or_default()
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
        assert!(registrations.contains(
            "import { OpenTelemetryCollector, createSmsClient, SmsClient } from \"@forklaunch/core/http\";"
        ));
        assert!(registrations.contains(
            "import { createConfigInjector, getEnvVar, Lifetime } from \"@forklaunch/core/services\";"
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
        assert!(registrations.contains(
            "import { OpenTelemetryCollector } from \"@forklaunch/core/http\";"
        ));
    }

    #[test]
    fn strip_specifiers_keeps_the_rest_of_the_import() {
        let text = "import { OpenTelemetryCollector, createSmsClient, SmsClient } from \"@forklaunch/core/http\";\nconst x = 1;\n";
        let out = strip_specifiers(text, IMPORT_SOURCE, &["createSmsClient", "SmsClient"]).unwrap();
        assert_eq!(
            out,
            "import { OpenTelemetryCollector } from \"@forklaunch/core/http\";\nconst x = 1;\n"
        );
        let only = "import { createSmsClient, SmsClient } from '@forklaunch/core/http';\nconst x = 1;\n";
        assert_eq!(
            strip_specifiers(only, IMPORT_SOURCE, &["createSmsClient", "SmsClient"]).unwrap(),
            "const x = 1;\n"
        );
        assert!(strip_specifiers(text, IMPORT_SOURCE, &["Nope"]).is_none());
    }
}
