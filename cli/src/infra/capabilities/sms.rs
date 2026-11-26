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
    workers: true,
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
    // createSmsClient and SmsClient go with the registration, keeping the
    // other names the file imports from `@forklaunch/core/http`.
    edit.remove_registration(None, &[], &["SmsClient"])
}

#[cfg(test)]
mod tests;
