//! Actor-owned direct completed-creation enrichment, without mutation authority.
use crate::{bound_creation::BoundPaymentLinkCreationProfile, services::StripeBillingService};
use forklaunch_billing_base::services::{
    BillingKind,
    postgres::{PaymentLinkCreationReadSnapshot, PostgresBillingStore},
};
use forklaunch_billing_interface::services::PaymentLinkDto;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::json;
use uuid::Uuid;
const MAX_LIST_BYTES: usize = 8 * 1024 * 1024;
/// Trusted service-side selection bound, independent of caller request data.
#[derive(Clone, Copy)]
pub struct BoundPaymentLinkListLimit(usize);
impl Default for BoundPaymentLinkListLimit {
    fn default() -> Self {
        Self(100)
    }
}
impl BoundPaymentLinkListLimit {
    /// # Errors
    /// Rejects zero or more than 1000 selected records.
    pub fn trusted(limit: usize) -> BlueprintResult<Self> {
        if !(1..=1000).contains(&limit) {
            return Err(mismatch());
        }
        Ok(Self(limit))
    }
}
fn consume_bytes(remaining: &mut usize, bytes: usize) -> BlueprintResult<()> {
    *remaining = remaining.checked_sub(bytes).ok_or_else(mismatch)?;
    Ok(())
}
fn consume_result(remaining: &mut usize, dto: &PaymentLinkDto) -> BlueprintResult<()> {
    struct Count<'a>(&'a mut usize);
    impl std::io::Write for Count<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            *self.0 = self
                .0
                .checked_sub(bytes.len())
                .ok_or_else(|| std::io::Error::other("result exceeds limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Count(remaining), dto).map_err(|_| mismatch())
}
fn mismatch() -> BlueprintError {
    BlueprintError::new(
        "billing_bound_payment_link_read_mismatch",
        "Payment link creation evidence does not match",
    )
}
impl StripeBillingService<PostgresBillingStore> {
    /// Checks direct client configuration and local read/audit grants only.
    /// No provider request or publisher capability is used.
    /// # Errors
    /// Rejects managed transport and missing local privileges.
    pub async fn validate_bound_payment_link_read_configuration(&self) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        self.base()
            .store()
            .validate_payment_link_creation_read_access()
            .await
    }

    /// Reads actor-owned completed direct creation provenance, never a mutable
    /// cache reference or administrative alias fallback. The original trusted
    /// price allowlist and current exact local receipt/binding must still match.
    /// Only providerFields is refreshed from an authenticated exact id/kind/mode
    /// read; active, line items, amount and payment state imply no local lifecycle
    /// update or entitlement. No POST, cache update, claims or publisher is used.
    /// Evidence JSON is bounded (32 depth/8192 nodes/256 KiB worst-case encoding).
    /// Data linearizes at the last matching SELECT, with freshness checked after
    /// audit commit; later local/provider changes remain possible.
    /// # Errors
    /// Denies managed transport/provenance, foreign actor/scope, changed policy,
    /// journal/body/binding mismatch, oversized evidence and stale provider context.
    pub async fn get_bound_created_payment_link(
        &self,
        profile: &BoundPaymentLinkCreationProfile,
        public_id: Uuid,
    ) -> BlueprintResult<PaymentLinkDto> {
        let (snapshot, provider_id) = self
            .prepare_bound_created_payment_link(profile, public_id)
            .await?;
        let context = self
            .client()
            .verify_context(profile.scope.account_id(), profile.scope.livemode())
            .await?;
        let provider = self
            .client()
            .retrieve_payment_link_in_context(&context, &provider_id)
            .await?;
        let record = self
            .base()
            .store()
            .read_payment_link_creation_snapshot(&snapshot)
            .await?;
        context.ensure_current(self.client())?;
        let mut record: PaymentLinkDto = serde_json::from_value(record).map_err(|_| mismatch())?;
        record.provider_fields = Some(provider);
        Ok(record)
    }
    pub(crate) async fn prepare_bound_created_payment_link(
        &self,
        profile: &BoundPaymentLinkCreationProfile,
        public_id: Uuid,
    ) -> BlueprintResult<(PaymentLinkCreationReadSnapshot, String)> {
        let version = self.client().direct_creation_version()?;
        let store = self.base().store();
        let binding = store
            .stripe_binding_by_public_id(&profile.scope, BillingKind::PaymentLink, public_id)
            .await?
            .ok_or_else(mismatch)?;
        let snapshot = store
            .payment_link_creation_read_snapshot(&binding, version)
            .await?;
        let payload = snapshot.payload();
        let object = payload.as_object().ok_or_else(mismatch)?;
        if object.len() != 3
            || !["request", "providerFields", "trustedPrices"]
                .iter()
                .all(|key| object.contains_key(*key))
            || payload["trustedPrices"] != json!(profile.prices)
        {
            return Err(mismatch());
        }
        Ok((snapshot, binding.provider_id().to_owned()))
    }

    /// Lists explicit, nonempty actor-owned completed UUIDs in input order, preserving
    /// duplicates. Every local proof is validated before provider I/O; original proofs
    /// are reobserved after each exact GET. Only boolean provider active=true is included.
    /// No enumeration, account scan, pagination, cache writes or lifecycle synchronization.
    /// One shared five-second provider context covers the entire remote phase. Evidence
    /// and results each have an aggregate 8 MiB bound (stricter than configured TS).
    /// Individual observations are not an atomic multi-record or remote snapshot.
    /// # Errors
    /// Rejects empty/nil/over-limit selections before DB access, missing/foreign/stale
    /// provenance, malformed active, budget overflow or expired provider context.
    pub async fn list_bound_created_payment_links(
        &self,
        profile: &BoundPaymentLinkCreationProfile,
        ids: &[Uuid],
        limit: BoundPaymentLinkListLimit,
    ) -> BlueprintResult<Vec<PaymentLinkDto>> {
        if ids.is_empty() || ids.len() > limit.0 || ids.iter().any(Uuid::is_nil) {
            return Err(mismatch());
        }
        self.client().direct_creation_version()?;
        let mut remaining = MAX_LIST_BYTES;
        let mut prepared = Vec::with_capacity(ids.len());
        for id in ids {
            let item = self
                .prepare_bound_created_payment_link(profile, *id)
                .await?;
            consume_bytes(&mut remaining, item.0.evidence_size()?)?;
            prepared.push(item);
        }
        let context = self
            .client()
            .verify_context(profile.scope.account_id(), profile.scope.livemode())
            .await?;
        let mut results = Vec::new();
        let mut remaining = MAX_LIST_BYTES;
        for (snapshot, provider_id) in prepared {
            let provider = self
                .client()
                .retrieve_payment_link_in_context(&context, &provider_id)
                .await?;
            let active = provider["active"].as_bool().ok_or_else(mismatch)?;
            let record = self
                .base()
                .store()
                .read_payment_link_creation_snapshot(&snapshot)
                .await?;
            context.ensure_current(self.client())?;
            if active {
                let mut dto: PaymentLinkDto =
                    serde_json::from_value(record).map_err(|_| mismatch())?;
                dto.provider_fields = Some(provider);
                consume_result(&mut remaining, &dto)?;
                results.push(dto);
            }
        }
        context.ensure_current(self.client())?;
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trusted_list_limits_and_aggregate_budget_are_bounded() {
        assert_eq!(BoundPaymentLinkListLimit::default().0, 100);
        assert!(BoundPaymentLinkListLimit::trusted(0).is_err());
        assert!(BoundPaymentLinkListLimit::trusted(1001).is_err());
        assert_eq!(BoundPaymentLinkListLimit::trusted(1000).unwrap().0, 1000);
        let mut remaining = MAX_LIST_BYTES;
        consume_bytes(&mut remaining, MAX_LIST_BYTES - 1).unwrap();
        consume_bytes(&mut remaining, 1).unwrap();
        assert!(consume_bytes(&mut remaining, 1).is_err());
        let dto: PaymentLinkDto = serde_json::from_value(json!({"id":"public","amount":1,"currency":"USD","paymentMethods":["card"],"status":"PENDING","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"})).unwrap();
        let bytes = serde_json::to_vec(&dto).unwrap().len();
        let mut exact = bytes;
        consume_result(&mut exact, &dto).unwrap();
        assert_eq!(exact, 0);
        assert!(consume_result(&mut (bytes - 1), &dto).is_err());
    }
}
