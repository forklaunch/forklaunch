//! Fixed-kind managed creation RPCs. Never route through the raw Stripe proxy.
use super::{BillingTransport, StripeBillingClient};
use crate::bound_creation::invalid;
use forklaunch_core::domain::BlueprintResult;
use reqwest::Method;
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Immutable client-bound observation, not a provider dispatch capability.
pub(crate) struct ManagedCreationContext {
    generation: Arc<()>,
    started: Instant,
    scope: Value,
    configuration_id: String,
    profile_id: String,
}
impl ManagedCreationContext {
    pub(crate) fn ensure_current(&self, client: &StripeBillingClient) -> BlueprintResult<()> {
        if !Arc::ptr_eq(&self.generation, &client.config_generation)
            || self.started.elapsed() >= Duration::from_secs(5)
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(crate) fn scope(&self) -> &Value {
        &self.scope
    }
    pub(crate) fn configuration_id(&self) -> &str {
        &self.configuration_id
    }
    pub(crate) fn matches(&self, envelope: &Value) -> BlueprintResult<()> {
        for key in [
            "instanceId",
            "organizationId",
            "accountId",
            "livemode",
            "apiVersion",
            "bindingGeneration",
        ] {
            if envelope[key] != self.scope[key] {
                return Err(invalid());
            }
        }
        if envelope["configurationId"] != self.configuration_id
            || envelope["profileId"] != self.profile_id
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn identity(
    scope: &Value,
    instance: &str,
    account: &str,
    mode: bool,
    version: &str,
) -> BlueprintResult<Value> {
    if scope["version"] != 1
        || scope["instanceId"] != instance
        || scope["accountId"] != account
        || scope["livemode"].as_bool() != Some(mode)
        || scope["apiVersion"] != version
        || scope["organizationId"]
            .as_str()
            .is_none_or(|v| v.trim().is_empty() || v.len() > 256)
        || scope["bindingGeneration"]
            .as_str()
            .is_none_or(|v| uuid::Uuid::parse_str(v).is_err())
        || scope["issuedAt"]
            .as_i64()
            .zip(scope["expiresAt"].as_i64())
            .is_none_or(|(a, b)| a < 0 || b.checked_sub(a) != Some(5000))
    {
        return Err(invalid());
    }
    Ok(
        json!({"version":1,"instanceId":scope["instanceId"],"organizationId":scope["organizationId"],"accountId":account,"livemode":mode,"apiVersion":version,"bindingGeneration":scope["bindingGeneration"]}),
    )
}
impl StripeBillingClient {
    pub(crate) fn managed_creation_version(&self) -> BlueprintResult<&str> {
        if !matches!(self.transport, BillingTransport::Managed(_)) {
            return Err(invalid());
        }
        Ok(self.api_version.as_str())
    }
    pub(crate) async fn managed_creation_context(
        &self,
        account: &str,
        mode: bool,
        profile: &str,
    ) -> BlueprintResult<ManagedCreationContext> {
        let BillingTransport::Managed(gateway) = &self.transport else {
            return Err(invalid());
        };
        let started = Instant::now();
        let response = gateway.request_json(Method::POST,&["payments","operations","payment-link-context"],Some(&json!({"accountId":account,"livemode":mode,"apiVersion":self.api_version.as_str(),"profileId":profile}))).await?;
        if !(200..300).contains(&response.status) || response.body["profileId"] != profile {
            return Err(invalid());
        }
        let configuration_id = response.body["configurationId"]
            .as_str()
            .filter(|v| {
                v.len() == 64
                    && v.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            .ok_or_else(invalid)?
            .to_owned();
        let scope = identity(
            &response.body["scope"],
            gateway.instance_id(),
            account,
            mode,
            self.api_version.as_str(),
        )?;
        let result = ManagedCreationContext {
            generation: self.config_generation.clone(),
            started,
            scope,
            configuration_id,
            profile_id: profile.into(),
        };
        result.ensure_current(self)?;
        Ok(result)
    }
    pub(crate) async fn create_managed_payment_link(
        &self,
        context: &ManagedCreationContext,
        dispatch: forklaunch_billing_base::services::postgres::PaymentLinkCreationReservation,
        envelope: &Value,
    ) -> BlueprintResult<Option<Value>> {
        if envelope["publicId"] != dispatch.public_id().to_string() {
            return Err(invalid());
        }
        context.ensure_current(self)?;
        context.matches(envelope)?;
        let BillingTransport::Managed(gateway) = &self.transport else {
            return Err(invalid());
        };
        let mut request = envelope.as_object().ok_or_else(invalid)?.clone();
        request.remove("instanceId");
        request.remove("organizationId");
        let response = gateway
            .request_json(
                Method::POST,
                &["payments", "operations", "payment-link-create"],
                Some(&Value::Object(request)),
            )
            .await?;
        context.ensure_current(self)?;
        if !(200..300).contains(&response.status) {
            return Err(invalid());
        }
        receipt(&response.body, envelope)
    }
    pub(crate) async fn inspect_managed_payment_link(
        &self,
        context: &ManagedCreationContext,
        envelope: &Value,
    ) -> BlueprintResult<Option<Value>> {
        context.ensure_current(self)?;
        context.matches(envelope)?;
        let BillingTransport::Managed(gateway) = &self.transport else {
            return Err(invalid());
        };
        let response = gateway
            .request_json(
                Method::POST,
                &["payments", "operations", "payment-link-inspect"],
                Some(&json!({"operationId":envelope["operationId"]})),
            )
            .await?;
        context.ensure_current(self)?;
        if !(200..300).contains(&response.status)
            || response.body["currentConfigurationId"] != context.configuration_id
            || response.body["currentProfileId"] != context.profile_id
        {
            return Err(invalid());
        }
        let current = identity(
            &response.body["scope"],
            gateway.instance_id(),
            context.scope["accountId"].as_str().ok_or_else(invalid)?,
            context.scope["livemode"].as_bool().ok_or_else(invalid)?,
            self.api_version.as_str(),
        )?;
        if current != context.scope {
            return Err(invalid());
        }
        if response.body["operation"].is_null() {
            return Ok(None);
        }
        receipt(&response.body["operation"], envelope)
    }
}
fn receipt(response: &Value, envelope: &Value) -> BlueprintResult<Option<Value>> {
    if response["operationId"] != envelope["operationId"] {
        return Err(invalid());
    }
    if response["state"] == "unresolved" {
        return Ok(None);
    }
    if response["state"] != "completed" || response["envelope"] != *envelope {
        return Err(invalid());
    }
    let provider = &response["provider"];
    if provider["object"] != "payment_link"
        || provider["livemode"] != envelope["livemode"]
        || provider["id"].as_str().is_none_or(|id| {
            !id.starts_with("plink_")
                || id.len() <= 6
                || id.len() > 128
                || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
    {
        return Err(invalid());
    }
    Ok(Some(provider.clone()))
}
