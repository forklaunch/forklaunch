use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::platform_events::VerifiedPlatformEvent;

/// Receives an event after instance-key signature and replay verification.
/// Deliveries can repeat with a fresh nonce: dedupe event IDs before business
/// effects. Incoming bodies may contain private data; avoid logging them.
pub async fn handle(event: VerifiedPlatformEvent) -> BlueprintResult<()> {
    // Use event.into_event::<YourData>() for an application-specific DTO.
    let _ = event;
    Ok(())
}
