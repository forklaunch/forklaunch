//! Explicit direct Portal creation with durable customer authority and local UUID.
//! Session access is not payment/entitlement proof. Default routes are unchanged.
use crate::{client::VerifiedStripeContext, services::StripeBillingService};
use forklaunch_billing_base::services::postgres::{
    PortalCreationClaim, PostgresBillingStore, StripeBindingScope, TrustedPortalPublisher,
};
use forklaunch_billing_interface::services::BillingPortalDto;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use reqwest::Method;
use serde_json::{Value, json};
use uuid::Uuid;

/// Server-authorized configuration and return destination. No arbitrary options,
/// `flow_data`, `customer_account` or `on_behalf_of` delegation in this finite profile.
/// Configuration GETs are observations, not locks on future remote changes.
pub struct BoundPortalCreationProfile {
    pub(crate) scope: StripeBindingScope,
    pub(crate) configuration_id: String,
    pub(crate) return_url: String,
}
impl BoundPortalCreationProfile {
    /// # Errors
    /// Rejects unsafe IDs and non-HTTPS/credential-bearing return destinations.
    pub fn trusted(
        scope: StripeBindingScope,
        configuration_id: String,
        return_url: String,
    ) -> BlueprintResult<Self> {
        if !crate::bound_plan_creation::safe_reference(&configuration_id)
            || return_url.len() > 8192
            || return_url.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        let url = reqwest::Url::parse(&return_url).map_err(|_| invalid())?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(invalid());
        }
        Ok(Self {
            scope,
            configuration_id,
            return_url,
        })
    }
}
/// Fresh creation returns its short-lived URL; creation duplicates expose only IDs.
/// A separately authorized stored-read API may return the original DTO within its
/// unextended local lifetime, without refreshing the URL or proving remote validity.
pub enum BoundPortalCreationOutcome {
    Created {
        operation_id: Uuid,
        record: Box<BillingPortalDto>,
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
        "billing_bound_portal_invalid",
        "Invalid bound portal creation",
    )
}
fn configuration(value: &Value, id: &str, mode: bool) -> BlueprintResult<()> {
    if value["object"] != "billing_portal.configuration"
        || value["id"] != id
        || value["livemode"].as_bool() != Some(mode)
        || value["active"].as_bool() != Some(true)
    {
        return Err(invalid());
    }
    Ok(())
}
fn session(
    value: &Value,
    profile: &BoundPortalCreationProfile,
    customer: &str,
    mode: bool,
) -> BlueprintResult<()> {
    if value["object"] != "billing_portal.session"
        || !value["id"]
            .as_str()
            .is_some_and(crate::bound_plan_creation::safe_reference)
        || value["livemode"].as_bool() != Some(mode)
        || value["customer"] != customer
        || value["configuration"] != profile.configuration_id
        || value["return_url"] != profile.return_url
        || value.get("on_behalf_of") != Some(&Value::Null)
        || value.get("flow") != Some(&Value::Null)
        || value.get("customer_account").is_some_and(|v| !v.is_null())
        || value["created"].as_i64().is_none_or(|v| v <= 0)
        || value["url"].as_str().is_none_or(|v| {
            v.len() > 65_536
                || v.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
                    .is_empty()
        })
    {
        return Err(invalid());
    }
    Ok(())
}
impl StripeBillingService<PostgresBillingStore> {
    /// Validates a selected direct profile and separate publisher before provider I/O.
    /// # Errors
    /// Rejects managed transport or incompatible actor/tenant/database/role authority.
    pub async fn validate_bound_portal_configuration(
        &self,
        publisher: &TrustedPortalPublisher,
    ) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        publisher.validate_for(self.base().store()).await
    }

    /// Trusted caller separately authorizes this local Customer/Party to access
    /// this Portal configuration. One POST only after confirmed reservation.
    /// Uncertain dispatch/COMMIT retains claims; inspect operation, never retry.
    /// Five-minute local expiry is application policy, not a Stripe lifetime promise.
    /// # Errors
    /// Denies managed transport, stale/wrong Customer authority and changed intent.
    #[allow(clippy::too_many_lines)]
    pub async fn create_bound_portal_session(
        &self,
        profile: &BoundPortalCreationProfile,
        publisher: &TrustedPortalPublisher,
        operation: Uuid,
        local_customer_id: Uuid,
    ) -> BlueprintResult<BoundPortalCreationOutcome> {
        let version = self.client().direct_creation_version()?;
        if operation.is_nil() || local_customer_id.is_nil() {
            return Err(invalid());
        }
        publisher.validate_for(self.base().store()).await?;
        let snapshot = self
            .base()
            .store()
            .snapshot_portal_dependencies(&profile.scope, local_customer_id, version)
            .await?;
        let customer = snapshot.provider_customer_id().to_owned();
        let context = self
            .client()
            .verify_context(profile.scope.account_id(), profile.scope.livemode())
            .await?;
        self.client()
            .verify_registration_customer(&context, &customer)
            .await?;
        let config = self
            .client()
            .portal_creation_request(
                &context,
                Method::GET,
                &[
                    "v1",
                    "billing_portal",
                    "configurations",
                    &profile.configuration_id,
                ],
                None,
                &json!({}),
            )
            .await?;
        configuration(&config, &profile.configuration_id, context.livemode())?;
        let fields = json!({"customer":customer,"configuration":profile.configuration_id,"return_url":profile.return_url});
        // Stable identity excludes remote presentation fields and observation time.
        // The trusted profile chooses the configuration, not a mutable default.
        let payload = json!({"profile":"direct_portal_v1","customer":local_customer_id.to_string(),"fields":fields});
        let ticket = match self
            .base()
            .store()
            .reserve_portal_creation(snapshot, operation, &payload, || {
                context.ensure_current(self.client())
            })
            .await
        {
            Ok(PortalCreationClaim::Reserved(ticket)) => ticket,
            Ok(PortalCreationClaim::Completed { public_id, .. }) => {
                return Ok(BoundPortalCreationOutcome::PreviouslyCompleted {
                    operation_id: operation,
                    public_id,
                });
            }
            Err(error)
                if error.code == "billing_provider_binding_conflict"
                    || error.code == "billing_validation" =>
            {
                return Err(error);
            }
            Ok(PortalCreationClaim::Existing(_)) | Err(_) => {
                return Ok(BoundPortalCreationOutcome::Unresolved {
                    operation_id: operation,
                });
            }
        };
        let Ok(provider) = self
            .dispatch_bound_portal(
                &context,
                profile,
                &customer,
                &fields,
                ticket.idempotency_key(),
            )
            .await
        else {
            return Ok(BoundPortalCreationOutcome::Unresolved {
                operation_id: operation,
            });
        };
        let now = chrono::Utc::now();
        let record = json!({"id":ticket.public_id().to_string(),"customerId":local_customer_id.to_string(),"uri":provider["url"],"expiresAt":now+chrono::Duration::minutes(5),"providerFields":provider,"createdAt":now,"updatedAt":now});
        if publisher
            .complete(ticket, record.clone(), || {
                context.ensure_current(self.client())
            })
            .await
            .is_err()
        {
            return Ok(BoundPortalCreationOutcome::Unresolved {
                operation_id: operation,
            });
        }
        if context.ensure_current(self.client()).is_err() {
            return Ok(
                BoundPortalCreationOutcome::RecordedEvidenceNoLongerCurrent {
                    operation_id: operation,
                },
            );
        }
        Ok(BoundPortalCreationOutcome::Created {
            operation_id: operation,
            record: Box::new(serde_json::from_value(record).map_err(|_| invalid())?),
        })
    }
    async fn dispatch_bound_portal(
        &self,
        context: &VerifiedStripeContext,
        profile: &BoundPortalCreationProfile,
        customer: &str,
        fields: &Value,
        key: &str,
    ) -> BlueprintResult<Value> {
        let provider = self
            .client()
            .portal_creation_request(
                context,
                Method::POST,
                &["v1", "billing_portal", "sessions"],
                Some(key),
                fields,
            )
            .await?;
        session(&provider, profile, customer, context.livemode())?;
        Ok(provider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_portal_response_does_not_accept_delegation_or_ambiguous_url() {
        let profile = BoundPortalCreationProfile::trusted(
            StripeBindingScope::administrative("owned".into(), "acct_owned".into(), false).unwrap(),
            "bpc_owned".into(),
            "https://example.test/account".into(),
        )
        .unwrap();
        let good = json!({"id":"bps_owned","object":"billing_portal.session","livemode":false,"customer":"cus_owned","configuration":"bpc_owned","created":1_791_936_000,"flow":null,"on_behalf_of":null,"return_url":"https://example.test/account","url":"https://billing.stripe.com/p/session?secret=private"});
        assert!(session(&good, &profile, "cus_owned", false).is_ok());
        for key in [
            "on_behalf_of",
            "flow",
            "url",
            "configuration",
            "customer",
            "created",
            "livemode",
        ] {
            let mut missing = good.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(
                session(&missing, &profile, "cus_owned", false).is_err(),
                "{key}"
            );
        }
        for value in [
            json!(null),
            json!(7),
            json!({}),
            json!(""),
            json!("\u{feff}"),
        ] {
            let mut bad = good.clone();
            bad["url"] = value;
            assert!(session(&bad, &profile, "cus_owned", false).is_err());
        }
        for url in [
            "http://example.test",
            "https://user:secret@example.test",
            "https://example.test/\nsecret",
        ] {
            assert!(
                BoundPortalCreationProfile::trusted(
                    profile.scope.clone(),
                    "bpc_owned".into(),
                    url.into()
                )
                .is_err()
            );
        }
    }
}
