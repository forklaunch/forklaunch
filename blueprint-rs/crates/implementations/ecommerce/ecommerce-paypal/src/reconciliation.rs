//! Reconcile completed captures by reading the provider; never initiate capture.
use crate::{PaypalClient, PaypalPaymentService, PaypalProvider, shape_error};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_base::cancellation::{PaymentCancellationStore, VerifiedCancellation};
use forklaunch_ecommerce_base::reconciliation::{PaymentReconciliationStore, VerifiedCapture};
use forklaunch_ecommerce_interface::{PaymentDto, PaymentService};
use reqwest::Method;
use serde_json::Value;
use std::future::Future;

/// Authenticated order lookup, separate so existing provider seams remain compatible.
pub trait PaypalOrderReader: PaypalProvider {
    /// # Errors
    /// Returns sanitized provider/network/shape errors. Implementations must bind
    /// the lookup to their configured merchant credentials and requested order ID.
    fn retrieve_order_details(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<Value>> + Send;
}
impl PaypalClient {
    /// Retrieves an order using configured merchant credentials and bounded transport.
    /// Returned provider data can contain PII; callers must not log it indiscriminately.
    /// # Errors
    /// Rejects malformed identities, substituted responses and provider failures.
    pub async fn retrieve_order_details(&self, id: &str) -> BlueprintResult<Value> {
        if !valid_id(id) {
            return Err(shape_error());
        }
        let token = self.access_token().await?;
        let response = self
            .http
            .execute_json(
                self.http
                    .request(Method::GET, &["v2", "checkout", "orders", id])?
                    .bearer_auth(token.as_str()),
            )
            .await?;
        if !response.is_success() {
            return Err(forklaunch_core::http::provider_http::provider_status_error(
                "PayPal",
                "retrieve order",
                response.status,
            ));
        }
        if response.body["id"].as_str() != Some(id) {
            return Err(shape_error());
        }
        Ok(response.body)
    }
}
impl PaypalOrderReader for PaypalClient {
    async fn retrieve_order_details(&self, id: &str) -> BlueprintResult<Value> {
        self.retrieve_order_details(id).await
    }
}
impl<S: PaymentReconciliationStore, P: PaypalOrderReader> PaypalPaymentService<S, P> {
    /// Repairs a local payment from a verified completed single-unit/single-capture
    /// order. Never captures funds, resets a claim, changes an order or triggers delivery.
    /// Uncaptured, pending, refunded or ambiguous provider data requires manual review.
    /// # Errors
    /// Rejects insufficient/mismatched evidence, stale local snapshots and audit failures.
    pub async fn reconcile_captured_payment(
        &self,
        payment_id: &str,
        actor: AuditContext,
    ) -> BlueprintResult<PaymentDto> {
        let expected = self.base.get_payment(payment_id).await?;
        let reference = expected
            .provider_ref
            .as_deref()
            .filter(|id| valid_id(id))
            .ok_or_else(review_required)?;
        let order = self.client.retrieve_order_details(reference).await?;
        let evidence = verify_capture(&expected, &order)?;
        self.base
            .record_verified_capture(expected, evidence, actor)
            .await
    }
}
impl<S: PaymentCancellationStore, P: PaypalOrderReader> PaypalPaymentService<S, P> {
    /// Reads terminal VOIDED evidence and atomically records local failure with an
    /// actor audit. Requires a single CAPTURE purchase unit with explicitly empty
    /// captures, authorizations and refunds. Missing history remains manual review,
    /// even if a provider omits empty arrays. Never voids/refunds provider funds or
    /// transitions the local order. `PayPal` supplies no cancellation timestamp.
    /// # Errors
    /// Rejects incomplete/nonterminal/charged evidence, mismatched ownership or
    /// amount, stale snapshots, contradictory capture audits and storage failures.
    pub async fn reconcile_voided_payment(
        &self,
        payment_id: &str,
        actor: AuditContext,
    ) -> BlueprintResult<PaymentDto> {
        let expected = self.base.get_payment(payment_id).await?;
        let reference = expected
            .provider_ref
            .as_deref()
            .filter(|id| valid_id(id))
            .ok_or_else(review_required)?;
        let order = self.client.retrieve_order_details(reference).await?;
        let evidence = verify_void(&expected, &order)?;
        self.base
            .record_verified_cancellation(expected, evidence, actor)
            .await
    }
}
fn verify_void(payment: &PaymentDto, order: &Value) -> BlueprintResult<VerifiedCancellation> {
    // PayPal Orders v2 order_status defines VOIDED as all purchase units voided;
    // purchase_unit.payments is the comprehensive history, whose three arrays
    // allow zero entries. Require explicit emptiness rather than interpreting
    // missing projections as evidence of no previous monetary effects.
    // https://developer.paypal.com/api/orders/v2/schema.json
    if order["id"].as_str() != payment.provider_ref.as_deref()
        || order["status"] != "VOIDED"
        || order["intent"] != "CAPTURE"
        || payment.amount_cents <= 0
        || payment.currency.len() != 3
        || !payment
            .currency
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic())
    {
        return Err(review_required());
    }
    let units = order["purchase_units"]
        .as_array()
        .filter(|units| units.len() == 1)
        .ok_or_else(review_required)?;
    let unit = &units[0];
    if unit["reference_id"].as_str() != Some(payment.order_id.as_str())
        || !matching_amount(&unit["amount"], payment)
        || ["captures", "authorizations", "refunds"].iter().any(|key| {
            unit["payments"]
                .get(key)
                .and_then(Value::as_array)
                .is_none_or(|items| !items.is_empty())
        })
    {
        return Err(review_required());
    }
    Ok(VerifiedCancellation {
        provider: "paypal".into(),
        provider_ref: payment.provider_ref.clone().ok_or_else(review_required)?,
        canceled_at: None,
        amount_cents: payment.amount_cents,
        currency: payment.currency.to_ascii_uppercase(),
    })
}
fn verify_capture(payment: &PaymentDto, order: &Value) -> BlueprintResult<VerifiedCapture> {
    if order["id"].as_str() != payment.provider_ref.as_deref()
        || order["status"] != "COMPLETED"
        || order["intent"] != "CAPTURE"
        || payment.amount_cents <= 0
        || payment.currency.len() != 3
        || !payment
            .currency
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic())
    {
        return Err(review_required());
    }
    let units = order["purchase_units"]
        .as_array()
        .filter(|units| units.len() == 1)
        .ok_or_else(review_required)?;
    let unit = &units[0];
    if unit["reference_id"].as_str() != Some(payment.order_id.as_str())
        || !matching_amount(&unit["amount"], payment)
        || ["authorizations", "refunds"].iter().any(|key| {
            unit["payments"]
                .get(key)
                .is_some_and(|value| value.as_array().is_none_or(|items| !items.is_empty()))
        })
    {
        return Err(review_required());
    }
    let captures = unit["payments"]["captures"]
        .as_array()
        .filter(|items| items.len() == 1)
        .ok_or_else(review_required)?;
    let capture = &captures[0];
    let capture_id = capture["id"]
        .as_str()
        .filter(|id| valid_id(id))
        .ok_or_else(review_required)?;
    if capture["status"] != "COMPLETED"
        || capture["final_capture"] != true
        || !matching_amount(&capture["amount"], payment)
    {
        return Err(review_required());
    }
    Ok(VerifiedCapture {
        provider: "paypal".into(),
        provider_ref: order["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or_else(review_required)?
            .into(),
        capture_id: capture_id.into(),
        amount_cents: payment.amount_cents,
        currency: payment.currency.to_ascii_uppercase(),
    })
}
fn matching_amount(value: &Value, payment: &PaymentDto) -> bool {
    value["currency_code"]
        .as_str()
        .is_some_and(|currency| currency.eq_ignore_ascii_case(&payment.currency))
        && value["value"].as_str().and_then(cents) == Some(payment.amount_cents)
}
// Exact decimal arithmetic for the existing amountCents/100 provider contract.
pub(crate) fn cents(value: &str) -> Option<i64> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let whole = whole.parse::<i64>().ok()?.checked_mul(100)?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().ok()?.checked_mul(10)?,
        _ => fraction.parse::<i64>().ok()?,
    };
    whole.checked_add(fraction)
}
fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn review_required() -> BlueprintError {
    BlueprintError::new(
        "paypal_reconciliation_required",
        "PayPal payment evidence requires manual review",
    )
}
