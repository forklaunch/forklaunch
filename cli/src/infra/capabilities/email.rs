//! `forklaunch infra add <service> email`: platform-held email (Amazon SES,
//! sent by the platform from the instance's own sending identity).
//!
//! The service gets an `EmailClient` registration built with
//! `createEmailClient()` from the managed env contract (no mail credential),
//! and a handler for the platform's `email.delivered`, `email.bounced` and
//! `email.complained` events at `api/platformEvents/email.ts`.

use anyhow::Result;
use regex::Regex;

use super::{Capability, CapabilityEdit};

pub(crate) static EMAIL: Capability = Capability {
    id: "email",
    resource_type: "email",
    summary: "send email through the platform (Amazon SES), with delivery, bounce and complaint events",
    registration_key: "EmailClient",
    receives_events: true,
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
    // The registration first (an empty import line skips the helper's import
    // splice, which would replace the service's existing imports from these
    // sources), then the named imports merged into what is there.
    edit.inject_registration("", HTTP_SOURCE, None, Some(RUNTIME_BLOCK))?;
    let path = edit.registrations_path();
    if let Some(text) = edit.read(&path)? {
        let core_source = format!("@{}/core", edit.app_name);
        let text = add_named_imports(&text, HTTP_SOURCE, HTTP_IMPORTS);
        let text = add_named_imports(&text, &core_source, &["type"]);
        edit.write(path, text);
    }
    edit.ensure_platform_events("email", EVENTS_HANDLER)?;
    Ok(())
}

fn remove(edit: &mut CapabilityEdit) -> Result<()> {
    edit.remove_registration(None, &[], &["EmailClient"])?;
    let path = edit.registrations_path();
    if let Some(text) = edit.read(&path)? {
        let mut text = remove_named_imports(&text, HTTP_SOURCE, HTTP_IMPORTS);
        // `type` stays when something else still uses it.
        if !text.contains("type<") {
            text = remove_named_imports(&text, &format!("@{}/core", edit.app_name), &["type"]);
        }
        edit.write(path, text);
    }
    Ok(())
}

/// The named imports of a one-line `import { a, type B } from '…';`.
pub(crate) fn import_names(import_line: &str) -> Vec<String> {
    let Some(open) = import_line.find('{') else {
        return Vec::new();
    };
    let Some(close) = import_line[open..].find('}') else {
        return Vec::new();
    };
    specifiers(&import_line[open + 1..open + close])
}

/// Whether the file has a named import from `source`.
pub(crate) fn imports_from(text: &str, source: &str) -> bool {
    import_pattern(source).is_match(text)
}

fn import_pattern(source: &str) -> Regex {
    Regex::new(&format!(
        r#"import\s*\{{([^}}]*)\}}\s*from\s*['"]{}['"]\s*;?"#,
        regex::escape(source)
    ))
    .expect("import pattern")
}

fn specifiers(list: &str) -> Vec<String> {
    list.split(',')
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
        .collect()
}

fn render_import(names: &[String], source: &str) -> String {
    format!("import {{ {} }} from \"{}\";", names.join(", "), source)
}

/// Merge named imports into the file's import from `source` (adding one after
/// the last import when there is none).
pub(crate) fn add_named_imports(text: &str, source: &str, names: &[&str]) -> String {
    let pattern = import_pattern(source);
    if let Some(found) = pattern.captures(text) {
        let mut existing = specifiers(&found[1]);
        for name in names {
            let bare = name.trim_start_matches("type ");
            let present = existing
                .iter()
                .any(|e| e == name || e.trim_start_matches("type ") == bare);
            if !present {
                existing.push(name.to_string());
            }
        }
        let range = found.get(0).unwrap().range();
        return format!(
            "{}{}{}",
            &text[..range.start],
            render_import(&existing, source),
            &text[range.end..]
        );
    }
    let line = render_import(&names.iter().map(|n| n.to_string()).collect::<Vec<_>>(), source);
    // After the last top-level import statement (single or multi-line).
    let import_end = Regex::new(r#"(?m)^import[^;]*;[^\n]*\n"#).expect("import end");
    match import_end.find_iter(text).last() {
        Some(m) => format!("{}{}\n{}", &text[..m.end()], line, &text[m.end()..]),
        None => format!("{line}\n{text}"),
    }
}

/// Drop named imports from the file's import from `source`, and the import
/// itself when nothing is left.
pub(crate) fn remove_named_imports(text: &str, source: &str, names: &[&str]) -> String {
    let pattern = import_pattern(source);
    let Some(found) = pattern.captures(text) else {
        return text.to_string();
    };
    let kept: Vec<String> = specifiers(&found[1])
        .into_iter()
        .filter(|e| {
            !names
                .iter()
                .any(|n| e == n || e.trim_start_matches("type ") == n.trim_start_matches("type "))
        })
        .collect();
    let range = found.get(0).unwrap().range();
    if kept.is_empty() {
        let end = if text[range.end..].starts_with('\n') {
            range.end + 1
        } else {
            range.end
        };
        format!("{}{}", &text[..range.start], &text[end..])
    } else {
        format!(
            "{}{}{}",
            &text[..range.start],
            render_import(&kept, source),
            &text[range.end..]
        )
    }
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
        assert!(registrations.contains("OpenTelemetryCollector, createEmailClient, type EmailClient"), "{registrations}");
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
    fn named_imports_merge_and_unmerge() {
        let text = "import { a } from '@x/y';\nimport {\n  b,\n  c\n} from \"@forklaunch/core/http\";\nconst z = 1;\n";
        let added = add_named_imports(text, HTTP_SOURCE, HTTP_IMPORTS);
        assert!(added.contains("import { b, c, createEmailClient, type EmailClient } from \"@forklaunch/core/http\";"));
        assert_eq!(add_named_imports(&added, HTTP_SOURCE, HTTP_IMPORTS), added, "idempotent");
        let removed = remove_named_imports(&added, HTTP_SOURCE, HTTP_IMPORTS);
        assert!(removed.contains("import { b, c } from \"@forklaunch/core/http\";"));

        let fresh = add_named_imports("import { a } from 'x';\nconst z = 1;\n", HTTP_SOURCE, &["createEmailClient"]);
        assert_eq!(
            fresh,
            "import { a } from 'x';\nimport { createEmailClient } from \"@forklaunch/core/http\";\nconst z = 1;\n"
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
        assert!(
            registrations.contains("import { createConfigInjector, getEnvVar, Lifetime } from \"@forklaunch/core/services\";"),
            "{registrations}"
        );
        assert!(registrations.contains("INSTANCE_HMAC_KEY:"));
        assert_eq!(import_names("import { getEnvVar, type X } from 'y';"), vec!["getEnvVar", "type X"]);
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
