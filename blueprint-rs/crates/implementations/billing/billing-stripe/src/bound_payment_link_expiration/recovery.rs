//! Request-key-linked metadata evidence, not proof of which process sent a request.
use super::{
    BoundPaymentLinkExpirationOutcome as Outcome, BoundPaymentLinkExpirationProfile, invalid,
};
use crate::services::StripeBillingService;
use forklaunch_billing_base::services::postgres::{
    PaymentLinkExpirationRecovery, PostgresBillingStore, TrustedPaymentLinkExpirationPublisher,
};
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use uuid::Uuid;
fn event_matches(
    event: &Value,
    event_id: &str,
    provider: &Value,
    provider_id: &str,
    profile: &BoundPaymentLinkExpirationProfile,
    recovery: &PaymentLinkExpirationRecovery,
    version: &str,
) -> BlueprintResult<()> {
    let created = event["created"].as_i64().ok_or_else(invalid)?;
    let key = event["request"]["idempotency_key"]
        .as_str()
        .ok_or_else(invalid)?;
    let now = chrono::Utc::now().timestamp();
    // Narrow admission window is operator policy, not a Stripe causality or key-retention guarantee.
    if event["id"] != event_id
        || event["object"] != "event"
        || event["type"] != "payment_link.updated"
        || event["api_version"] != version
        || event["livemode"].as_bool() != Some(profile.creation.scope.livemode())
        || !event["context"].is_null()
        || (!event["account"].is_null() && event["account"] != profile.creation.scope.account_id())
        || !recovery.matches_event_request_key(key)
        || created < recovery.recorded_at_seconds().saturating_sub(5)
        || created > recovery.recorded_at_seconds().saturating_add(300)
        || created > now
    {
        return Err(invalid());
    }
    for value in [&event["data"]["object"], provider] {
        if value["id"] != provider_id
            || value["object"] != "payment_link"
            || value["livemode"].as_bool() != Some(profile.creation.scope.livemode())
            || value["metadata"]["status"] != "EXPIRED"
        {
            return Err(invalid());
        }
    }
    Ok(())
}
impl StripeBillingService<PostgresBillingStore> {
    /// Startup-only local direct-context/grant/publisher checks, with no provider I/O.
    /// Does not require permission to reserve or dispatch an expiration operation.
    /// # Errors
    /// Rejects managed/Connect, missing recovery grants or invalid publisher separation.
    pub async fn validate_bound_payment_link_expiration_recovery_configuration(
        &self,
        publisher: &TrustedPaymentLinkExpirationPublisher,
    ) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        self.base()
            .store()
            .validate_payment_link_expiration_recovery_access()
            .await?;
        publisher
            .validate_configuration_for(self.base().store())
            .await
    }

    /// Finishes local expiration after authenticated request-key-linked evidence.
    /// Only fixed GETs are made. This never retries POST, deactivates a URL, or proves
    /// payment/entitlement state or which process originated the matched request.
    /// Supported event rendering version must equal the sealed request version and
    /// event time must lie within reservation −5s..+5min and not be in the future.
    /// These are admission restrictions, not provider guarantees. Missing events,
    /// automatic updates or expired event retention remain manual/pending.
    /// Already-completed operations return their actor-owned historical status without
    /// inspecting a new event or making a claim about current provider state.
    /// # Errors
    /// Denies changed actor/profile/dependencies, missing key, mismatched evidence.
    /// Publication/COMMIT uncertainty remains inspectable and never permits redispatch.
    pub async fn recover_bound_payment_link_expiration(
        &self,
        publisher: &TrustedPaymentLinkExpirationPublisher,
        profile: &BoundPaymentLinkExpirationProfile,
        operation: Uuid,
        public_id: Uuid,
        event_id: &str,
    ) -> BlueprintResult<Outcome> {
        if operation.is_nil()
            || public_id.is_nil()
            || !event_id.starts_with("evt_")
            || event_id.len() <= 4
            || event_id.len() > 128
            || !event_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(invalid());
        }
        let version = self.client().direct_creation_version()?;
        publisher.validate_for(self.base().store()).await?;
        if let Some(done @ Outcome::Completed { .. }) = self
            .inspect_bound_payment_link_expiration(profile, operation, public_id)
            .await?
        {
            return Ok(done);
        }
        let (recovery, provider_id) = match self
            .prepare_expiration_recovery(profile, operation, public_id)
            .await
        {
            Ok(value) => value,
            Err(error) => {
                if let Some(done @ Outcome::Completed { .. }) = self
                    .inspect_bound_payment_link_expiration(profile, operation, public_id)
                    .await?
                {
                    return Ok(done);
                }
                return Err(error);
            }
        };
        let context = self
            .client()
            .verify_context(
                profile.creation.scope.account_id(),
                profile.creation.scope.livemode(),
            )
            .await?;
        let event = self
            .client()
            .retrieve_payment_link_expiration_event(&context, event_id)
            .await?;
        let current = self
            .client()
            .retrieve_payment_link_in_context(&context, &provider_id)
            .await?;
        event_matches(
            &event,
            event_id,
            &current,
            &provider_id,
            profile,
            &recovery,
            version,
        )?;
        // Bounded evidence is checked by publisher before cloning/sealing it. References
        // here avoid duplicating an attacker-sized body into an intermediate JSON envelope.
        let evidence = Value::Object(serde_json::Map::from_iter([
            ("event".into(), event),
            ("current".into(), current),
        ]));
        if publisher
            .complete_recovery(recovery, &evidence, || {
                context.ensure_current(self.client())
            })
            .await
            .is_err()
        {
            // An unreadable receipt after a failed COMMIT acknowledgement remains
            // inspectable uncertainty, never evidence of rollback or retry permission.
            return Ok(
                match self
                    .inspect_bound_payment_link_expiration(profile, operation, public_id)
                    .await
                {
                    Ok(Some(status)) => status,
                    _ => Outcome::Unresolved {
                        operation_id: operation,
                        public_id,
                    },
                },
            );
        }
        if context.ensure_current(self.client()).is_err() {
            return Ok(Outcome::RecordedEvidenceNoLongerCurrent {
                operation_id: operation,
                public_id,
            });
        }
        Ok(Outcome::Completed {
            operation_id: operation,
            public_id,
        })
    }
    async fn prepare_expiration_recovery(
        &self,
        profile: &BoundPaymentLinkExpirationProfile,
        operation: Uuid,
        public_id: Uuid,
    ) -> BlueprintResult<(PaymentLinkExpirationRecovery, String)> {
        let (snapshot, provider_id) = self
            .prepare_bound_created_payment_link(&profile.creation, public_id)
            .await?;
        let recovery = self
            .base()
            .store()
            .snapshot_payment_link_expiration_recovery(
                operation,
                snapshot,
                &json!(profile.creation.prices),
            )
            .await?;
        Ok((recovery, provider_id))
    }
}
