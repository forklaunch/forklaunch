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
mod tests;
