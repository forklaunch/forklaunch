//! `payments`: Stripe Connect through the platform.
//!
//! The service keeps using the real Stripe SDK, built by `createStripeClient`
//! (`@forklaunch/core/http`): in managed mode its requests go to the instance
//! gateway, signed with the instance key, and the platform pins them to the
//! instance's own connected account; locally they go to the gateway mock.
//! Stripe's events arrive as platform events, handled in
//! `api/platformEvents/payments.ts`.
//!
//! A service that already registers a `StripeClient` (billing-stripe,
//! ecommerce-stripe) has its factory switched to `createStripeClient` and its
//! Stripe keys made optional; any other service gets a new `StripeClient`
//! registration and the `stripe` dependency.

use anyhow::{Result, bail};
use regex::Regex;

use super::{Capability, CapabilityEdit};
use crate::infra::in_place;

pub(crate) static PAYMENTS: Capability = Capability {
    id: "payments",
    resource_type: "payment",
    summary: "Stripe Connect through the platform: the instance's own connected account, no Stripe key in the app",
    registration_key: "StripeClient",
    receives_events: true,
    add,
    remove,
};

/// The version the blueprints use.
const STRIPE_VERSION: &str = "^22.6.2";
const CORE_HTTP: &str = "@forklaunch/core/http";
const KEYS: &[&str] = &["STRIPE_API_KEY", "STRIPE_SECRET_KEY", "STRIPE_WEBHOOK_SECRET"];

/// The registration a service without a Stripe client gets.
const RUNTIME_BLOCK: &str = "const configInjector = createConfigInjector(SchemaValidator(), {
    StripeClient: {
        lifetime: Lifetime.Singleton,
        type: Stripe,
        factory: () => createStripeClient({ Stripe })
    }
});";

/// The switched factory of an existing registration: the gateway in managed
/// mode (and locally, through the gateway mock), the key otherwise.
const SWITCHED_FACTORY: &str =
    "factory: ({ STRIPE_API_KEY }) => createStripeClient({ Stripe, apiKey: STRIPE_API_KEY })";

fn original_factory() -> Regex {
    Regex::new(
        r"factory:\s*\(\s*\{\s*STRIPE_API_KEY\s*\}\s*\)\s*=>\s*new\s+Stripe\(\s*STRIPE_API_KEY\s*\)",
    )
    .unwrap()
}

fn switched_factory() -> Regex {
    Regex::new(
        r"factory:\s*\(\s*\{\s*STRIPE_API_KEY\s*\}\s*\)\s*=>\s*createStripeClient\(\s*\{\s*Stripe,\s*apiKey:\s*STRIPE_API_KEY\s*\}\s*\)",
    )
    .unwrap()
}

fn injected_factory() -> Regex {
    Regex::new(r"factory:\s*\(\s*\)\s*=>\s*createStripeClient\(\s*\{\s*Stripe\s*\}\s*\)").unwrap()
}

/// `KEY: { … type: <from> …}` -> `<to>`, for each Stripe key.
fn retype_keys(text: &str, from: &str, to: &str) -> String {
    let mut out = text.to_string();
    for key in KEYS {
        let re = Regex::new(&format!(
            r"(\b{key}\s*:\s*\{{[^{{}}]*?\btype\s*:\s*){}(\s*[,\n}}])",
            regex::escape(from)
        ))
        .unwrap();
        out = re.replace(&out, format!("${{1}}{to}${{2}}")).into_owned();
    }
    out
}

/// Add `name` to the named imports from `source` (or add that import), in
/// the file's style.
pub(crate) fn ensure_named_import(text: &str, name: &str, source: &str) -> String {
    in_place::add_named_imports(text, source, &[name])
}

fn has_default_stripe_import(text: &str) -> bool {
    Regex::new(r#"import\s+(?:\{\s*default\s+as\s+Stripe\s*\}|Stripe)\s+from\s+['"]stripe['"]"#)
        .unwrap()
        .is_match(text)
}

/// The module that provides `string` to registrations.ts (the app's core).
fn schema_source(text: &str) -> Option<String> {
    let re = Regex::new(r#"(?s)import\s*\{([^}]*)\}\s*from\s*['"]([^'"]+)['"]"#).unwrap();
    re.captures_iter(text)
        .find(|c| c[1].split(',').any(|n| n.trim() == "string"))
        .map(|c| c[2].to_string())
}

fn add(edit: &mut CapabilityEdit) -> Result<()> {
    let path = edit.registrations_path();
    let Some(mut text) = edit.read(&path)? else {
        bail!("{path:?} not found");
    };

    let existing = text.contains("StripeClient:");
    // The billing-stripe blueprint's own registration already picks
    // createStripeClient in managed mode; it needs no switching (and `infra
    // remove` leaves it as it is).
    let native = existing && text.contains("isManagedInstance()") && text.contains("createStripeClient(");
    if native {
        // Already managed-ready.
    } else if existing {
        // billing-stripe / ecommerce-stripe: switch the factory, keep the key
        // for running outside managed mode, and make it optional.
        if switched_factory().is_match(&text) {
            // Already switched (an earlier add); nothing to do here.
        } else if original_factory().is_match(&text) {
            text = original_factory()
                .replace(&text, SWITCHED_FACTORY)
                .into_owned();
        } else {
            bail!(
                "{path:?} registers a StripeClient with a factory `infra add` does not recognise; \
                 build it with createStripeClient({{ Stripe, apiKey }}) from @forklaunch/core/http by hand"
            );
        }
        text = retype_keys(&text, "string", "optional(string)");
        if let Some(source) = schema_source(&text) {
            text = ensure_named_import(&text, "optional", &source);
        }
        text = ensure_named_import(&text, "createStripeClient", CORE_HTTP);
        edit.write(path.clone(), text);

        // The key-verified /webhook route keeps working outside managed mode;
        // with the secret optional, it refuses every delivery in managed mode
        // (where Stripe's events arrive as platform events instead).
        let controller = edit.service_path.join("api/controllers/webhook.controller.ts");
        if let Some(source) = edit.read(&controller)? {
            let resolved = "ci.resolve(tokens.STRIPE_WEBHOOK_SECRET);";
            if source.contains(resolved) {
                edit.write(
                    controller,
                    source.replace(resolved, "ci.resolve(tokens.STRIPE_WEBHOOK_SECRET) ?? '';"),
                );
            }
        }
    } else {
        if !has_default_stripe_import(&text) {
            text = in_place::add_default_import(&text, "Stripe", "stripe");
        }
        text = ensure_named_import(&text, "createStripeClient", CORE_HTTP);
        edit.write(path, text);
        // The imports are in place, so only the block is merged.
        edit.inject_registration("", "stripe", None, Some(RUNTIME_BLOCK))?;
        ensure_stripe_dependency(edit)?;
    }

    let stub = if existing && text_has_webhook_service(edit)? {
        BILLING_HANDLER_STUB
    } else {
        HANDLER_STUB
    };
    edit.ensure_platform_events("payments", stub)?;
    Ok(())
}

fn text_has_webhook_service(edit: &CapabilityEdit) -> Result<bool> {
    Ok(edit
        .read(&edit.registrations_path())?
        .is_some_and(|t| t.contains("WebhookService:") && t.contains("StripeWebhookService")))
}

/// The `stripe` dependency line, added in place (see `in_place`).
fn ensure_stripe_dependency(edit: &mut CapabilityEdit) -> Result<()> {
    let path = edit.service_path.join("package.json");
    let Some(text) = edit.read(&path)? else {
        return Ok(());
    };
    let changed = in_place::add_dependency(&text, "stripe", STRIPE_VERSION)?;
    if changed != text {
        edit.write(path, changed);
    }
    Ok(())
}

fn remove(edit: &mut CapabilityEdit) -> Result<()> {
    let path = edit.registrations_path();
    let Some(before) = edit.read(&path)? else {
        return Ok(());
    };
    if switched_factory().is_match(&before) {
        let mut text = switched_factory()
            .replace(&before, "factory: ({ STRIPE_API_KEY }) => new Stripe(STRIPE_API_KEY)")
            .into_owned();
        text = retype_keys(&text, "optional(string)", "string");
        // createStripeClient goes with the switched factory.
        text = in_place::drop_orphaned_imports(&before, &text, &["createStripeClient"]);
        edit.write(path, text);
        let controller = edit.service_path.join("api/controllers/webhook.controller.ts");
        if let Some(source) = edit.read(&controller)? {
            let switched = "ci.resolve(tokens.STRIPE_WEBHOOK_SECRET) ?? '';";
            if source.contains(switched) {
                edit.write(
                    controller,
                    source.replace(switched, "ci.resolve(tokens.STRIPE_WEBHOOK_SECRET);"),
                );
            }
        }
    } else if injected_factory().is_match(&before) {
        // createStripeClient, and `import Stripe` when nothing else in the
        // file uses it, go with the registration.
        edit.remove_registration(None, &[], &["StripeClient"])?;
        drop_stripe_dependency_if_unused(edit)?;
    }
    Ok(())
}

fn drop_stripe_dependency_if_unused(edit: &mut CapabilityEdit) -> Result<()> {
    fn uses_stripe(dir: &std::path::Path, edit: &CapabilityEdit) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if !["node_modules", "dist"].contains(&name.as_str()) && uses_stripe(&p, edit) {
                    return true;
                }
            } else if name.ends_with(".ts") {
                let text = edit.read(&p).ok().flatten().unwrap_or_default();
                if text.contains("from 'stripe'") || text.contains("from \"stripe\"") {
                    return true;
                }
            }
        }
        false
    }
    if uses_stripe(&edit.service_path.clone(), edit) {
        return Ok(());
    }
    let path = edit.service_path.join("package.json");
    let Some(text) = edit.read(&path)? else {
        return Ok(());
    };
    let changed = in_place::remove_dependency(&text, "stripe")?;
    if changed != text {
        edit.write(path, changed);
    }
    Ok(())
}

const HANDLER_STUB: &str = "import type { PlatformEvent } from '@forklaunch/core/http';

/**
 * Stripe events for this instance's connected account, relayed by the
 * ForkLaunch platform after it verified Stripe's signature (`forklaunch infra
 * add <service> payments`).
 *
 *   event.id    the Stripe event id (stable across redeliveries)
 *   event.type  the Stripe event type: checkout.session.completed,
 *               invoice.paid, customer.subscription.updated, account.updated, …
 *   event.data  the event's object (a Checkout Session, an Invoice, …)
 *
 * Deliveries can repeat: dedupe on event.id. The in-memory set below only
 * covers one process; record processed ids in the database for more.
 */
const processed = new Set<string>();

export async function handle(event: PlatformEvent): Promise<void> {
  if (processed.has(event.id)) return;
  switch (event.type) {
    case 'checkout.session.completed':
      // TODO: mark the order / appointment in event.data.metadata as paid.
      break;
    case 'account.updated':
      // TODO: onboarding progressed; payments.status() has the details.
      break;
    default:
      break;
  }
  processed.add(event.id);
}
";

/// For billing-stripe: hand the event to its StripeWebhookService, which
/// already knows what each Stripe event means for its records.
const BILLING_HANDLER_STUB: &str = "import type { PlatformEvent } from '@forklaunch/core/http';
import type Stripe from 'stripe';
import { ci, tokens } from '../../bootstrapper';

const webhookServiceFactory = ci.scopedResolver(tokens.WebhookService);

/**
 * Stripe events for this instance's connected account, relayed by the
 * ForkLaunch platform after it verified Stripe's signature (`forklaunch infra
 * add <service> payments`). They go to the same StripeWebhookService the
 * key-verified /webhook route uses outside managed mode.
 *
 *   event.id    the Stripe event id (stable across redeliveries)
 *   event.type  the Stripe event type
 *   event.data  the event's object
 *
 * Deliveries can repeat: dedupe on event.id. The in-memory set below only
 * covers one process; record processed ids in the database for more.
 */
const processed = new Set<string>();

export async function handle(event: PlatformEvent): Promise<void> {
  if (processed.has(event.id)) return;
  await webhookServiceFactory().handleWebhookEvent({
    id: event.id,
    object: 'event',
    type: event.type,
    created: Math.floor(new Date(event.occurredAt).getTime() / 1000),
    data: { object: event.data }
  } as unknown as Stripe.Event);
  // TODO: anything else this app records about a payment.
  processed.add(event.id);
}
";

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const BILLING: &str = "import {\n  number,\n  optional,\n  schemaValidator,\n  SchemaValidator,\n  string\n} from '@demo/core';\nimport { OpenTelemetryCollector } from '@forklaunch/core/http';\nimport { createConfigInjector, getEnvVar, Lifetime } from '@forklaunch/core/services';\nimport Stripe from 'stripe';\n\nconst configInjector = createConfigInjector(SchemaValidator(), {\n  STRIPE_API_KEY: {\n    lifetime: Lifetime.Singleton,\n    type: string,\n    value: getEnvVar('STRIPE_API_KEY')\n  },\n  STRIPE_WEBHOOK_SECRET: {\n    lifetime: Lifetime.Singleton,\n    type: string,\n    value: getEnvVar('STRIPE_WEBHOOK_SECRET')\n  }\n});\nexport const environmentConfig = configInjector.chain({});\nconst runtimeDependencies = environmentConfig.chain({\n  StripeClient: {\n    lifetime: Lifetime.Singleton,\n    type: Stripe,\n    factory: ({ STRIPE_API_KEY }) => new Stripe(STRIPE_API_KEY)\n  }\n});\n";

    fn edit_for(dir: &Path) -> CapabilityEdit {
        CapabilityEdit::new(
            dir,
            "demo",
            "billing",
            &dir.join("billing"),
            vec!["payments".to_string()],
            vec!["payments".to_string()],
        )
    }

    #[test]
    fn named_imports_are_merged_and_dropped_without_losing_others() {
        let text = "import { OpenTelemetryCollector } from '@forklaunch/core/http';\nconst a = 1;\n";
        let added = ensure_named_import(text, "createStripeClient", CORE_HTTP);
        assert!(added.contains("import { OpenTelemetryCollector, createStripeClient } from '@forklaunch/core/http';"));
        assert_eq!(ensure_named_import(&added, "createStripeClient", CORE_HTTP), added);
        let removed = in_place::remove_named_imports(&added, CORE_HTTP, &["createStripeClient"]);
        assert_eq!(removed, text);
        let fresh = ensure_named_import("import x from 'y';\nconst a = 1;\n", "createStripeClient", CORE_HTTP);
        assert_eq!(
            fresh,
            "import { createStripeClient } from '@forklaunch/core/http';\nimport x from 'y';\nconst a = 1;\n"
        );
        assert_eq!(
            in_place::remove_named_imports(&fresh, CORE_HTTP, &["createStripeClient"]),
            "import x from 'y';\nconst a = 1;\n"
        );
    }

    #[test]
    fn the_default_stripe_import_is_added_once_in_the_files_quotes() {
        let text = "import {\n  createConfigInjector,\n  getEnvVar\n} from \"@forklaunch/core/services\";\nimport { a } from \"./a\";\n";
        let added = in_place::add_default_import(text, "Stripe", "stripe");
        assert_eq!(
            added,
            "import {\n  createConfigInjector,\n  getEnvVar\n} from \"@forklaunch/core/services\";\nimport Stripe from \"stripe\";\nimport { a } from \"./a\";\n"
        );
        assert_eq!(in_place::add_default_import(&added, "Stripe", "stripe"), added);
    }

    #[test]
    fn keys_are_made_optional_and_back() {
        let optional = retype_keys(BILLING, "string", "optional(string)");
        assert_eq!(optional.matches("type: optional(string)").count(), 2);
        assert_eq!(retype_keys(&optional, "optional(string)", "string"), BILLING);
    }

    #[test]
    fn an_existing_stripe_client_is_switched_not_duplicated_and_restored() {
        let dir = std::env::temp_dir().join("fl-payments-cap-billing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("billing/api/controllers")).unwrap();
        std::fs::write(dir.join("billing/registrations.ts"), BILLING).unwrap();
        std::fs::write(
            dir.join("billing/server.ts"),
            "import x from 'y';\nconst app = f();\napp.use(a);\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("billing/api/controllers/webhook.controller.ts"),
            "const STRIPE_WEBHOOK_SECRET = ci.resolve(tokens.STRIPE_WEBHOOK_SECRET);\n",
        )
        .unwrap();

        let mut edit = edit_for(&dir);
        add(&mut edit).unwrap();
        edit.commit().unwrap();
        let text = std::fs::read_to_string(dir.join("billing/registrations.ts")).unwrap();
        assert_eq!(text.matches("StripeClient:").count(), 1);
        assert!(text.contains(SWITCHED_FACTORY));
        assert!(text.contains("import { OpenTelemetryCollector, createStripeClient } from '@forklaunch/core/http';"));
        assert_eq!(text.matches("type: optional(string)").count(), 2);
        let controller =
            std::fs::read_to_string(dir.join("billing/api/controllers/webhook.controller.ts")).unwrap();
        assert!(controller.contains("?? ''"));
        let handler =
            std::fs::read_to_string(dir.join("billing/api/platformEvents/payments.ts")).unwrap();
        assert!(handler.contains("export async function handle(event: PlatformEvent)"));
        assert!(handler.contains("processed.has(event.id)"));

        let mut edit = edit_for(&dir);
        edit.service_capabilities.clear();
        remove(&mut edit).unwrap();
        edit.commit().unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("billing/registrations.ts")).unwrap(), BILLING);
        assert!(
            std::fs::read_to_string(dir.join("billing/api/controllers/webhook.controller.ts"))
                .unwrap()
                .ends_with("tokens.STRIPE_WEBHOOK_SECRET);\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_blueprints_managed_ready_registration_is_left_alone() {
        let dir = std::env::temp_dir().join("fl-payments-cap-native");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("billing")).unwrap();
        let native = BILLING.replace(
            "factory: ({ STRIPE_API_KEY }) => new Stripe(STRIPE_API_KEY)",
            "factory: ({ STRIPE_API_KEY }) => { if (isManagedInstance()) return createStripeClient({ Stripe }); return new Stripe(STRIPE_API_KEY!); }",
        );
        std::fs::write(dir.join("billing/registrations.ts"), &native).unwrap();
        std::fs::write(dir.join("billing/server.ts"), "const app = f();\napp.use(a);\n").unwrap();
        let mut edit = edit_for(&dir);
        add(&mut edit).unwrap();
        edit.commit().unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("billing/registrations.ts")).unwrap(), native);
        let mut edit = edit_for(&dir);
        remove(&mut edit).unwrap();
        edit.commit().unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("billing/registrations.ts")).unwrap(), native);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_stripe_factory_is_refused_rather_than_guessed() {
        let dir = std::env::temp_dir().join("fl-payments-cap-unknown");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("billing")).unwrap();
        std::fs::write(
            dir.join("billing/registrations.ts"),
            BILLING.replace("new Stripe(STRIPE_API_KEY)", "new Stripe(STRIPE_API_KEY, { apiVersion })"),
        )
        .unwrap();
        let mut edit = edit_for(&dir);
        let err = add(&mut edit).unwrap_err().to_string();
        assert!(err.contains("createStripeClient"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_capability_is_registered() {
        let c = super::super::find("payments").expect("payments is in CAPABILITIES");
        assert_eq!(c.resource_type, "payment");
        assert_eq!(c.registration_key, "StripeClient");
        assert!(c.receives_events);
    }
}
