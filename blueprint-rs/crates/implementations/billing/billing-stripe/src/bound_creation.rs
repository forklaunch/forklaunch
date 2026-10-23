//! Opt-in durable public UUID creation. Legacy routes/cache profiles are untouched.
use crate::services::StripeBillingService;
use forklaunch_billing_base::services::postgres::{
    PaymentLinkCreationClaim, PaymentLinkCreationStatus, PostgresBillingStore, StripeBindingScope,
};
use forklaunch_billing_interface::services::{CreatePaymentLinkDto, PaymentLinkDto};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use uuid::Uuid;

/// Server-owned finite creation profile. Price references are explicitly supplied
/// trusted configuration, not inferred local ownership or validated price amounts.
/// This version permits only `line_items` with price/quantity and normal payment
/// methods/currency. Delegation, transfers, fees, metadata and unknown options fail.
pub struct BoundPaymentLinkCreationProfile {
    pub(crate) scope: StripeBindingScope,
    pub(crate) prices: BTreeSet<String>,
}
impl BoundPaymentLinkCreationProfile {
    /// # Errors
    /// Rejects empty or malformed trusted price-reference sets.
    pub fn trusted(scope: StripeBindingScope, prices: Vec<String>) -> BlueprintResult<Self> {
        if prices.is_empty()
            || prices.len() > 1000
            || prices.iter().any(|id| !provider_id(id, "price_"))
        {
            return Err(invalid());
        }
        Ok(Self {
            scope,
            prices: prices.into_iter().collect(),
        })
    }
}
/// Historical creation outcome. Unresolved never means the provider did nothing.
/// Inspect the operation; this API never automatically replays a pending POST.
pub enum BoundPaymentLinkCreationOutcome {
    Completed {
        operation_id: Uuid,
        record: PaymentLinkDto,
    },
    Unresolved {
        operation_id: Uuid,
    },
    /// Local commit returned success but the observation expired afterwards.
    /// Receipt inspection is necessary; no rollback or redispatch is possible.
    RecordedEvidenceNoLongerCurrent {
        operation_id: Uuid,
    },
}
pub(crate) fn invalid() -> BlueprintError {
    BlueprintError::new(
        "billing_bound_creation_invalid",
        "Invalid bound payment link creation",
    )
}
fn provider_id(id: &str, prefix: &str) -> bool {
    id.starts_with(prefix)
        && id.len() > prefix.len()
        && id.len() <= 128
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
pub(crate) fn fields(
    profile: &BoundPaymentLinkCreationProfile,
    request: &CreatePaymentLinkDto,
) -> BlueprintResult<Value> {
    if request.id.is_some()
        || !request.amount.is_finite()
        || request.amount < 0.0
        || request.status != "PENDING"
        || request.currency.len() != 3
        || !request.currency.bytes().all(|b| b.is_ascii_uppercase())
        || request.payment_methods.is_empty()
        || request.payment_methods.len() > 32
        || request.payment_methods.iter().any(|v| {
            v.is_empty()
                || v.len() > 128
                || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
    {
        return Err(invalid());
    }
    let options = request
        .provider_fields
        .as_ref()
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    if options.len() != 1 || !options.contains_key("line_items") {
        return Err(invalid());
    }
    let items = options["line_items"].as_array().ok_or_else(invalid)?;
    if items.is_empty() || items.len() > 20 {
        return Err(invalid());
    }
    for item in items {
        let item = item.as_object().ok_or_else(invalid)?;
        if item.len() != 2
            || !item
                .get("price")
                .and_then(Value::as_str)
                .is_some_and(|price| profile.prices.contains(price))
            || !item
                .get("quantity")
                .and_then(Value::as_u64)
                .is_some_and(|q| q > 0 && q <= 999_999)
        {
            return Err(invalid());
        }
    }
    Ok(
        json!({"line_items":items,"currency":request.currency,"payment_method_types":request.payment_methods}),
    )
}
pub(crate) fn completed(
    operation_id: Uuid,
    record: Value,
) -> BlueprintResult<BoundPaymentLinkCreationOutcome> {
    Ok(BoundPaymentLinkCreationOutcome::Completed {
        operation_id,
        record: serde_json::from_value(record).map_err(|_| invalid())?,
    })
}
impl StripeBillingService<PostgresBillingStore> {
    /// Validates direct transport and creation/inspection grants without provider I/O.
    /// No separately privileged publisher is used by this finite `PaymentLink` profile.
    /// # Errors
    /// Rejects managed clients and missing local privileges before listening.
    pub async fn validate_bound_payment_link_creation_configuration(&self) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        self.base()
            .store()
            .validate_payment_link_creation_access()
            .await
    }

    /// Direct-only, one-dispatch creation backed by a durable immutable journal.
    /// Operation IDs are retry/inspection keys, never authorization. The configured
    /// store tenant/actor and trusted profile supply authority. No cache is written.
    /// # Errors
    /// Rejects invalid profile/input, managed clients, and conflicting operation IDs.
    /// After reservation, uncertain outcomes return an inspectable operation UUID.
    pub async fn create_bound_payment_link(
        &self,
        profile: &BoundPaymentLinkCreationProfile,
        operation: Uuid,
        request: CreatePaymentLinkDto,
    ) -> BlueprintResult<BoundPaymentLinkCreationOutcome> {
        let version = self.client().direct_creation_version()?;
        if operation.is_nil() {
            return Err(invalid());
        }
        let fields = fields(profile, &request)?;
        let payload =
            json!({"request":request,"providerFields":fields,"trustedPrices":profile.prices});
        let claim = match self
            .base()
            .store()
            .reserve_payment_link_creation(&profile.scope, operation, version, &payload)
            .await
        {
            Ok(claim) => claim,
            Err(error)
                if error.code == "billing_provider_binding_conflict"
                    || error.code == "billing_validation" =>
            {
                return Err(error);
            }
            Err(_) => {
                return Ok(BoundPaymentLinkCreationOutcome::Unresolved {
                    operation_id: operation,
                });
            }
        };
        let ticket = match claim {
            PaymentLinkCreationClaim::Reserved(ticket) => ticket,
            PaymentLinkCreationClaim::Existing(PaymentLinkCreationStatus::Pending { .. }) => {
                return Ok(BoundPaymentLinkCreationOutcome::Unresolved {
                    operation_id: operation,
                });
            }
            PaymentLinkCreationClaim::Existing(PaymentLinkCreationStatus::Completed {
                record,
                ..
            }) => return completed(operation, record),
        };
        // Verification begins after reservation commit. No ticket is reissued if
        // cancellation happens before or during this network work.
        let Ok(context) = self
            .client()
            .verify_context(profile.scope.account_id(), profile.scope.livemode())
            .await
        else {
            return Ok(BoundPaymentLinkCreationOutcome::Unresolved {
                operation_id: operation,
            });
        };
        let Ok(provider) = self
            .client()
            .create_payment_link_in_context(&context, ticket.idempotency_key(), &fields)
            .await
        else {
            return Ok(BoundPaymentLinkCreationOutcome::Unresolved {
                operation_id: operation,
            });
        };
        if provider["object"] != "payment_link"
            || !provider["id"]
                .as_str()
                .is_some_and(|id| provider_id(id, "plink_"))
            || provider["livemode"].as_bool() != Some(profile.scope.livemode())
        {
            return Ok(BoundPaymentLinkCreationOutcome::Unresolved {
                operation_id: operation,
            });
        }
        let now = chrono::Utc::now();
        let record = json!({"id":ticket.public_id().to_string(),"amount":request.amount,"currency":request.currency,"paymentMethods":request.payment_methods,"status":request.status,"providerFields":provider,"createdAt":now,"updatedAt":now});
        if self
            .base()
            .store()
            .complete_payment_link_creation(ticket, record.clone(), || {
                context.ensure_current(self.client())
            })
            .await
            .is_err()
        {
            return Ok(BoundPaymentLinkCreationOutcome::Unresolved {
                operation_id: operation,
            });
        }
        if context.ensure_current(self.client()).is_err() {
            return Ok(
                BoundPaymentLinkCreationOutcome::RecordedEvidenceNoLongerCurrent {
                    operation_id: operation,
                },
            );
        }
        completed(operation, record)
    }
    /// Inspect an immutable actor/tenant-owned receipt. This does not refresh
    /// provider state or authorize future mutations through mutable record fields.
    /// # Errors
    /// Returns sanitized actor/tenant/encryption/storage/audit failures.
    pub async fn inspect_bound_payment_link_creation(
        &self,
        operation: Uuid,
    ) -> BlueprintResult<Option<BoundPaymentLinkCreationOutcome>> {
        self.base()
            .store()
            .inspect_payment_link_creation(operation)
            .await?
            .map(|status| match status {
                PaymentLinkCreationStatus::Pending { operation_id, .. } => {
                    Ok(BoundPaymentLinkCreationOutcome::Unresolved { operation_id })
                }
                PaymentLinkCreationStatus::Completed {
                    operation_id,
                    record,
                } => completed(operation_id, record),
            })
            .transpose()
    }
}
