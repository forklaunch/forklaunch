//! Direct/Connected captured-payment adoption, without provider writes or order changes.
//! Balance evidence deliberately excludes FX and proves bookkeeping, not settlement.
use crate::{
    StripeClient, StripeConnect, StripePaymentService, context::mismatch,
    reconciliation::verify_capture,
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::provider_status_error;
use forklaunch_ecommerce_base::{
    postgres::{PaymentProviderScope, PostgresEcommerceStore, ScopedCaptureOutcome},
    reconciliation::VerifiedCapture,
};
use forklaunch_ecommerce_interface::PaymentDto;
use reqwest::Method;
use serde_json::Value;
use uuid::Uuid;

impl StripePaymentService<PostgresEcommerceStore> {
    /// Adopts a fully captured payment under a fresh direct account/mode observation
    /// and immutable administrative binding. Preserves order state and provider refs.
    /// Retain `receipt_id` before calling: uncertain COMMIT requires receipt inspection,
    /// never a provider retry. A refund/dispute can occur after the authenticated GET;
    /// this bounded observation does not lock remote funds or prove settlement.
    /// # Errors
    /// Rejects Connect, changed local ownership/state, stale observations, incomplete,
    /// refunded/disputed/FX evidence, malformed provider data and failed audit writes.
    pub async fn reconcile_bound_captured_payment(
        &self,
        scope: &PaymentProviderScope,
        payment_id: &str,
        actor: AuditContext,
        receipt_id: Uuid,
    ) -> BlueprintResult<ScopedCaptureOutcome> {
        self.reconcile_bound_captured_payment_profile(scope, payment_id, actor, receipt_id, false)
            .await
    }
    /// Explicit direct-charge Connected counterpart. All provider GETs use the
    /// sealed account configuration. Delegation/transfers and absent critical
    /// nullable fields require manual review. Coherent historical application fees
    /// are allowed, independently of current fee settings; no net settlement or
    /// fee ownership is inferred. The same retained-receipt COMMIT rules apply.
    /// # Errors
    /// Rejects missing/mismatched Connect context, ambiguous transfer/fee evidence,
    /// stale local/provider snapshots and failed atomic audit/receipt writes.
    pub async fn reconcile_bound_connected_captured_payment(
        &self,
        scope: &PaymentProviderScope,
        payment_id: &str,
        actor: AuditContext,
        receipt_id: Uuid,
    ) -> BlueprintResult<ScopedCaptureOutcome> {
        self.reconcile_bound_captured_payment_profile(scope, payment_id, actor, receipt_id, true)
            .await
    }
    async fn reconcile_bound_captured_payment_profile(
        &self,
        scope: &PaymentProviderScope,
        payment_id: &str,
        actor: AuditContext,
        receipt_id: Uuid,
        connected: bool,
    ) -> BlueprintResult<ScopedCaptureOutcome> {
        if self.connect.is_some() != connected
            || scope.provider() != "stripe"
            || receipt_id.is_nil()
            || actor
                .actor_id
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
            || actor.correlation_id.trim().is_empty()
        {
            return Err(mismatch());
        }
        let context = self
            .client
            .verify_service_context(scope.merchant_id(), scope.livemode(), self.connect.as_ref())
            .await?;
        let binding = self
            .base
            .store()
            .payment_provider_binding(scope, payment_id)
            .await?;
        context.ensure_current(&self.client, self.connect.as_ref())?;
        let snapshot = self
            .base
            .store()
            .captured_payment_snapshot(&binding)
            .await?;
        context.ensure_current(&self.client, self.connect.as_ref())?;
        let intent = self
            .client
            .scoped_capture_evidence(binding.provider_ref(), context.sealed_connect_config())
            .await?;
        context.ensure_current(&self.client, self.connect.as_ref())?;
        let (evidence, balance_id) =
            verify_scoped_capture(snapshot.payment(), &intent, scope.livemode())?;
        if connected {
            verify_connected_charge_profile(&intent, snapshot.payment().amount_cents)?;
        }
        self.base
            .store()
            .record_scoped_capture(snapshot, evidence, &balance_id, actor, receipt_id, || {
                context.ensure_current(&self.client, self.connect.as_ref())
            })
            .await
    }
}
impl StripeClient {
    async fn scoped_capture_evidence(
        &self,
        id: &str,
        connect: Option<&StripeConnect>,
    ) -> BlueprintResult<Value> {
        if !safe_id(id) {
            return Err(mismatch());
        }
        let mut request = self
            .http
            .request(Method::GET, &["v1", "payment_intents", id])?
            .bearer_auth(self.secret.as_str())
            .query(&[("expand[]", "latest_charge.balance_transaction")]);
        if let Some(connect) = connect {
            request = request.header("Stripe-Account", &connect.account_id);
        }
        let response = self.http.execute_json(request).await?;
        if !response.is_success() {
            return Err(provider_status_error(
                "Stripe",
                "retrieve scoped capture evidence",
                response.status,
            ));
        }
        Ok(response.body)
    }
}
// Connected direct charges must not be confused with incoming transfers or
// destination/delegated charges. Required nullable SDK fields must be explicit;
// omitted critical data is manual review, not inferred absence.
fn verify_connected_charge_profile(intent: &Value, gross: i64) -> BlueprintResult<()> {
    let charge = &intent["latest_charge"];
    for (value, fields) in [
        (intent, &["on_behalf_of"][..]),
        (
            charge,
            &["on_behalf_of", "source_transfer", "transfer_data"][..],
        ),
    ] {
        if fields
            .iter()
            .any(|field| value.get(*field) != Some(&Value::Null))
        {
            return Err(mismatch());
        }
    }
    if intent.get("transfer_data").is_some_and(|v| !v.is_null())
        || charge.get("transfer").is_some_and(|v| !v.is_null())
    {
        return Err(mismatch());
    }
    let fee = |value: &Value| match value.get("application_fee_amount") {
        Some(Value::Null) => Some(0),
        Some(value) => value.as_i64().filter(|fee| (0..=gross).contains(fee)),
        None => None,
    };
    let intent_fee = fee(intent).ok_or_else(mismatch)?;
    if fee(charge) != Some(intent_fee)
        || charge
            .get("application_fee")
            .is_some_and(|value| !value.is_null() && !value.as_str().is_some_and(safe_id))
    {
        return Err(mismatch());
    }
    Ok(())
}
fn safe_id(value: &str) -> bool {
    !value
        .trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
        .is_empty()
        && value.len() <= 1024
        && !value.chars().any(char::is_control)
}
// https://docs.stripe.com/api/charges/object: paid includes authorizations and
// refunded=false includes partial refunds. Full amount_captured and zero refunded
// are required by verify_capture. Balance amounts can use a different FX currency:
// https://docs.stripe.com/api/balance_transactions/object; this profile excludes FX.
fn verify_scoped_capture(
    payment: &PaymentDto,
    intent: &Value,
    livemode: bool,
) -> BlueprintResult<(VerifiedCapture, String)> {
    let evidence = verify_capture(payment, intent)?;
    let charge = &intent["latest_charge"];
    let balance = &charge["balance_transaction"];
    let currency = payment.currency.to_ascii_lowercase();
    let balance_id = balance["id"]
        .as_str()
        .filter(|id| safe_id(id))
        .ok_or_else(mismatch)?;
    if intent["livemode"].as_bool() != Some(livemode)
        || charge["livemode"].as_bool() != Some(livemode)
        || intent["amount_capturable"].as_i64() != Some(0)
        || intent.get("canceled_at").is_some_and(|v| !v.is_null())
        || intent["currency"] != currency
        || charge["currency"] != currency
        || !safe_id(&evidence.capture_id)
        || balance["object"] != "balance_transaction"
        || balance["source"].as_str() != Some(evidence.capture_id.as_str())
        || !matches!(balance["type"].as_str(), Some("charge" | "payment"))
        || !matches!(balance["status"].as_str(), Some("pending" | "available"))
        || balance["currency"] != currency
        || balance["amount"].as_i64() != Some(payment.amount_cents)
        || balance.get("exchange_rate").is_some_and(|v| !v.is_null())
    {
        return Err(mismatch());
    }
    Ok((evidence, balance_id.into()))
}

#[cfg(test)]
mod tests {
    use super::{verify_connected_charge_profile, verify_scoped_capture};
    use forklaunch_ecommerce_interface::PaymentDto;
    use serde_json::{Value, json};
    fn fixture() -> (PaymentDto, Value) {
        let payment = serde_json::from_value(json!({"id":"payment","orderId":"order","amountCents":105,"currency":"USD","status":"pending","providerRef":"pi_owned"})).unwrap();
        let intent = json!({"id":"pi_owned","object":"payment_intent","livemode":false,"status":"succeeded","amount":105,"amount_received":105,"amount_capturable":0,"currency":"usd","metadata":{"orderId":"order"},"latest_charge":{"id":"ch_owned","object":"charge","livemode":false,"payment_intent":"pi_owned","status":"succeeded","paid":true,"captured":true,"refunded":false,"disputed":false,"amount":105,"amount_captured":105,"amount_refunded":0,"currency":"usd","balance_transaction":{"id":"txn_owned","object":"balance_transaction","source":"ch_owned","amount":105,"currency":"usd","type":"charge","status":"pending","exchange_rate":null}}});
        (payment, intent)
    }
    #[test]
    fn connected_direct_charge_fee_and_transfer_profile_is_explicit() {
        let (_, mut intent) = fixture();
        intent["on_behalf_of"] = Value::Null;
        intent["application_fee_amount"] = Value::Null;
        for field in [
            "on_behalf_of",
            "source_transfer",
            "transfer_data",
            "application_fee_amount",
        ] {
            intent["latest_charge"][field] = Value::Null;
        }
        assert!(verify_connected_charge_profile(&intent, 105).is_ok());
        for fee in [0, 5, 105] {
            intent["application_fee_amount"] = json!(fee);
            intent["latest_charge"]["application_fee_amount"] = json!(fee);
            intent["latest_charge"]["application_fee"] = json!("fee_owned");
            assert!(verify_connected_charge_profile(&intent, 105).is_ok());
        }
        for invalid in [
            json!(-1),
            json!(106),
            json!(1.5),
            json!("5"),
            json!({}),
            json!(true),
        ] {
            let mut bad = intent.clone();
            bad["application_fee_amount"] = invalid.clone();
            bad["latest_charge"]["application_fee_amount"] = invalid;
            assert!(verify_connected_charge_profile(&bad, 105).is_err());
        }
        for key in [
            "on_behalf_of",
            "source_transfer",
            "transfer_data",
            "application_fee_amount",
        ] {
            let mut bad = intent.clone();
            bad["latest_charge"].as_object_mut().unwrap().remove(key);
            assert!(verify_connected_charge_profile(&bad, 105).is_err());
        }
        intent["application_fee_amount"] = Value::Null;
        intent["latest_charge"]["application_fee_amount"] = json!(0);
        intent["latest_charge"]["application_fee"] = Value::Null;
        assert!(verify_connected_charge_profile(&intent, 105).is_ok());
    }
    #[test]
    fn non_fx_bookkeeping_allows_pending_and_available_not_settlement_claims() {
        let (payment, mut intent) = fixture();
        for status in ["pending", "available"] {
            for kind in ["charge", "payment"] {
                intent["latest_charge"]["balance_transaction"]["status"] = json!(status);
                intent["latest_charge"]["balance_transaction"]["type"] = json!(kind);
                let (capture, balance) = verify_scoped_capture(&payment, &intent, false).unwrap();
                assert_eq!(capture.capture_id, "ch_owned");
                assert_eq!(balance, "txn_owned");
            }
        }
    }
    #[test]
    fn rejects_partial_authorized_refunded_disputed_fx_and_substituted_evidence() {
        let (payment, original) = fixture();
        let cases = [
            ("/livemode", json!(true)),
            ("/amount_capturable", json!(105)),
            ("/amount_received", json!(104)),
            ("/currency", json!("USD")),
            ("/canceled_at", json!(1)),
            ("/metadata/orderId", json!("other")),
            ("/latest_charge/captured", json!(false)),
            ("/latest_charge/paid", json!(false)),
            ("/latest_charge/livemode", json!(true)),
            ("/latest_charge/amount_captured", json!(104)),
            ("/latest_charge/amount_refunded", json!(1)),
            ("/latest_charge/refunded", json!(true)),
            ("/latest_charge/disputed", json!(true)),
            ("/latest_charge/id", json!("\u{feff}")),
            ("/latest_charge/payment_intent", json!("pi_other")),
            ("/latest_charge/balance_transaction", Value::Null),
            ("/latest_charge/balance_transaction", json!("txn_owned")),
            ("/latest_charge/balance_transaction/id", json!("")),
            (
                "/latest_charge/balance_transaction/source",
                json!("ch_other"),
            ),
            (
                "/latest_charge/balance_transaction/source",
                json!({"id":"ch_owned"}),
            ),
            ("/latest_charge/balance_transaction/object", json!("charge")),
            ("/latest_charge/balance_transaction/type", json!("refund")),
            ("/latest_charge/balance_transaction/status", json!("paid")),
            ("/latest_charge/balance_transaction/amount", json!(104)),
            ("/latest_charge/balance_transaction/currency", json!("eur")),
            (
                "/latest_charge/balance_transaction/exchange_rate",
                json!(1.0),
            ),
        ];
        for (pointer, value) in cases {
            let mut intent = original.clone();
            if pointer == "/canceled_at" {
                intent["canceled_at"] = value;
            } else {
                *intent.pointer_mut(pointer).unwrap() = value;
            }
            assert!(
                verify_scoped_capture(&payment, &intent, false).is_err(),
                "{pointer}"
            );
        }
        for (parent, key) in [
            ("", "livemode"),
            ("", "amount_capturable"),
            ("/latest_charge", "livemode"),
            ("/latest_charge", "amount_captured"),
            ("/latest_charge", "amount_refunded"),
            ("/latest_charge", "balance_transaction"),
            ("/latest_charge/balance_transaction", "source"),
            ("/latest_charge/balance_transaction", "amount"),
            ("/latest_charge/balance_transaction", "status"),
            ("/latest_charge/balance_transaction", "type"),
        ] {
            let mut intent = original.clone();
            intent
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                verify_scoped_capture(&payment, &intent, false).is_err(),
                "missing {parent}/{key}"
            );
        }
        let mut fx = original.clone();
        fx["latest_charge"]["balance_transaction"]["amount"] = json!(129);
        fx["latest_charge"]["balance_transaction"]["currency"] = json!("eur");
        fx["latest_charge"]["balance_transaction"]["exchange_rate"] = json!(1.23);
        assert!(
            verify_scoped_capture(&payment, &fx, false).is_err(),
            "legitimate FX stays manual"
        );
    }
}
