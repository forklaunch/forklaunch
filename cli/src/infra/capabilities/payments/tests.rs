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
    assert!(added.contains(
        "import { OpenTelemetryCollector, createStripeClient } from '@forklaunch/core/http';"
    ));
    assert_eq!(
        ensure_named_import(&added, "createStripeClient", CORE_HTTP),
        added
    );
    let removed = in_place::remove_named_imports(&added, CORE_HTTP, &["createStripeClient"]);
    assert_eq!(removed, text);
    let fresh = ensure_named_import(
        "import x from 'y';\nconst a = 1;\n",
        "createStripeClient",
        CORE_HTTP,
    );
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
    assert_eq!(
        in_place::add_default_import(&added, "Stripe", "stripe"),
        added
    );
}

#[test]
fn keys_are_made_optional_and_back() {
    let optional = retype_keys(BILLING, "string", "optional(string)");
    assert_eq!(optional.matches("type: optional(string)").count(), 2);
    assert_eq!(
        retype_keys(&optional, "optional(string)", "string"),
        BILLING
    );
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
    assert!(text.contains(
        "import { OpenTelemetryCollector, createStripeClient } from '@forklaunch/core/http';"
    ));
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
    assert_eq!(
        std::fs::read_to_string(dir.join("billing/registrations.ts")).unwrap(),
        BILLING
    );
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
    std::fs::write(
        dir.join("billing/server.ts"),
        "const app = f();\napp.use(a);\n",
    )
    .unwrap();
    let mut edit = edit_for(&dir);
    add(&mut edit).unwrap();
    edit.commit().unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("billing/registrations.ts")).unwrap(),
        native
    );
    let mut edit = edit_for(&dir);
    remove(&mut edit).unwrap();
    edit.commit().unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("billing/registrations.ts")).unwrap(),
        native
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unknown_stripe_factory_is_refused_rather_than_guessed() {
    let dir = std::env::temp_dir().join("fl-payments-cap-unknown");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("billing")).unwrap();
    std::fs::write(
        dir.join("billing/registrations.ts"),
        BILLING.replace(
            "new Stripe(STRIPE_API_KEY)",
            "new Stripe(STRIPE_API_KEY, { apiVersion })",
        ),
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
