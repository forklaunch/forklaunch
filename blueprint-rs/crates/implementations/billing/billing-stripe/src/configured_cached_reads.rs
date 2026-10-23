//! The configured TypeScript cache-read hooks, not a provider ownership proof.
use crate::{client::StripeResource, services::StripeBillingService};
use forklaunch_billing_base::services::{BillingKind, BillingStore};
use forklaunch_billing_interface::services::{
    CheckoutSessionDto, PaymentLinkDto, UpdatePaymentLinkDto,
};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use std::sync::OnceLock;

fn invalid() -> BlueprintError {
    BlueprintError::new(
        "billing_configured_mapper_error",
        "Invalid cached provider response",
    )
}
fn provider_error(error: BlueprintError) -> BlueprintError {
    if error.code == "stripe_invalid_response" {
        invalid()
    } else {
        error
    }
}
fn blank(value: &str) -> bool {
    value.trim_matches(|c| matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')).is_empty()
}
// Use the application's existing enum metadata rather than a second currency or
// payment-method catalog. These three enums are identical for both families.
fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        let registry = forklaunch_billing_interface::module_contracts::module_registry(true);
        let route = registry
            .routes()
            .iter()
            .find(|route| route.operation_name == "getPaymentLink")
            .expect("package PaymentLink route");
        let body = route
            .responses
            .iter()
            .find(|response| response.status_code == 200)
            .and_then(|response| response.body.as_ref())
            .expect("package response schema");
        serde_json::from_str(
            body.schema
                .json_schema
                .as_deref()
                .expect("package JSON schema"),
        )
        .expect("package JSON schema parses")
    })
}
fn local_fields(
    id: &str,
    expected: &str,
    currency: &str,
    methods: &[String],
    status: &str,
) -> BlueprintResult<()> {
    let properties = &schema()["properties"];
    let member = |values: &Value, value: &str| {
        values
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(value)))
    };
    if id != expected
        || blank(id)
        || !member(&properties["currency"]["enum"], currency)
        || !member(&properties["status"]["enum"], status)
        || methods
            .iter()
            .any(|method| !member(&properties["paymentMethods"]["items"]["enum"], method))
    {
        return Err(invalid());
    }
    Ok(())
}
fn reference<'a>(value: &'a Value, prefix: &str, object: &str) -> BlueprintResult<(&'a str, bool)> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id, prefix))
        .ok_or_else(invalid)?;
    let mode = value
        .get("livemode")
        .and_then(Value::as_bool)
        .ok_or_else(invalid)?;
    if value.get("object").and_then(Value::as_str) != Some(object) {
        return Err(invalid());
    }
    Ok((id, mode))
}
fn valid_id(id: &str, prefix: &str) -> bool {
    id.len() <= 128
        && id.strip_prefix(prefix).is_some_and(|rest| {
            !rest.is_empty() && rest.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        })
}
fn checkout(value: &Value) -> BlueprintResult<(&str, bool, Option<&str>)> {
    let (id, live) = reference(value, "cs_", "checkout.session")?;
    if value.get("mode").and_then(Value::as_str) != Some("subscription") {
        return Err(invalid());
    }
    let customer = match value.get("customer") {
        Some(Value::Null) => None,
        Some(Value::String(id)) => Some(id.as_str()),
        Some(Value::Object(customer))
            if customer.get("object").and_then(Value::as_str) == Some("customer")
                && customer
                    .get("deleted")
                    .is_none_or(|deleted| deleted == &Value::Bool(false)) =>
        {
            Some(
                customer
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?,
            )
        }
        _ => return Err(invalid()),
    };
    if customer.is_some_and(|id| !valid_id(id, "cus_")) {
        return Err(invalid());
    }
    Ok((id, live, customer))
}
fn validate_link(local: &PaymentLinkDto, expected: &str) -> BlueprintResult<()> {
    local_fields(
        &local.id,
        expected,
        &local.currency,
        &local.payment_methods,
        &local.status,
    )?;
    if !local.amount.is_finite() {
        return Err(invalid());
    }
    reference(
        local.provider_fields.as_ref().ok_or_else(invalid)?,
        "plink_",
        "payment_link",
    )?;
    Ok(())
}
impl<S: BillingStore> StripeBillingService<S> {
    async fn configured_cached_dto<T: serde::de::DeserializeOwned>(
        &self,
        kind: BillingKind,
        id: &str,
    ) -> BlueprintResult<T> {
        // Retain the native store's public ID admission. Cached DTO failures are
        // configured mapper failures, not malformed incoming request bodies.
        if id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
            return Err(BlueprintError::new(
                "billing_validation",
                "Invalid billing input",
            ));
        }
        let raw = self
            .base()
            .store()
            .get_configured_cached(kind, id)
            .await?
            .ok_or_else(invalid)?;
        serde_json::from_value(raw).map_err(|_| invalid())
    }
    /// Returns the checked cached DTO; its stored Stripe id is the dispatch target.
    pub(crate) async fn configured_preflight_payment_link_update(
        &self,
        request: &UpdatePaymentLinkDto,
    ) -> BlueprintResult<PaymentLinkDto> {
        let local: PaymentLinkDto = self
            .configured_cached_dto(BillingKind::PaymentLink, &request.id)
            .await?;
        // This checks the configured cache DTO and patch only; the caller
        // resolves the Stripe target from the stored providerFields id.
        local_fields(
            &local.id,
            &request.id,
            &local.currency,
            &local.payment_methods,
            &local.status,
        )?;
        if !local.amount.is_finite() || request.amount.is_some_and(f64::is_nan) {
            return Err(invalid());
        }
        local_fields(
            &request.id,
            &local.id,
            request.currency.as_deref().unwrap_or(&local.currency),
            request
                .payment_methods
                .as_deref()
                .unwrap_or(&local.payment_methods),
            request.status.as_deref().unwrap_or(&local.status),
        )?;
        Ok(local)
    }
    pub(crate) async fn configured_get_checkout_session(
        &self,
        id: &str,
    ) -> BlueprintResult<CheckoutSessionDto> {
        let mut local: CheckoutSessionDto = self
            .configured_cached_dto(BillingKind::CheckoutSession, id)
            .await?;
        local_fields(
            &local.id,
            id,
            &local.currency,
            &local.payment_methods,
            &local.status,
        )?;
        let before = checkout(local.provider_fields.as_ref().ok_or_else(invalid)?)?;
        let provider = self
            .client()
            .retrieve_configured(StripeResource::CheckoutSession, before.0)
            .await
            .map_err(provider_error)?;
        let after = checkout(&provider)?;
        if before.0 != after.0 || before.1 != after.1 || (before.2.is_some() && before.2 != after.2)
        {
            return Err(invalid());
        }
        local.provider_fields = Some(provider);
        Ok(local)
    }
    pub(crate) async fn configured_get_payment_link(
        &self,
        id: &str,
    ) -> BlueprintResult<PaymentLinkDto> {
        let local: PaymentLinkDto = self
            .configured_cached_dto(BillingKind::PaymentLink, id)
            .await?;
        validate_link(&local, id)?;
        self.enrich_payment_link(local).await
    }
    async fn enrich_payment_link(
        &self,
        mut local: PaymentLinkDto,
    ) -> BlueprintResult<PaymentLinkDto> {
        let before = reference(
            local.provider_fields.as_ref().ok_or_else(invalid)?,
            "plink_",
            "payment_link",
        )?;
        let provider = self
            .client()
            .retrieve_configured(StripeResource::PaymentLink, before.0)
            .await
            .map_err(provider_error)?;
        let after = reference(&provider, "plink_", "payment_link")?;
        if before != after {
            return Err(invalid());
        }
        local.provider_fields = Some(provider);
        Ok(local)
    }
    pub(crate) async fn configured_list_payment_links(
        &self,
        ids: Option<&[String]>,
    ) -> BlueprintResult<Vec<PaymentLinkDto>> {
        // The store admits the complete key selection before reading/decrypting
        // concrete-store values. Validate every local DTO before the first GET.
        let selected = self
            .base()
            .store()
            .list_configured_cached(ids, self.max_configured_list_records)
            .await?;
        let mut locals = Vec::with_capacity(selected.len());
        for (id, raw) in selected {
            let local: PaymentLinkDto = serde_json::from_value(raw).map_err(|_| invalid())?;
            validate_link(&local, &id)?;
            locals.push(local);
        }
        let mut result = Vec::new();
        for local in locals {
            let fresh = self.enrich_payment_link(local).await?;
            let active = fresh
                .provider_fields
                .as_ref()
                .and_then(|fields| fields.get("active"))
                .and_then(Value::as_bool)
                .ok_or_else(invalid)?;
            if active {
                result.push(fresh);
            }
        }
        Ok(result)
    }
}
