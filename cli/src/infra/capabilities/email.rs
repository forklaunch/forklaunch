//! `forklaunch infra add <service> email`: platform-held email (Amazon SES,
//! sent by the platform from the instance's own sending identity).
//!
//! The service gets an `EmailClient` registration built with
//! `createEmailClient()` from the managed env contract (no mail credential),
//! and a handler for the platform's `email.delivered`, `email.bounced` and
//! `email.complained` events at `api/platformEvents/email.ts`.

use anyhow::Result;

use super::{Capability, CapabilityEdit, add_named_imports};

pub(crate) static EMAIL: Capability = Capability {
    id: "email",
    resource_type: "email",
    summary: "send email through the platform (Amazon SES), with delivery, bounce and complaint events",
    registration_key: "EmailClient",
    receives_events: true,
    workers: true,
    add,
    remove,
};

const HTTP_SOURCE: &str = "@forklaunch/core/http";
const HTTP_IMPORTS: &[&str] = &["createEmailClient", "type EmailClient"];

/// Scoped, not singleton: singletons are built at boot, and a service started
/// without the managed contract (a unit test, a plain deploy) would fail to
/// start instead of failing where email is actually used.
const RUNTIME_BLOCK: &str = "const configInjector = createConfigInjector(SchemaValidator(), {
    EmailClient: {
        lifetime: Lifetime.Scoped,
        type: type<EmailClient>(),
        factory: ({ PLATFORM_GATEWAY_URL, INSTANCE_ID, INSTANCE_HMAC_KEY }) =>
            createEmailClient({
                gatewayUrl: PLATFORM_GATEWAY_URL,
                instanceId: INSTANCE_ID,
                hmacKey: INSTANCE_HMAC_KEY
            })
    }
});";

pub(crate) const EVENTS_HANDLER: &str = "import type { EmailEventData, PlatformEvent } from '@forklaunch/core/http';

/**
 * Email delivery events from the ForkLaunch platform (Amazon SES), verified
 * by the platform-events controller before they reach this handler.
 *
 *   email.delivered   the recipient's mail server accepted the message
 *   email.bounced     it was refused; `permanent` bounces are suppressed by
 *                     the platform (a later send to the address answers 422)
 *   email.complained  the recipient marked it as spam; also suppressed
 *
 * `data.messageId` is what EmailClient.send returned. Deliveries can repeat,
 * so dedupe on `event.id`.
 */
const seen = new Set<string>();

export async function handle(event: PlatformEvent): Promise<void> {
  // TODO: dedupe in the database when the service runs more than one replica.
  if (seen.has(event.id)) return;
  seen.add(event.id);
  if (seen.size > 10_000) seen.delete(seen.values().next().value as string);

  const data = event.data as unknown as EmailEventData;
  switch (event.type) {
    case 'email.delivered':
      break;
    case 'email.bounced':
    case 'email.complained':
      // TODO: mark data.recipients undeliverable (for example on the user
      // record) so the app stops sending to them and can ask for a new address.
      void data.recipients;
      break;
    default:
      break;
  }
}
";

fn add(edit: &mut CapabilityEdit) -> Result<()> {
    // The registration, then its names merged into the file's imports from
    // these sources (in the file's style; see `in_place`).
    edit.inject_registration("", HTTP_SOURCE, None, Some(RUNTIME_BLOCK))?;
    let path = edit.registrations_path();
    if let Some(before) = edit.read(&path)? {
        let core_source = format!("@{}/core", edit.app_name);
        let text = add_named_imports(&before, HTTP_SOURCE, HTTP_IMPORTS);
        let text = add_named_imports(&text, &core_source, &["type"]);
        if text != before {
            edit.write(path, text);
        }
    }
    edit.ensure_platform_events("email", EVENTS_HANDLER)?;
    Ok(())
}

fn remove(edit: &mut CapabilityEdit) -> Result<()> {
    // Its imports (createEmailClient, EmailClient, and `type` unless
    // something else still uses it) go with the registration.
    edit.remove_registration(None, &[], &["EmailClient"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const REGISTRATIONS: &str = r#"import { SchemaValidator, number, string } from "@demo/core";
import { OpenTelemetryCollector } from "@forklaunch/core/http";
import { Lifetime, createConfigInjector, getEnvVar } from "@forklaunch/core/services";
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
		factory: () => new OpenTelemetryCollector("billing")
	}
});
const serviceDependencies = runtimeDependencies.chain({});
"#;

    const SERVER: &str = "import { forklaunchExpress, schemaValidator } from '@demo/core';\nimport { ci, tokens } from './bootstrapper';\nconst app = forklaunchExpress(schemaValidator, otel);\napp.use(billingRouter);\napp.listen(1);";

    fn service(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("fl-cap-email-{name}"));
        let _ = fs::remove_dir_all(&root);
        let service = root.join("src/modules/billing");
        fs::create_dir_all(&service).unwrap();
        fs::write(service.join("registrations.ts"), REGISTRATIONS).unwrap();
        fs::write(service.join("server.ts"), SERVER).unwrap();
        (root, service)
    }

    fn edit(root: &std::path::Path, service: &std::path::Path, caps: &[&str]) -> CapabilityEdit {
        let caps: Vec<String> = caps.iter().map(|c| c.to_string()).collect();
        CapabilityEdit::new(root, "demo", "billing", service, caps.clone(), caps)
    }

    #[test]
    fn add_registers_a_keyless_client_and_an_events_handler() {
        let (root, service) = service("add");
        let mut e = edit(&root, &service, &["email"]);
        add(&mut e).unwrap();
        e.commit().unwrap();

        let registrations = fs::read_to_string(service.join("registrations.ts")).unwrap();
        assert!(registrations.contains("EmailClient:"), "{registrations}");
        assert!(registrations.contains("createEmailClient({"), "{registrations}");
        assert!(registrations.contains("Lifetime.Scoped"));
        // Merged into the file's imports at their sorted place, in its quotes.
        assert!(registrations.contains("import { type EmailClient, OpenTelemetryCollector, createEmailClient } from \"@forklaunch/core/http\";"), "{registrations}");
        assert!(registrations.contains("SchemaValidator, number, string, type } from \"@demo/core\""), "{registrations}");
        assert!(!registrations.contains("SES"), "no vendor SDK or key");
        assert!(!registrations.contains("API_KEY"));

        let handler = fs::read_to_string(service.join("api/platformEvents/email.ts")).unwrap();
        assert!(handler.contains("seen.has(event.id)"));
        assert!(handler.contains("TODO: mark data.recipients undeliverable"));
        let index = fs::read_to_string(service.join("api/platformEvents/index.ts")).unwrap();
        assert!(index.contains("email: emailEvents"));
        assert!(fs::read_to_string(service.join("server.ts")).unwrap().contains("platformEventsRouter"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_worker_gets_the_gateway_settings_in_both_its_containers() {
        let (root, service) = service("worker");
        fs::write(
            root.join("docker-compose.yaml"),
            "volumes: {}\nnetworks:\n  demo-network:\n    name: demo-network\nservices:\n  billing-server:\n    image: node:22\n    networks: [demo-network]\n    environment:\n      PORT: '8001'\n  billing-worker:\n    image: node:22\n    networks: [demo-network]\n",
        )
        .unwrap();
        let mut e = edit(&root, &service, &["email"]).for_worker();
        super::super::apply(&mut e, &EMAIL, true).unwrap();
        e.commit().unwrap();

        let compose = fs::read_to_string(root.join("docker-compose.yaml")).unwrap();
        let parsed: crate::core::docker::DockerCompose = serde_yml::from_str(&compose).unwrap();
        for key in ["billing-server", "billing-worker"] {
            let env = parsed.services[key].environment.as_ref().unwrap();
            assert!(env.contains_key("PLATFORM_GATEWAY_URL"), "{key}: {compose}");
            assert!(env.contains_key("INSTANCE_HMAC_KEY"), "{key}");
        }
        // Events go to the server, which mounts /platform-events.
        let mock = parsed.services[super::super::GATEWAY_MOCK_SERVICE].environment.as_ref().unwrap();
        assert_eq!(mock["MOCK_EVENTS_URL_EMAIL"], "http://billing-server:8001");
        assert!(fs::read_to_string(service.join("registrations.ts")).unwrap().contains("EmailClient:"));

        // And all of it comes off again.
        let mut e = edit(&root, &service, &[]).for_worker();
        super::super::apply(&mut e, &EMAIL, false).unwrap();
        e.commit().unwrap();
        let parsed: crate::core::docker::DockerCompose =
            serde_yml::from_str(&fs::read_to_string(root.join("docker-compose.yaml")).unwrap()).unwrap();
        assert!(!parsed.services.contains_key(super::super::GATEWAY_MOCK_SERVICE));
        let _ = fs::remove_dir_all(&root);
    }

    fn events_route(root: &std::path::Path) -> Option<String> {
        let parsed: crate::core::docker::DockerCompose =
            serde_yml::from_str(&fs::read_to_string(root.join("docker-compose.yaml")).unwrap())
                .unwrap();
        parsed
            .services
            .get(super::super::GATEWAY_MOCK_SERVICE)?
            .environment
            .as_ref()?
            .get("MOCK_EVENTS_URL_EMAIL")
            .cloned()
    }

    #[test]
    fn a_service_and_a_worker_with_email_both_keep_their_event_route() {
        let (root, billing) = service("service-and-worker");
        fs::write(
            root.join("docker-compose.yaml"),
            "volumes: {}\nnetworks:\n  demo-network:\n    name: demo-network\nservices:\n  billing:\n    image: node:22\n    networks: [demo-network]\n    environment:\n      PORT: '8001'\n  jobs-server:\n    image: node:22\n    networks: [demo-network]\n    environment:\n      PORT: '8002'\n  jobs-worker:\n    image: node:22\n    networks: [demo-network]\n",
        )
        .unwrap();
        let jobs = root.join("src/modules/jobs");
        fs::create_dir_all(&jobs).unwrap();
        fs::write(jobs.join("registrations.ts"), REGISTRATIONS).unwrap();
        fs::write(jobs.join("server.ts"), SERVER).unwrap();
        let caps = |c: &[&str]| c.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        // The service has email first.
        let mut e = edit(&root, &billing, &["email"]);
        super::super::apply(&mut e, &EMAIL, true).unwrap();
        e.commit().unwrap();
        assert_eq!(events_route(&root).as_deref(), Some("http://billing:8001"));

        // Then a worker gets it: the service's route stays, the worker's
        // server is added beside it.
        let mut e = CapabilityEdit::new(
            &root,
            "demo",
            "jobs",
            &jobs,
            caps(&["email"]),
            caps(&["email", "email"]),
        )
        .for_worker();
        super::super::apply(&mut e, &EMAIL, true).unwrap();
        e.commit().unwrap();
        assert_eq!(
            events_route(&root).as_deref(),
            Some("http://billing:8001,http://jobs-server:8002")
        );
        for module in [&billing, &jobs] {
            assert!(module.join("api/platformEvents/email.ts").exists());
            assert!(fs::read_to_string(module.join("server.ts"))
                .unwrap()
                .contains("platformEventsRouter"));
        }

        // Taking it off the worker leaves the service's route alone.
        let mut e = CapabilityEdit::new(&root, "demo", "jobs", &jobs, caps(&[]), caps(&["email"]))
            .for_worker();
        super::super::apply(&mut e, &EMAIL, false).unwrap();
        e.remove_platform_events("email").unwrap();
        e.commit().unwrap();
        assert_eq!(events_route(&root).as_deref(), Some("http://billing:8001"));
        assert!(billing.join("api/platformEvents/email.ts").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_worker_needs_both_its_containers_in_compose() {
        let (root, service) = service("worker-half");
        fs::write(
            root.join("docker-compose.yaml"),
            "volumes: {}\nnetworks:\n  demo-network:\n    name: demo-network\nservices:\n  billing-server:\n    image: node:22\n    networks: [demo-network]\n",
        )
        .unwrap();
        let mut e = edit(&root, &service, &["email"]).for_worker();
        let error = super::super::apply(&mut e, &EMAIL, true).unwrap_err().to_string();
        assert!(error.contains("'billing-worker'"), "{error}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn every_capability_may_be_added_to_a_worker() {
        assert!(EMAIL.workers);
        let on_workers: Vec<&str> = super::super::CAPABILITIES
            .iter()
            .filter(|c| c.workers)
            .map(|c| c.id)
            .collect();
        assert_eq!(on_workers, vec!["email", "sms", "whatsapp", "voice", "payments", "models"]);
    }

    #[test]
    fn remove_takes_back_the_registration_and_its_imports() {
        let (root, service) = service("remove");
        let mut e = edit(&root, &service, &["email"]);
        add(&mut e).unwrap();
        e.commit().unwrap();
        let mut e = edit(&root, &service, &[]);
        remove(&mut e).unwrap();
        e.remove_platform_events("email").unwrap();
        e.commit().unwrap();

        let registrations = fs::read_to_string(service.join("registrations.ts")).unwrap();
        assert!(!registrations.contains("EmailClient"), "{registrations}");
        // Byte for byte what it was.
        assert_eq!(registrations, REGISTRATIONS);
        assert!(!registrations.contains("createEmailClient"));
        assert!(registrations.contains("import { OpenTelemetryCollector } from \"@forklaunch/core/http\";"));
        // `type` goes with the last `type<…>()` that used it.
        assert!(registrations.contains("import { SchemaValidator, number, string } from \"@demo/core\";"), "{registrations}");
        assert!(!service.join("api/platformEvents/email.ts").exists());
        assert!(!service.join("api/routes/platformEvents.routes.ts").exists());
        // The unindented mount goes too, and nothing else in server.ts moves.
        assert_eq!(fs::read_to_string(service.join("server.ts")).unwrap().trim_end(), SERVER);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn named_imports_merge_and_unmerge_in_the_files_style() {
        use super::super::remove_named_imports;
        let text = "import { a } from '@x/y';\nimport {\n  b,\n  c\n} from \"@forklaunch/core/http\";\nconst z = 1;\n";
        let added = add_named_imports(text, HTTP_SOURCE, HTTP_IMPORTS);
        assert!(added.contains("import {\n  type EmailClient,\n  b,\n  c,\n  createEmailClient\n} from \"@forklaunch/core/http\";"), "{added}");
        assert_eq!(add_named_imports(&added, HTTP_SOURCE, HTTP_IMPORTS), added, "idempotent");
        assert_eq!(remove_named_imports(&added, HTTP_SOURCE, HTTP_IMPORTS), text);

        let fresh = add_named_imports("import { a } from 'x';\nconst z = 1;\n", HTTP_SOURCE, &["createEmailClient"]);
        assert_eq!(
            fresh,
            "import { createEmailClient } from '@forklaunch/core/http';\nimport { a } from 'x';\nconst z = 1;\n"
        );
        assert_eq!(
            remove_named_imports(&fresh, HTTP_SOURCE, &["createEmailClient"]),
            "import { a } from 'x';\nconst z = 1;\n"
        );
    }

    #[test]
    fn gateway_wiring_keeps_the_services_other_imports() {
        let (root, service) = service("wiring");
        let text = REGISTRATIONS.replace(
            "import { Lifetime, createConfigInjector, getEnvVar } from \"@forklaunch/core/services\";",
            "import {\n  createConfigInjector,\n  getEnvVar,\n  Lifetime\n} from '@forklaunch/core/services';",
        );
        fs::write(service.join("registrations.ts"), text).unwrap();
        let mut e = edit(&root, &service, &["email"]);
        add(&mut e).unwrap();
        e.ensure_gateway_wiring("email").unwrap();
        e.commit().unwrap();
        let registrations = fs::read_to_string(service.join("registrations.ts")).unwrap();
        // Left exactly as the file wrote it.
        assert!(
            registrations.contains("import {\n  createConfigInjector,\n  getEnvVar,\n  Lifetime\n} from '@forklaunch/core/services';"),
            "{registrations}"
        );
        assert!(registrations.contains("INSTANCE_HMAC_KEY:"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn email_is_a_registered_capability_that_receives_events() {
        let c = super::super::find("email").expect("registered");
        assert_eq!(c.resource_type, "email");
        assert_eq!(c.registration_key, "EmailClient");
        assert!(c.receives_events);
    }
}
