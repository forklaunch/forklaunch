//! Read-only provider verification followed by an audited local payment repair.
use crate::{StripeClient, StripeConnect, StripePaymentService, shape_error};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::provider_status_error;
use forklaunch_ecommerce_base::cancellation::{PaymentCancellationStore, VerifiedCancellation};
use forklaunch_ecommerce_base::reconciliation::{PaymentReconciliationStore, VerifiedCapture};
use forklaunch_ecommerce_interface::{PaymentDto, PaymentService};
use reqwest::Method;
use serde_json::Value;
impl StripeClient {
    /// Retrieves a `PaymentIntent` with its latest charge expanded, in the configured
    /// Connect account when present. Provider data can contain PII/client secrets.
    /// # Errors
    /// Rejects invalid/substituted identities and sanitized transport/provider errors.
    pub async fn retrieve_intent_details(
        &self,
        id: &str,
        connect: Option<&StripeConnect>,
    ) -> BlueprintResult<Value> {
        if !valid_id(id) {
            return Err(shape_error());
        }
        let mut request = self
            .http
            .request(Method::GET, &["v1", "payment_intents", id])?
            .bearer_auth(self.secret.as_str())
            .query(&[("expand[]", "latest_charge")]);
        if let Some(connect) = connect {
            request = request.header("Stripe-Account", &connect.account_id);
        }
        let response = self.http.execute_json(request).await?;
        if !response.is_success() {
            return Err(provider_status_error(
                "Stripe",
                "retrieve capture evidence",
                response.status,
            ));
        }
        if response.body["id"].as_str() != Some(id) {
            return Err(shape_error());
        }
        Ok(response.body)
    }
}
impl<S: PaymentReconciliationStore> StripePaymentService<S> {
    /// Verifies a fully captured, unrefunded, undisputed payment and repairs local
    /// state atomically with an operator audit. Never creates/captures/refunds a charge
    /// or transitions an order. Other provider states require review.
    /// # Errors
    /// Rejects insufficient/mismatched evidence, stale snapshots and audit failures.
    pub async fn reconcile_captured_payment(
        &self,
        id: &str,
        actor: AuditContext,
    ) -> BlueprintResult<PaymentDto> {
        let expected = self.base.get_payment(id).await?;
        let reference = expected
            .provider_ref
            .as_deref()
            .filter(|id| valid_id(id))
            .ok_or_else(review_required)?;
        let intent = self
            .client
            .retrieve_intent_details(reference, self.connect.as_ref())
            .await?;
        let evidence = verify_capture(&expected, &intent)?;
        self.base
            .record_verified_capture(expected, evidence, actor)
            .await
    }
}
impl<S: PaymentCancellationStore> StripePaymentService<S> {
    /// Read a terminal canceled intent with no charge or a verified failed, unpaid
    /// charge and atomically mark the local
    /// payment failed with an operator audit. Preserves the provider reference and
    /// never cancels/refunds a provider payment or transitions the associated order.
    /// # Errors
    /// Rejects nonterminal, paid, authorized, mismatched or contradictory provider evidence,
    /// stale snapshots and audit/storage failures. Pending/ambiguous states require review.
    pub async fn reconcile_canceled_payment(
        &self,
        id: &str,
        actor: AuditContext,
    ) -> BlueprintResult<PaymentDto> {
        let expected = self.base.get_payment(id).await?;
        let reference = expected
            .provider_ref
            .as_deref()
            .filter(|id| valid_id(id))
            .ok_or_else(review_required)?;
        let intent = self
            .client
            .retrieve_intent_details(reference, self.connect.as_ref())
            .await?;
        let evidence = verify_cancellation(&expected, &intent)?;
        self.base
            .record_verified_cancellation(expected, evidence, actor)
            .await
    }
}
pub(crate) fn verify_cancellation(
    payment: &PaymentDto,
    intent: &Value,
) -> BlueprintResult<VerifiedCancellation> {
    let reference = payment
        .provider_ref
        .as_deref()
        .filter(|id| valid_id(id))
        .ok_or_else(review_required)?;
    let canceled_at = intent["canceled_at"]
        .as_i64()
        .filter(|timestamp| *timestamp > 0)
        .ok_or_else(review_required)?;
    if payment.amount_cents <= 0
        || payment.currency.len() != 3
        || !payment.currency.bytes().all(|b| b.is_ascii_alphabetic())
        || intent["id"] != reference
        || intent["object"] != "payment_intent"
        || intent["status"] != "canceled"
        || intent["amount"].as_i64() != Some(payment.amount_cents)
        || intent["amount_received"].as_i64() != Some(0)
        || intent["amount_capturable"].as_i64() != Some(0)
        || !unpaid_latest_charge(payment, intent.get("latest_charge"), reference)
        || intent["metadata"]["orderId"].as_str() != Some(payment.order_id.as_str())
        || !intent["currency"]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(&payment.currency))
    {
        return Err(review_required());
    }
    Ok(VerifiedCancellation {
        provider: "stripe".into(),
        provider_ref: reference.into(),
        canceled_at: Some(canceled_at),
        amount_cents: payment.amount_cents,
        currency: payment.currency.to_ascii_uppercase(),
    })
}
// A failed attempt is not evidence of cancellation by itself: the caller also
// requires the parent intent's terminal canceled status and zero received and
// capturable amounts. String references, absent fields, authorized charges and
// refunds remain manual review. Stripe's expanded charge is read in the same
// merchant/Connect scope as its parent intent.
fn unpaid_latest_charge(payment: &PaymentDto, charge: Option<&Value>, reference: &str) -> bool {
    let Some(charge) = charge else {
        return false;
    };
    if charge.is_null() {
        return true;
    }
    charge["id"].as_str().is_some_and(valid_id)
        && charge["object"] == "charge"
        && charge["payment_intent"] == reference
        && charge["status"] == "failed"
        && charge["paid"] == false
        && charge["captured"] == false
        && charge["refunded"] == false
        && charge["disputed"] == false
        && charge["amount"].as_i64() == Some(payment.amount_cents)
        && charge["amount_captured"].as_i64() == Some(0)
        && charge["amount_refunded"].as_i64() == Some(0)
        && charge["currency"]
            .as_str()
            .is_some_and(|currency| currency.eq_ignore_ascii_case(&payment.currency))
}
pub(crate) fn verify_capture(
    payment: &PaymentDto,
    intent: &Value,
) -> BlueprintResult<VerifiedCapture> {
    let reference = payment
        .provider_ref
        .as_deref()
        .filter(|id| valid_id(id))
        .ok_or_else(review_required)?;
    let currency_matches = |value: &Value| {
        value
            .as_str()
            .is_some_and(|currency| currency.eq_ignore_ascii_case(&payment.currency))
    };
    if payment.amount_cents <= 0
        || payment.currency.len() != 3
        || !payment
            .currency
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic())
        || intent["id"] != reference
        || intent["object"] != "payment_intent"
        || intent["status"] != "succeeded"
        || intent["amount"].as_i64() != Some(payment.amount_cents)
        || intent["amount_received"].as_i64() != Some(payment.amount_cents)
        || !currency_matches(&intent["currency"])
        || intent["metadata"]["orderId"].as_str() != Some(payment.order_id.as_str())
    {
        return Err(review_required());
    }
    let charge = &intent["latest_charge"];
    let capture_id = charge["id"]
        .as_str()
        .filter(|id| valid_id(id))
        .ok_or_else(review_required)?;
    if charge["object"] != "charge"
        || charge["payment_intent"] != reference
        || charge["status"] != "succeeded"
        || charge["paid"] != true
        || charge["captured"] != true
        || charge["refunded"] != false
        || charge["disputed"] != false
        || charge["amount"].as_i64() != Some(payment.amount_cents)
        || charge["amount_captured"].as_i64() != Some(payment.amount_cents)
        || charge["amount_refunded"].as_i64() != Some(0)
        || !currency_matches(&charge["currency"])
    {
        return Err(review_required());
    }
    Ok(VerifiedCapture {
        provider: "stripe".into(),
        provider_ref: reference.into(),
        capture_id: capture_id.into(),
        amount_cents: payment.amount_cents,
        currency: payment.currency.to_ascii_uppercase(),
    })
}
fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn review_required() -> BlueprintError {
    BlueprintError::new(
        "stripe_reconciliation_required",
        "Stripe capture evidence requires manual review",
    )
}
