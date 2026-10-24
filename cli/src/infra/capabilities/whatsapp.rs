//! `whatsapp`: WhatsApp messages through the platform (AWS End User Messaging
//! Social). The service gets a `WhatsAppClient` built with
//! `createWhatsAppClient()` from the gateway settings, and a handler for the
//! `whatsapp.received` / `whatsapp.status` events the platform relays. No Meta
//! token or AWS credential reaches the service.

use anyhow::{Context, Result};

use super::{Capability, CapabilityEdit, add_named_imports};

pub(crate) static WHATSAPP: Capability = Capability {
    id: "whatsapp",
    resource_type: "whatsapp",
    summary: "WhatsApp template and session messages through the platform (no Meta token in the app)",
    registration_key: "WhatsAppClient",
    receives_events: true,
    workers: true,
    add,
    remove,
};

const FRAMEWORK_HTTP: &str = "@forklaunch/core/http";
const CLIENT_FACTORY: &str = "createWhatsAppClient";
const SERVICES: &str = "@forklaunch/core/services";

/// The registration. `type<ReturnType<…>>()` keeps it to one import; the
/// gateway settings are the ones `ensure_gateway_wiring` declares.
const RUNTIME_BLOCK: &str = "const configInjector = createConfigInjector(SchemaValidator(), {
    WhatsAppClient: {
        lifetime: Lifetime.Singleton,
        type: type<ReturnType<typeof createWhatsAppClient>>(),
        factory: ({ PLATFORM_GATEWAY_URL, INSTANCE_ID, INSTANCE_HMAC_KEY }) =>
            createWhatsAppClient({
                gatewayUrl: PLATFORM_GATEWAY_URL,
                instanceId: INSTANCE_ID,
                hmacKey: INSTANCE_HMAC_KEY
            })
    }
});";

pub(crate) const HANDLER_STUB: &str = "import type {
  PlatformEvent,
  WhatsAppReceivedEvent,
  WhatsAppStatusEvent
} from '@forklaunch/core/http';

/**
 * WhatsApp events the platform relays to this service, already verified.
 *
 *   whatsapp.received  someone wrote to the business number. It opens the
 *                      24-hour window in which sendText is allowed for them.
 *   whatsapp.status    a sent message moved to sent / delivered / read / failed;
 *                      `messageId` is the one sendTemplate/sendText returned.
 *
 * Deliveries can repeat: dedupe on event.id. Throwing makes the platform
 * retry. Never log or store message text without classifying it: it is
 * whatever the person chose to write.
 */
export async function handle(event: PlatformEvent): Promise<void> {
  switch (event.type) {
    case 'whatsapp.received': {
      const message = event.data as unknown as WhatsAppReceivedEvent;
      // TODO: route the reply (message.from, message.text, message.type)
      void message;
      return;
    }
    case 'whatsapp.status': {
      const status = event.data as unknown as WhatsAppStatusEvent;
      // TODO: record delivery (status.messageId, status.status)
      void status;
      return;
    }
    default:
      return;
  }
}
";

fn core_package(edit: &CapabilityEdit) -> String {
    format!("@{}/core", edit.app_name)
}

fn add(edit: &mut CapabilityEdit) -> Result<()> {
    let core = core_package(edit);
    let path = edit.registrations_path();
    let before = edit
        .read(&path)?
        .with_context(|| format!("{path:?} not found"))?;
    // Merged into the file's imports from these sources, in its style.
    let mut text = add_named_imports(&before, FRAMEWORK_HTTP, &[CLIENT_FACTORY]);
    text = add_named_imports(&text, &core, &["type"]);
    text = add_named_imports(&text, SERVICES, &["getEnvVar"]);
    if text != before {
        edit.write(path, text);
    }
    // The gateway settings the factory reads go in with it
    // (`ensure_gateway_wiring` then finds them there).
    edit.inject_registration(
        "",
        FRAMEWORK_HTTP,
        Some(super::GATEWAY_ENV_BLOCK),
        Some(RUNTIME_BLOCK),
    )?;
    edit.ensure_platform_events("whatsapp", HANDLER_STUB)?;
    Ok(())
}

fn remove(edit: &mut CapabilityEdit) -> Result<()> {
    // createWhatsAppClient, and `type` unless something else still uses it,
    // go with the registration.
    edit.remove_registration(None, &[], &["WhatsAppClient"])
}

#[cfg(test)]
mod tests;
