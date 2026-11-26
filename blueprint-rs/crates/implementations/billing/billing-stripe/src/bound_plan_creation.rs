//! Explicit direct creation of a local UUID and a separately sealed Plan binding.
//! Unit amount is a declared price per unit, not an invoice or charged total.
use crate::{client::VerifiedStripeContext, services::StripeBillingService};
use forklaunch_billing_base::services::postgres::{
    PlanCreationClaim, PlanCreationStatus, PostgresBillingStore, StripeBindingScope,
};
use forklaunch_billing_interface::services::PlanDto;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use uuid::Uuid;

/// Server configuration, never inferred from public DTOs or mutable record fields.
pub struct BoundPlanCreationProfile {
    pub(crate) scope: StripeBindingScope,
    pub(crate) products: BTreeSet<String>,
}
impl BoundPlanCreationProfile {
    /// # Errors
    /// Rejects an empty/oversized product allowlist or unsafe URL identifiers.
    pub fn trusted(scope: StripeBindingScope, products: Vec<String>) -> BlueprintResult<Self> {
        if products.is_empty()
            || products.len() > 1000
            || products.iter().any(|id| !safe_reference(id))
        {
            return Err(invalid());
        }
        Ok(Self {
            scope,
            products: products.into_iter().collect(),
        })
    }
}
/// Finite per-unit, licensed, single-interval profile. No caller-generated IDs,
/// external IDs, arbitrary provider options, delegated ownership or nested creation.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundPlanCreateRequest {
    pub name: String,
    pub description: Option<String>,
    pub features: Option<Vec<String>>,
    pub active: bool,
    pub unit_amount: u64,
    pub currency: String,
    pub cadence: String,
    pub product_id: String,
}
/// Historical outcome; unresolved may already have created a provider Plan.
pub enum BoundPlanCreationOutcome {
    Completed {
        operation_id: Uuid,
        record: Box<PlanDto>,
    },
    Unresolved {
        operation_id: Uuid,
    },
    RecordedEvidenceNoLongerCurrent {
        operation_id: Uuid,
    },
}
fn invalid() -> BlueprintError {
    BlueprintError::new("billing_bound_plan_invalid", "Invalid bound plan creation")
}
pub(crate) fn safe_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
pub(crate) fn fields(
    profile: &BoundPlanCreationProfile,
    request: &BoundPlanCreateRequest,
) -> BlueprintResult<Value> {
    if !profile.products.contains(&request.product_id)
        || request
            .name
            .trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
            .is_empty()
        || request.name.len() > 1000
        || request
            .description
            .as_ref()
            .is_some_and(|v| v.len() > 10000)
        || request
            .features
            .as_ref()
            .is_some_and(|v| v.len() > 1000 || v.iter().any(|v| v.len() > 1000))
        || request.unit_amount > 9_007_199_254_740_991
        || request.currency.len() != 3
        || !request.currency.bytes().all(|b| b.is_ascii_uppercase())
        || !matches!(request.cadence.as_str(), "day" | "week" | "month" | "year")
    {
        return Err(invalid());
    }
    Ok(
        json!({"product":request.product_id,"amount":request.unit_amount,"currency":request.currency.to_ascii_lowercase(),"interval":request.cadence,"interval_count":1,"billing_scheme":"per_unit","usage_type":"licensed","active":request.active}),
    )
}
pub(crate) fn product(value: &Value, expected: &str, mode: bool) -> BlueprintResult<()> {
    if value["object"] != "product"
        || value["id"] != expected
        || value["livemode"].as_bool() != Some(mode)
        || value
            .get("deleted")
            .is_some_and(|v| v.as_bool() != Some(false))
    {
        return Err(invalid());
    }
    Ok(())
}
pub(crate) fn plan(
    value: &Value,
    expected: Option<&str>,
    request: &BoundPlanCreateRequest,
    mode: bool,
) -> BlueprintResult<String> {
    let id = value["id"]
        .as_str()
        .filter(|id| safe_reference(id))
        .ok_or_else(invalid)?;
    if expected.is_some_and(|expected| id != expected)
        || value["object"] != "plan"
        || value["livemode"].as_bool() != Some(mode)
        || value["amount"].as_u64() != Some(request.unit_amount)
        || value["currency"] != request.currency.to_ascii_lowercase()
        || value["interval"] != request.cadence
        || value["interval_count"].as_u64() != Some(1)
        || value["billing_scheme"] != "per_unit"
        || value["usage_type"] != "licensed"
        || value["active"].as_bool() != Some(request.active)
    {
        return Err(invalid());
    }
    if let Some(id) = value["product"].as_str() {
        if id != request.product_id {
            return Err(invalid());
        }
    } else {
        product(&value["product"], &request.product_id, mode)?;
    }
    Ok(id.into())
}
fn unresolved(operation_id: Uuid) -> BoundPlanCreationOutcome {
    BoundPlanCreationOutcome::Unresolved { operation_id }
}
fn completed(operation_id: Uuid, record: Value) -> BlueprintResult<BoundPlanCreationOutcome> {
    Ok(BoundPlanCreationOutcome::Completed {
        operation_id,
        record: Box::new(serde_json::from_value(record).map_err(|_| invalid())?),
    })
}
impl StripeBillingService<PostgresBillingStore> {
    /// Validates explicit direct creation's local grants, without provider I/O.
    /// # Errors
    /// Rejects managed transport or missing ordinary-role journal/write privileges.
    pub async fn validate_bound_plan_creation_configuration(&self) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        self.base().store().validate_plan_creation_access().await
    }

    /// Durable intent precedes network effects. Only a newly committed ticket can
    /// dispatch; repeated pending operations never POST. No default route selects it.
    /// # Errors
    /// Rejects invalid input/configuration, managed clients and operation conflicts.
    pub async fn create_bound_plan(
        &self,
        profile: &BoundPlanCreationProfile,
        operation: Uuid,
        request: BoundPlanCreateRequest,
    ) -> BlueprintResult<BoundPlanCreationOutcome> {
        let version = self.client().direct_creation_version()?;
        if operation.is_nil() {
            return Err(invalid());
        }
        let fields = fields(profile, &request)?;
        let payload =
            json!({"request":request,"providerFields":fields,"trustedProducts":profile.products});
        let ticket = match self
            .base()
            .store()
            .reserve_plan_creation(&profile.scope, operation, version, &payload)
            .await
        {
            Ok(PlanCreationClaim::Reserved(ticket)) => ticket,
            Ok(PlanCreationClaim::Existing(PlanCreationStatus::Pending { .. })) => {
                return Ok(unresolved(operation));
            }
            Ok(PlanCreationClaim::Existing(PlanCreationStatus::Completed { record, .. })) => {
                return completed(operation, record);
            }
            Err(error)
                if error.code == "billing_provider_binding_conflict"
                    || error.code == "billing_validation" =>
            {
                return Err(error);
            }
            Err(_) => return Ok(unresolved(operation)),
        };
        let Ok(context) = self
            .client()
            .verify_context(profile.scope.account_id(), profile.scope.livemode())
            .await
        else {
            return Ok(unresolved(operation));
        };
        let Ok(provider) = self
            .create_verified_plan(&context, &request, &fields, ticket.idempotency_key())
            .await
        else {
            return Ok(unresolved(operation));
        };
        let now = chrono::Utc::now();
        let mut record = json!({"id":ticket.public_id().to_string(),"name":request.name,"description":request.description,"features":request.features,"active":request.active,"price":request.unit_amount,"currency":request.currency,"cadence":request.cadence,"externalId":provider["id"],"billingProvider":"stripe","providerFields":provider["product"],"createdAt":now,"updatedAt":now});
        for field in ["description", "features"] {
            if record[field].is_null() {
                record.as_object_mut().ok_or_else(invalid)?.remove(field);
            }
        }
        if self
            .base()
            .store()
            .complete_plan_creation(ticket, record.clone(), provider, || {
                context.ensure_current(self.client())
            })
            .await
            .is_err()
        {
            return Ok(unresolved(operation));
        }
        if context.ensure_current(self.client()).is_err() {
            return Ok(BoundPlanCreationOutcome::RecordedEvidenceNoLongerCurrent {
                operation_id: operation,
            });
        }
        completed(operation, record)
    }
    async fn create_verified_plan(
        &self,
        context: &VerifiedStripeContext,
        request: &BoundPlanCreateRequest,
        fields: &Value,
        key: &str,
    ) -> BlueprintResult<Value> {
        let owned = self
            .client()
            .plan_creation_request(
                context,
                Method::GET,
                &["v1", "products", &request.product_id],
                None,
                &json!({}),
            )
            .await?;
        product(&owned, &request.product_id, context.livemode())?;
        let created = self
            .client()
            .plan_creation_request(context, Method::POST, &["v1", "plans"], Some(key), fields)
            .await?;
        let id = plan(&created, None, request, context.livemode())?;
        let readback = self
            .client()
            .plan_creation_request(
                context,
                Method::GET,
                &["v1", "plans", &id],
                None,
                &json!({"expand":["product"]}),
            )
            .await?;
        plan(&readback, Some(&id), request, context.livemode())?;
        // Expanded product must still be an authenticated, non-deleted same-mode object.
        product(
            &readback["product"],
            &request.product_id,
            context.livemode(),
        )?;
        Ok(readback)
    }
    /// Historical inspection only. Pending operations cannot be automatically
    /// retried or reconstructed from mutable provider fields/cache entries.
    /// # Errors
    /// Denies foreign tenant/actor, encryption/storage or malformed receipts.
    pub async fn inspect_bound_plan_creation(
        &self,
        operation: Uuid,
    ) -> BlueprintResult<Option<BoundPlanCreationOutcome>> {
        self.base()
            .store()
            .inspect_plan_creation(operation)
            .await?
            .map(|status| match status {
                PlanCreationStatus::Pending { .. } => Ok(unresolved(operation)),
                PlanCreationStatus::Completed { record, .. } => completed(operation, record),
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_profile_rejects_unsupported_identity_and_price_options() {
        let scope =
            StripeBindingScope::administrative("test".into(), "acct_test".into(), false).unwrap();
        let profile =
            BoundPlanCreationProfile::trusted(scope.clone(), vec!["custom-product".into()])
                .unwrap();
        let value = json!({"name":"Plan","active":true,"unitAmount":1200,"currency":"USD","cadence":"month","productId":"custom-product"});
        assert!(fields(&profile, &serde_json::from_value(value.clone()).unwrap()).is_ok());
        for (key, bad) in [
            ("productId", json!("not-allowed")),
            ("currency", json!("usd")),
            ("cadence", json!("quarter")),
            ("unitAmount", json!(9_007_199_254_740_992_u64)),
            ("name", json!("\u{feff}")),
        ] {
            let mut input = value.clone();
            input[key] = bad;
            assert!(fields(&profile, &serde_json::from_value(input).unwrap()).is_err());
        }
        for field in ["id", "externalId", "providerFields", "billingProvider"] {
            let mut input = value.clone();
            input[field] = json!("caller-controlled");
            assert!(serde_json::from_value::<BoundPlanCreateRequest>(input).is_err());
        }
        for id in [
            "",
            "../other",
            "custom/product",
            "id?query",
            "id%2Fother",
            "\u{feff}",
        ] {
            assert!(BoundPlanCreationProfile::trusted(scope.clone(), vec![id.into()]).is_err());
        }
    }
}
