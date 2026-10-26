//! Explicit direct Checkout creation; an open session is not a paid subscription.
//! Remote Customer/Plan/Price reads are observations, not remote mutation locks.
mod validation;
use crate::{client::VerifiedStripeContext, services::StripeBillingService};
use forklaunch_billing_base::services::postgres::{
    CheckoutCreationClaim, PostgresBillingStore, StripeBindingScope, TrustedCheckoutPublisher,
};
use forklaunch_billing_interface::services::CheckoutSessionDto;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use reqwest::Method;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Explicit server authorization of completed local Plan provenance and redirects.
/// Caller still authorizes which local Customer may use these Plans.
pub struct BoundCheckoutCreationProfile {
    scope: StripeBindingScope,
    plans: BTreeMap<Uuid, Uuid>,
    success_url: String,
    cancel_url: String,
}
impl BoundCheckoutCreationProfile {
    /// # Errors
    /// Rejects ambiguous provenance, unsafe redirects or oversized configuration.
    pub fn trusted(
        scope: StripeBindingScope,
        plans: Vec<(Uuid, Uuid)>,
        success_url: String,
        cancel_url: String,
    ) -> BlueprintResult<Self> {
        if plans.is_empty()
            || plans.len() > 1000
            || plans.iter().any(|(a, b)| a.is_nil() || b.is_nil())
            || !redirect(&success_url)
            || !redirect(&cancel_url)
        {
            return Err(invalid());
        }
        let count = plans.len();
        let plans: BTreeMap<_, _> = plans.into_iter().collect();
        if count != plans.len() {
            return Err(invalid());
        }
        Ok(Self {
            scope,
            plans,
            success_url,
            cancel_url,
        })
    }
}
fn redirect(value: &str) -> bool {
    value.len() <= 8192
        && !value.chars().any(char::is_control)
        && reqwest::Url::parse(value).is_ok_and(|u| {
            u.scheme() == "https"
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
        })
}
/// No raw provider options, customer/price IDs or caller-supplied public ID.
pub struct BoundCheckoutCreateRequest {
    pub local_customer_id: Uuid,
    pub local_plan_id: Uuid,
    pub quantity: u64,
}
/// Only a fresh new completion contains a URL. Historical results return IDs.
pub enum BoundCheckoutCreationOutcome {
    Created {
        operation_id: Uuid,
        record: Box<CheckoutSessionDto>,
    },
    PreviouslyCompleted {
        operation_id: Uuid,
        public_id: Uuid,
    },
    Unresolved {
        operation_id: Uuid,
    },
    RecordedEvidenceNoLongerCurrent {
        operation_id: Uuid,
    },
}
fn invalid() -> BlueprintError {
    BlueprintError::new(
        "billing_bound_checkout_invalid",
        "Invalid bound checkout creation",
    )
}
impl StripeBillingService<PostgresBillingStore> {
    /// Preflight for an explicitly selected direct-only server adapter. No provider I/O.
    /// # Errors
    /// Rejects managed transport or a publisher with different actor/tenant/database/roles.
    pub async fn validate_bound_checkout_configuration(
        &self,
        publisher: &TrustedCheckoutPublisher,
    ) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        publisher.validate_for(self.base().store()).await
    }

    /// One POST after a confirmed reservation. Unknown outcomes retain all claims;
    /// inspection never reconstructs dispatch capability or returns historical URL.
    /// # Errors
    /// Rejects unsupported profile/dependencies before dispatch. Post-effect errors
    /// return an unresolved operation rather than claiming rollback or retry safety.
    #[allow(clippy::too_many_lines)]
    pub async fn create_bound_checkout_session(
        &self,
        profile: &BoundCheckoutCreationProfile,
        publisher: &TrustedCheckoutPublisher,
        operation: Uuid,
        request: BoundCheckoutCreateRequest,
    ) -> BlueprintResult<BoundCheckoutCreationOutcome> {
        let version = self.client().direct_creation_version()?;
        if operation.is_nil()
            || request.local_customer_id.is_nil()
            || request.quantity == 0
            || request.quantity > 9_007_199_254_740_991
        {
            return Err(invalid());
        }
        let plan_operation = *profile
            .plans
            .get(&request.local_plan_id)
            .ok_or_else(invalid)?;
        publisher.validate_for(self.base().store()).await?;
        let snapshot = self
            .base()
            .store()
            .snapshot_checkout_dependencies(
                &profile.scope,
                request.local_customer_id,
                request.local_plan_id,
                plan_operation,
                version,
            )
            .await?;
        let customer = snapshot.provider_customer_id().to_owned();
        let plan = snapshot.provider_plan_id().to_owned();
        let receipt = snapshot.plan_receipt().clone();
        let context = self
            .client()
            .verify_context(profile.scope.account_id(), profile.scope.livemode())
            .await?;
        let current_customer = self
            .client()
            .checkout_creation_request(
                &context,
                Method::GET,
                &["v1", "customers", &customer],
                None,
                &json!({}),
            )
            .await?;
        validation::customer(&current_customer, &customer, context.livemode())?;
        let current_plan = self
            .client()
            .checkout_creation_request(
                &context,
                Method::GET,
                &["v1", "plans", &plan],
                None,
                &json!({"expand":["product"]}),
            )
            .await?;
        validation::plan(&current_plan, &receipt, context.livemode(), true)?;
        // Stripe's Price retrieve contract accepts Plan IDs. Verify the returned Price
        // independently; never synthesize a price ID from a prefix or mutable DTO.
        let price = self
            .client()
            .checkout_creation_request(
                &context,
                Method::GET,
                &["v1", "prices", &plan],
                None,
                &json!({}),
            )
            .await?;
        validation::price(&price, &receipt, context.livemode())?;
        let unit = validation::integer(&price["unit_amount"])?;
        let total = unit
            .checked_mul(request.quantity)
            .filter(|v| *v <= 9_007_199_254_740_991)
            .ok_or_else(invalid)?;
        let fields = json!({"mode":"subscription","ui_mode":"hosted_page","customer":customer,"line_items":[{"price":plan,"quantity":request.quantity}],"payment_method_types":["card"],"success_url":profile.success_url,"cancel_url":profile.cancel_url,"allow_promotion_codes":false,"automatic_tax":{"enabled":false},"adaptive_pricing":{"enabled":false}});
        let trusted_plans: Vec<_> = profile
            .plans
            .iter()
            .map(|(id, op)| json!([id.to_string(), op.to_string()]))
            .collect();
        let payload =
            json!({"profile":"direct_checkout_v1","trustedPlans":trusted_plans,"fields":fields});
        let ticket = match self
            .base()
            .store()
            .reserve_checkout_creation(snapshot, operation, &payload, || {
                context.ensure_current(self.client())
            })
            .await?
        {
            CheckoutCreationClaim::Reserved(ticket) => ticket,
            CheckoutCreationClaim::Existing(_) => {
                return Ok(BoundCheckoutCreationOutcome::Unresolved {
                    operation_id: operation,
                });
            }
            CheckoutCreationClaim::Completed {
                operation_id,
                public_id,
            } => {
                return Ok(BoundCheckoutCreationOutcome::PreviouslyCompleted {
                    operation_id,
                    public_id,
                });
            }
        };
        let Ok(provider) = self
            .dispatch_bound_checkout(
                &context,
                profile,
                &customer,
                &receipt,
                request.quantity,
                total,
                &fields,
                ticket.idempotency_key(),
            )
            .await
        else {
            return Ok(BoundCheckoutCreationOutcome::Unresolved {
                operation_id: operation,
            });
        };
        let expires = validation::timestamp(&provider["expires_at"])?;
        let now = chrono::Utc::now();
        let record = json!({"id":ticket.public_id().to_string(),"customerId":request.local_customer_id.to_string(),"paymentMethods":["card"],"currency":receipt["currency"],"status":"PENDING","uri":provider["url"],"successRedirectUri":profile.success_url,"cancelRedirectUri":profile.cancel_url,"expiresAt":expires,"providerFields":provider,"createdAt":now,"updatedAt":now});
        if publisher
            .complete(ticket, record.clone(), || {
                context.ensure_current(self.client())
            })
            .await
            .is_err()
        {
            return Ok(BoundCheckoutCreationOutcome::Unresolved {
                operation_id: operation,
            });
        }
        if context.ensure_current(self.client()).is_err() {
            return Ok(
                BoundCheckoutCreationOutcome::RecordedEvidenceNoLongerCurrent {
                    operation_id: operation,
                },
            );
        }
        Ok(BoundCheckoutCreationOutcome::Created {
            operation_id: operation,
            record: Box::new(serde_json::from_value(record).map_err(|_| invalid())?),
        })
    }
    #[allow(clippy::too_many_arguments)]
    async fn dispatch_bound_checkout(
        &self,
        context: &VerifiedStripeContext,
        profile: &BoundCheckoutCreationProfile,
        customer: &str,
        receipt: &Value,
        quantity: u64,
        total: u64,
        fields: &Value,
        key: &str,
    ) -> BlueprintResult<Value> {
        let provider = self
            .client()
            .checkout_creation_request(
                context,
                Method::POST,
                &["v1", "checkout", "sessions"],
                Some(key),
                fields,
            )
            .await?;
        validation::session(
            &provider,
            profile,
            customer,
            receipt,
            context.livemode(),
            total,
        )?;
        let id = provider["id"].as_str().ok_or_else(invalid)?;
        let items = self
            .client()
            .checkout_creation_request(
                context,
                Method::GET,
                &["v1", "checkout", "sessions", id, "line_items"],
                None,
                &json!({"limit":2}),
            )
            .await?;
        validation::line_items(&items, id, receipt, context.livemode(), quantity, total)?;
        Ok(provider)
    }
}
