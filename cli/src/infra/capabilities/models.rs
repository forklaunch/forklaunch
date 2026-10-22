//! `forklaunch infra add <service> models`: platform-hosted AI models through
//! the managed gateway (the model gateway).
//!
//! The service gets a `ModelGatewayClient` registration built with
//! `createModelGatewayClient()` from the gateway settings: no provider key.
//! The platform enforces the model allowlist, the monthly token budget and the
//! rate limit, and offers a HIPAA product only BAA-covered models. There are
//! no model events, so no `/platform-events` route is written.
//!
//! A direct provider client the service already has (an `OpenAI` or
//! `Anthropic` registration, its key) is left exactly as it is: the gateway
//! client is added beside it, and the developer moves calls over.

use anyhow::Result;

use super::{Capability, CapabilityEdit, add_named_imports};
use crate::infra::in_place;

pub(crate) static MODELS: Capability = Capability {
    id: "models",
    resource_type: "models",
    summary: "AI models through the platform's model gateway: no provider key, the product's model allowlist and token budget",
    registration_key: "ModelGatewayClient",
    receives_events: false,
    workers: true,
    add,
    remove,
};

const HTTP_SOURCE: &str = "@forklaunch/core/http";
const HTTP_IMPORTS: &[&str] = &["createModelGatewayClient", "type ModelGatewayClient"];

/// Scoped, not singleton: `createModelGatewayClient` refuses to be created
/// without the gateway settings, and a singleton is built at boot, so a
/// service started without them (a unit test, a deploy that does not bind
/// models) would fail to start instead of failing where models are called.
const RUNTIME_BLOCK: &str = "const configInjector = createConfigInjector(SchemaValidator(), {
    ModelGatewayClient: {
        lifetime: Lifetime.Scoped,
        type: type<ModelGatewayClient>(),
        factory: ({ PLATFORM_GATEWAY_URL, INSTANCE_ID, INSTANCE_HMAC_KEY }) =>
            createModelGatewayClient({
                gatewayUrl: PLATFORM_GATEWAY_URL,
                instanceId: INSTANCE_ID,
                hmacKey: INSTANCE_HMAC_KEY
            })
    }
});";

fn add(edit: &mut CapabilityEdit) -> Result<()> {
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
    Ok(())
}

fn remove(edit: &mut CapabilityEdit) -> Result<()> {
    // Only the registration `infra add` wrote: a `ModelGatewayClient` the
    // developer built by hand (before `models` was a capability) stays.
    let path = edit.registrations_path();
    let Some(text) = edit.read(&path)? else {
        return Ok(());
    };
    if text.contains("ModelGatewayClient:")
        && !in_place::entry_matches_snippet(
            &text,
            "runtimeDependencies",
            "ModelGatewayClient",
            RUNTIME_BLOCK,
        )
    {
        eprintln!(
            "note: registrations.ts builds ModelGatewayClient its own way; kept (remove it by hand if nothing calls it)"
        );
        return Ok(());
    }
    edit.remove_registration(None, &[], &["ModelGatewayClient"])
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const REGISTRATIONS: &str = r#"import { SchemaValidator, number, string } from "@demo/core";
import { OpenTelemetryCollector } from "@forklaunch/core/http";
import { Lifetime, createConfigInjector, getEnvVar } from "@forklaunch/core/services";
const configInjector = createConfigInjector(SchemaValidator(), {
	SERVICE_METADATA: {
		lifetime: Lifetime.Singleton,
		type: { name: string, version: string },
		value: { name: "intake", version: "0.1.0" }
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
		factory: () => new OpenTelemetryCollector("intake")
	}
});
const serviceDependencies = runtimeDependencies.chain({});
"#;

    /// A service that already calls OpenAI and Anthropic with its own keys.
    const DIRECT_PROVIDERS: &str = r#"import Anthropic from '@anthropic-ai/sdk';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  Lifetime,
  createConfigInjector,
  getEnvVar
} from '@forklaunch/core/services';
import { number, optional, schemaValidator, string } from '@demo/core';
import OpenAI from 'openai';

const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: { name: string, version: string },
    value: { name: 'intake', version: '0.1.0' }
  }
});

const environmentConfig = configInjector.chain({
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  },
  OPENAI_API_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('OPENAI_API_KEY')
  },
  ANTHROPIC_API_KEY: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('ANTHROPIC_API_KEY')
  }
});

const runtimeDependencies = environmentConfig.chain({
  OpenAIClient: {
    lifetime: Lifetime.Singleton,
    type: OpenAI,
    factory: ({ OPENAI_API_KEY }) => new OpenAI({ apiKey: OPENAI_API_KEY })
  },
  AnthropicClient: {
    lifetime: Lifetime.Singleton,
    type: Anthropic,
    factory: ({ ANTHROPIC_API_KEY }) => new Anthropic({ apiKey: ANTHROPIC_API_KEY })
  }
});

const serviceDependencies = runtimeDependencies.chain({});
"#;

    const SERVER: &str = "import { forklaunchExpress, schemaValidator } from '@demo/core';\nimport { ci, tokens } from './bootstrapper';\nconst app = forklaunchExpress(schemaValidator, otel);\napp.use(intakeRouter);\napp.listen(1);";

    const PACKAGE_JSON: &str = "{\n  \"name\": \"@demo/intake\",\n  \"dependencies\": {\n    \"@anthropic-ai/sdk\": \"^0.60.0\",\n    \"@forklaunch/core\": \"~3.1.0\",\n    \"openai\": \"^5.0.0\"\n  }\n}\n";

    fn service(name: &str, registrations: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let service = dir.path().join(format!("src/modules/{name}"));
        fs::create_dir_all(&service).unwrap();
        fs::write(service.join("registrations.ts"), registrations).unwrap();
        fs::write(service.join("server.ts"), SERVER).unwrap();
        fs::write(service.join("package.json"), PACKAGE_JSON).unwrap();
        fs::write(
            service.join(".env.local"),
            "PORT=8003\nOPENAI_API_KEY=sk-local\n",
        )
        .unwrap();
        (dir, service)
    }

    fn edit(root: &std::path::Path, service: &std::path::Path, caps: &[&str]) -> CapabilityEdit {
        let caps: Vec<String> = caps.iter().map(|c| c.to_string()).collect();
        CapabilityEdit::new(root, "demo", "intake", service, caps.clone(), caps)
    }

    fn snapshot(root: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, String> {
        fn walk(
            dir: &std::path::Path,
            out: &mut std::collections::BTreeMap<std::path::PathBuf, String>,
        ) {
            for entry in fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.insert(path.clone(), fs::read_to_string(&path).unwrap());
                }
            }
        }
        let mut out = std::collections::BTreeMap::new();
        walk(root, &mut out);
        out
    }

    #[test]
    fn models_is_a_registered_worker_capability_without_events() {
        let c = super::super::find("models").expect("registered");
        assert_eq!(c.resource_type, "models");
        assert_eq!(c.registration_key, "ModelGatewayClient");
        assert!(c.workers);
        assert!(!c.receives_events);
        assert_eq!(super::super::resource_type("models"), "models");
    }

    #[test]
    fn add_registers_a_keyless_client_in_place_and_remove_takes_it_back() {
        let (dir, service) = service("intake", REGISTRATIONS);
        let before = snapshot(dir.path());
        let mut e = edit(dir.path(), &service, &["models"]);
        super::super::apply(&mut e, &MODELS, true).unwrap();
        e.commit().unwrap();

        let registrations = fs::read_to_string(service.join("registrations.ts")).unwrap();
        assert!(
            registrations.contains("\tModelGatewayClient: {\n\t\tlifetime: Lifetime.Scoped,"),
            "{registrations}"
        );
        assert!(
            registrations.contains("createModelGatewayClient({"),
            "{registrations}"
        );
        assert!(registrations.contains("import { type ModelGatewayClient, OpenTelemetryCollector, createModelGatewayClient } from \"@forklaunch/core/http\";"), "{registrations}");
        assert!(
            registrations.contains("SchemaValidator, number, string, type } from \"@demo/core\""),
            "{registrations}"
        );
        assert!(registrations.contains("INSTANCE_HMAC_KEY:"));
        assert!(!registrations.contains("API_KEY"), "no provider key");
        // No events: no route, no handler, server.ts untouched.
        assert!(!service.join("api/platformEvents").exists());
        assert!(!service.join("api/routes/platformEvents.routes.ts").exists());
        assert_eq!(
            fs::read_to_string(service.join("server.ts")).unwrap(),
            SERVER
        );

        let mut e = edit(dir.path(), &service, &[]);
        super::super::apply(&mut e, &MODELS, false).unwrap();
        e.commit().unwrap();
        assert_eq!(snapshot(dir.path()), before, "byte for byte what it was");
    }

    #[test]
    fn a_direct_openai_or_anthropic_client_is_left_as_it_is() {
        let (dir, service) = service("direct", DIRECT_PROVIDERS);
        let before = snapshot(dir.path());
        let mut e = edit(dir.path(), &service, &["models"]);
        super::super::apply(&mut e, &MODELS, true).unwrap();
        e.commit().unwrap();

        let registrations = fs::read_to_string(service.join("registrations.ts")).unwrap();
        // The provider clients, their keys and imports are all still there.
        for kept in [
            "import Anthropic from '@anthropic-ai/sdk';",
            "import OpenAI from 'openai';",
            "  OPENAI_API_KEY: {\n    lifetime: Lifetime.Singleton,\n    type: string,\n    value: getEnvVar('OPENAI_API_KEY')\n  },",
            "  OpenAIClient: {\n    lifetime: Lifetime.Singleton,\n    type: OpenAI,\n    factory: ({ OPENAI_API_KEY }) => new OpenAI({ apiKey: OPENAI_API_KEY })\n  },",
            "factory: ({ ANTHROPIC_API_KEY }) => new Anthropic({ apiKey: ANTHROPIC_API_KEY })",
        ] {
            assert!(
                registrations.contains(kept),
                "lost {kept:?}\n{registrations}"
            );
        }
        // The gateway client sits beside them, in the file's style.
        assert!(registrations.contains("  ModelGatewayClient: {\n    lifetime: Lifetime.Scoped,\n    type: type<ModelGatewayClient>(),"), "{registrations}");
        assert!(registrations.contains("from '@forklaunch/core/http';"));
        assert!(
            !registrations.contains('"'),
            "switched quote style\n{registrations}"
        );
        // Dependencies and the local key are the developer's.
        assert_eq!(
            fs::read_to_string(service.join("package.json")).unwrap(),
            PACKAGE_JSON
        );
        assert!(
            fs::read_to_string(service.join(".env.local"))
                .unwrap()
                .starts_with("PORT=8003\nOPENAI_API_KEY=sk-local\n")
        );

        let mut e = edit(dir.path(), &service, &[]);
        super::super::apply(&mut e, &MODELS, false).unwrap();
        e.commit().unwrap();
        assert_eq!(snapshot(dir.path()), before);
    }

    #[test]
    fn a_hand_built_model_gateway_client_is_kept_on_add_and_remove() {
        let own = REGISTRATIONS.replace(
            "const serviceDependencies",
            "const extra = runtimeDependencies.chain({});\nconst serviceDependencies",
        ).replace(
            "\t\tfactory: () => new OpenTelemetryCollector(\"intake\")\n\t}\n",
            "\t\tfactory: () => new OpenTelemetryCollector(\"intake\")\n\t},\n\tModelGatewayClient: {\n\t\tlifetime: Lifetime.Singleton,\n\t\ttype: OpenTelemetryCollector,\n\t\tfactory: () => createModelGatewayClient()\n\t}\n",
        );
        let (dir, service) = service("own", &own);
        let mut e = edit(dir.path(), &service, &["models"]);
        super::super::apply(&mut e, &MODELS, true).unwrap();
        e.commit().unwrap();
        let added = fs::read_to_string(service.join("registrations.ts")).unwrap();
        assert_eq!(added.matches("ModelGatewayClient: {").count(), 1, "{added}");
        assert!(added.contains("factory: () => createModelGatewayClient()"));

        let mut e = edit(dir.path(), &service, &[]);
        super::super::apply(&mut e, &MODELS, false).unwrap();
        e.commit().unwrap();
        let removed = fs::read_to_string(service.join("registrations.ts")).unwrap();
        assert!(
            removed.contains("factory: () => createModelGatewayClient()"),
            "{removed}"
        );
    }

    #[test]
    fn a_worker_gets_the_gateway_settings_and_no_event_route() {
        let (dir, service) = service("jobs", REGISTRATIONS);
        let root = dir.path();
        fs::write(
            root.join("docker-compose.yaml"),
            "volumes: {}\nnetworks:\n  demo-network:\n    name: demo-network\nservices:\n  jobs-server:\n    image: node:22\n    networks: [demo-network]\n    environment:\n      PORT: '8001'\n  jobs-worker:\n    image: node:22\n    networks: [demo-network]\n",
        )
        .unwrap();
        let before = snapshot(root);
        let caps = vec!["models".to_string()];
        let mut e =
            CapabilityEdit::new(root, "demo", "jobs", &service, caps.clone(), caps).for_worker();
        super::super::apply(&mut e, &MODELS, true).unwrap();
        e.commit().unwrap();

        let parsed: crate::core::docker::DockerCompose =
            serde_yml::from_str(&fs::read_to_string(root.join("docker-compose.yaml")).unwrap())
                .unwrap();
        for key in ["jobs-server", "jobs-worker"] {
            let env = parsed.services[key].environment.as_ref().unwrap();
            assert_eq!(
                env["PLATFORM_GATEWAY_URL"], "http://gateway-mock:8080",
                "{key}"
            );
        }
        let mock = &parsed.services[super::super::GATEWAY_MOCK_SERVICE];
        // The compose mock is the all-features gateway mock, which serves
        // the model routes (/models, /models/chat/completions[/stream]).
        let command = format!("{:?}", mock.command);
        assert!(command.contains("forklaunch-gateway-mock"), "{command}");
        assert!(
            !mock
                .environment
                .as_ref()
                .unwrap()
                .contains_key("MOCK_EVENTS_URL_MODELS")
        );
        assert!(!service.join("api/platformEvents").exists());

        let mut e =
            CapabilityEdit::new(root, "demo", "jobs", &service, vec![], vec![]).for_worker();
        super::super::apply(&mut e, &MODELS, false).unwrap();
        e.commit().unwrap();
        let after = snapshot(root);
        assert_eq!(after.len(), before.len());
        assert_eq!(after[&service.join("registrations.ts")], REGISTRATIONS);
        let parsed: crate::core::docker::DockerCompose =
            serde_yml::from_str(&after[&root.join("docker-compose.yaml")]).unwrap();
        assert!(
            !parsed
                .services
                .contains_key(super::super::GATEWAY_MOCK_SERVICE)
        );
    }
}
