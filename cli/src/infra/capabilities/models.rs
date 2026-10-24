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
mod tests;
