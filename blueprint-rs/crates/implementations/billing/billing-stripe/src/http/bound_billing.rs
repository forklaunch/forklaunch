//! One explicit finite selector for compatible domain-specific authority profiles.
use super::{
    BillingBackend, BoundCheckoutConfig, BoundCheckoutV1, BoundPortalV1, bound_checkout_contracts,
    bound_plan_read_contracts, bound_portal_contracts, bound_subscription_read_contracts,
};
use crate::bound_creation::BoundPaymentLinkCreationProfile;
use crate::bound_payment_link_expiration::BoundPaymentLinkExpirationProfile;
use crate::bound_plan_creation::BoundPlanCreationProfile;
use crate::bound_portal_creation::BoundPortalCreationProfile;
use crate::bound_subscription_creation::{
    BoundSubscriptionCreationProfile, SubscriptionCollectionPolicy,
};
use forklaunch_billing_base::services::postgres::{
    StripeBindingScope, TrustedCheckoutPublisher, TrustedPaymentLinkExpirationPublisher,
    TrustedPortalPublisher, TrustedSubscriptionPublisher,
};
use forklaunch_core::contract::RouteContract;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use uuid::Uuid;

pub(super) fn configuration() -> BlueprintError {
    BlueprintError::new(
        "billing_configuration",
        "Invalid bound billing configuration",
    )
}
/// Server-owned finite Portal policy. No credentials or raw provider options.
pub struct BoundPortalConfig {
    pub(super) profile: BoundPortalCreationProfile,
}
/// Shared scope, optional explicitly selected domains; no Debug/Serde or credentials.
pub(super) struct PaymentLinkReadConfig {
    pub(super) profile: BoundPaymentLinkCreationProfile,
    pub(super) max_records: usize,
}
pub struct BoundBillingConfig {
    checkout: Option<BoundCheckoutConfig>,
    portal: Option<BoundPortalConfig>,
    portal_local_expiration: Option<BoundPortalConfig>,
    plan_read: Option<BoundPlanCreationProfile>,
    subscription_read: Option<BoundSubscriptionCreationProfile>,
    payment_link_read: Option<PaymentLinkReadConfig>,
    payment_link: Option<BoundPaymentLinkCreationProfile>,
    payment_link_expiration: Option<BoundPaymentLinkExpirationProfile>,
    payment_link_expiration_recovery: Option<BoundPaymentLinkExpirationProfile>,
    plan: Option<BoundPlanCreationProfile>,
    subscription: Option<BoundSubscriptionCreationProfile>,
    subscription_recovery: Option<BoundSubscriptionCreationProfile>,
}
fn scope(value: &Value) -> BlueprintResult<StripeBindingScope> {
    StripeBindingScope::administrative(
        value["deploymentLabel"]
            .as_str()
            .ok_or_else(configuration)?
            .to_owned(),
        value["accountId"]
            .as_str()
            .ok_or_else(configuration)?
            .to_owned(),
        value["livemode"].as_bool().ok_or_else(configuration)?,
    )
    .map_err(|_| configuration())
}
fn exact(value: &Value, required: &[&str], optional: &[&str]) -> BlueprintResult<()> {
    let object = value.as_object().ok_or_else(configuration)?;
    if required.iter().any(|key| !object.contains_key(*key))
        || object
            .keys()
            .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        return Err(configuration());
    }
    Ok(())
}
fn subscription_profile(
    scope: &StripeBindingScope,
    part: &Value,
) -> BlueprintResult<BoundSubscriptionCreationProfile> {
    exact(part, &["plans", "collectionPolicy"], &[])?;
    if part["collectionPolicy"] != "automatic_default_incomplete_v1" {
        return Err(configuration());
    }
    let rows = part["plans"].as_array().ok_or_else(configuration)?;
    if rows.is_empty() || rows.len() > 1000 {
        return Err(configuration());
    }
    let mut plans = Vec::with_capacity(rows.len());
    for row in rows {
        exact(row, &["localPlanId", "creationOperationId"], &[])?;
        let id = |key: &str| {
            Uuid::parse_str(row[key].as_str().ok_or_else(configuration)?)
                .map_err(|_| configuration())
        };
        plans.push((id("localPlanId")?, id("creationOperationId")?));
    }
    BoundSubscriptionCreationProfile::trusted(
        scope.clone(),
        plans,
        SubscriptionCollectionPolicy::AutomaticDefaultIncomplete,
    )
    .map_err(|_| configuration())
}
fn subscription_creation_profile(
    scope: &StripeBindingScope,
    part: &Value,
) -> BlueprintResult<BoundSubscriptionCreationProfile> {
    exact(part, &["plans", "collectionPolicy", "authority"], &[])?;
    if part["authority"] != "internal_service_tenant_v1" {
        return Err(configuration());
    }
    let mut selected = part.clone();
    selected
        .as_object_mut()
        .ok_or_else(configuration)?
        .remove("authority");
    subscription_profile(scope, &selected)
}
fn payment_link_profile(
    scope: &StripeBindingScope,
    part: &Value,
) -> BlueprintResult<PaymentLinkReadConfig> {
    exact(part, &["trustedPrices"], &["maxRecords"])?;
    let prices: Vec<String> =
        serde_json::from_value(part["trustedPrices"].clone()).map_err(|_| configuration())?;
    let max_records = match part.get("maxRecords") {
        Some(value) => value
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .filter(|v| (1..=1000).contains(v))
            .ok_or_else(configuration)?,
        None => 100,
    };
    Ok(PaymentLinkReadConfig {
        profile: BoundPaymentLinkCreationProfile::trusted(scope.clone(), prices)
            .map_err(|_| configuration())?,
        max_records,
    })
}
fn payment_link_creation_profile(
    scope: &StripeBindingScope,
    part: &Value,
) -> BlueprintResult<BoundPaymentLinkCreationProfile> {
    exact(part, &["trustedPrices"], &[])?;
    let prices: Vec<String> =
        serde_json::from_value(part["trustedPrices"].clone()).map_err(|_| configuration())?;
    BoundPaymentLinkCreationProfile::trusted(scope.clone(), prices).map_err(|_| configuration())
}
fn payment_link_expiration_profile(
    scope: &StripeBindingScope,
    part: &Value,
) -> BlueprintResult<BoundPaymentLinkExpirationProfile> {
    exact(part, &["trustedPrices", "authority"], &[])?;
    if part["authority"] != "internal_service_tenant_v1" {
        return Err(configuration());
    }
    let prices: Vec<String> =
        serde_json::from_value(part["trustedPrices"].clone()).map_err(|_| configuration())?;
    BoundPaymentLinkExpirationProfile::trusted(scope.clone(), prices).map_err(|_| configuration())
}
fn plan_profile(
    scope: &StripeBindingScope,
    part: &Value,
) -> BlueprintResult<BoundPlanCreationProfile> {
    exact(part, &["trustedProducts"], &[])?;
    let products: Vec<String> =
        serde_json::from_value(part["trustedProducts"].clone()).map_err(|_| configuration())?;
    BoundPlanCreationProfile::trusted(scope.clone(), products).map_err(|_| configuration())
}
fn portal_profile(scope: &StripeBindingScope, part: &Value) -> BlueprintResult<BoundPortalConfig> {
    exact(part, &["configurationId", "returnUrl"], &[])?;
    Ok(BoundPortalConfig {
        profile: BoundPortalCreationProfile::trusted(
            scope.clone(),
            part["configurationId"]
                .as_str()
                .ok_or_else(configuration)?
                .to_owned(),
            part["returnUrl"]
                .as_str()
                .ok_or_else(configuration)?
                .to_owned(),
        )
        .map_err(|_| configuration())?,
    })
}
fn checkout_profile(value: &Value) -> BlueprintResult<Option<BoundCheckoutConfig>> {
    let Some(part) = value.get("checkout") else {
        return Ok(None);
    };
    exact(part, &["plans", "successUrl", "cancelUrl"], &[])?;
    let mut legacy = part.clone();
    for key in ["deploymentLabel", "accountId", "livemode"] {
        legacy[key] = value[key].clone();
    }
    BoundCheckoutConfig::from_json(&legacy.to_string())
        .map(Some)
        .map_err(|_| configuration())
}

fn matching_expiration_prices(value: &Value) -> BlueprintResult<()> {
    let (Some(left), Some(right)) = (
        value.get("paymentLinkExpiration"),
        value.get("paymentLinkExpirationRecovery"),
    ) else {
        return Ok(());
    };
    let prices = |value: &Value| -> BlueprintResult<std::collections::BTreeSet<String>> {
        value
            .get("trustedPrices")
            .and_then(Value::as_array)
            .ok_or_else(configuration)?
            .iter()
            .map(|v| v.as_str().map(str::to_owned).ok_or_else(configuration))
            .collect()
    };
    if prices(left)? != prices(right)? {
        return Err(configuration());
    }
    Ok(())
}

impl BoundBillingConfig {
    /// Parses at most 64 KiB of exact configuration. At least one domain is required.
    /// # Errors
    /// Rejects unknown/null/empty domains, malformed scope, redirects or provenance.
    pub fn from_json(text: &str) -> BlueprintResult<Self> {
        if text.len() > 65_536 {
            return Err(configuration());
        }
        let value: Value = serde_json::from_str(text).map_err(|_| configuration())?;
        exact(
            &value,
            &["deploymentLabel", "accountId", "livemode"],
            &[
                "checkout",
                "portal",
                "portalLocalExpiration",
                "planRead",
                "subscriptionRead",
                "paymentLinkRead",
                "paymentLink",
                "paymentLinkExpiration",
                "paymentLinkExpirationRecovery",
                "plan",
                "subscription",
                "subscriptionRecovery",
            ],
        )?;
        let scope = scope(&value)?;
        let checkout = checkout_profile(&value)?;
        let portal_config = |key| {
            value
                .get(key)
                .map(|part| portal_profile(&scope, part))
                .transpose()
        };
        let portal = portal_config("portal")?;
        let portal_local_expiration = portal_config("portalLocalExpiration")?;
        let plan_read = value
            .get("planRead")
            .map(|part| plan_profile(&scope, part))
            .transpose()?;
        let subscription_read = value
            .get("subscriptionRead")
            .map(|part| subscription_profile(&scope, part))
            .transpose()?;
        let subscription_recovery = value
            .get("subscriptionRecovery")
            .map(|part| subscription_creation_profile(&scope, part))
            .transpose()?;
        let subscription = value
            .get("subscription")
            .map(|part| subscription_creation_profile(&scope, part))
            .transpose()?;
        let plan = value
            .get("plan")
            .map(|part| plan_profile(&scope, part))
            .transpose()?;
        let payment_link = value
            .get("paymentLink")
            .map(|part| payment_link_creation_profile(&scope, part))
            .transpose()?;
        let payment_link_expiration = value
            .get("paymentLinkExpiration")
            .map(|part| payment_link_expiration_profile(&scope, part))
            .transpose()?;
        let payment_link_expiration_recovery = value
            .get("paymentLinkExpirationRecovery")
            .map(|part| payment_link_expiration_profile(&scope, part))
            .transpose()?;
        matching_expiration_prices(&value)?;
        let payment_link_read = value
            .get("paymentLinkRead")
            .map(|part| payment_link_profile(&scope, part))
            .transpose()?;
        if checkout.is_none()
            && portal.is_none()
            && portal_local_expiration.is_none()
            && plan_read.is_none()
            && subscription_read.is_none()
            && payment_link_read.is_none()
            && payment_link.is_none()
            && payment_link_expiration.is_none()
            && payment_link_expiration_recovery.is_none()
            && plan.is_none()
            && subscription.is_none()
            && subscription_recovery.is_none()
        {
            return Err(configuration());
        }
        Ok(Self {
            checkout,
            portal,
            portal_local_expiration,
            plan_read,
            subscription_read,
            payment_link_read,
            payment_link,
            payment_link_expiration,
            payment_link_expiration_recovery,
            plan,
            subscription,
            subscription_recovery,
        })
    }
    #[must_use]
    pub const fn has_checkout(&self) -> bool {
        self.checkout.is_some()
    }
    #[must_use]
    pub const fn has_portal(&self) -> bool {
        self.portal.is_some()
    }
    #[must_use]
    pub const fn has_plan_read(&self) -> bool {
        self.plan_read.is_some()
    }
    #[must_use]
    pub const fn has_subscription_read(&self) -> bool {
        self.subscription_read.is_some()
    }
    #[must_use]
    pub const fn has_payment_link_read(&self) -> bool {
        self.payment_link_read.is_some()
    }
    #[must_use]
    pub const fn has_subscription_recovery(&self) -> bool {
        self.subscription_recovery.is_some()
    }
    #[must_use]
    pub const fn has_subscription(&self) -> bool {
        self.subscription.is_some()
    }
    #[must_use]
    pub const fn has_plan(&self) -> bool {
        self.plan.is_some()
    }
    #[must_use]
    pub const fn has_payment_link(&self) -> bool {
        self.payment_link.is_some()
    }
    #[must_use]
    pub const fn has_payment_link_expiration(&self) -> bool {
        self.payment_link_expiration.is_some()
    }
    #[must_use]
    pub const fn has_payment_link_expiration_recovery(&self) -> bool {
        self.payment_link_expiration_recovery.is_some()
    }
    /// Checkout/Portal/Subscription creation and `PaymentLink` expiration require separate publication connections.
    /// `PaymentLink` uses its existing ordinary-role immutable journal authority.
    #[must_use]
    pub const fn requires_publisher(&self) -> bool {
        self.has_checkout()
            || self.has_portal()
            || self.has_subscription()
            || self.has_subscription_recovery()
            || self.has_payment_link_expiration()
            || self.has_payment_link_expiration_recovery()
    }
    /// Shared runtime/artifact contracts, with no server configuration embedded.
    #[must_use]
    pub fn contracts(&self) -> Vec<RouteContract> {
        let mut routes = Vec::new();
        if self.has_subscription() {
            routes.extend(super::bound_subscription::bound_subscription_contracts());
        }
        if self.has_subscription_recovery() {
            routes.extend(
                super::bound_subscription::bound_subscription_recovery_contracts()
                    .into_iter()
                    .filter(|r| {
                        !self.has_subscription()
                            || r.operation_name != "inspectBoundSubscriptionOperation"
                    }),
            );
        }
        if self.has_plan() {
            routes.extend(super::bound_plan::bound_plan_contracts());
        }
        if self.has_payment_link_expiration() {
            routes.extend(
                super::bound_payment_link_expiration::bound_payment_link_expiration_contracts(),
            );
        }
        if self.has_payment_link_expiration_recovery() {
            routes.extend(super::bound_payment_link_expiration::bound_payment_link_expiration_recovery_contracts().into_iter().filter(|r|!self.has_payment_link_expiration()||r.operation_name!="inspectBoundPaymentLinkExpiration"));
        }
        if self.has_payment_link() {
            routes.extend(super::bound_payment_link::bound_payment_link_contracts());
        }
        if self.has_checkout() {
            routes.extend(bound_checkout_contracts());
        }
        if self.has_portal() {
            routes.extend(bound_portal_contracts());
        }
        if self.portal_local_expiration.is_some() {
            routes.extend(super::bound_portal_expiration::bound_portal_expiration_contracts());
        }
        if self.has_plan_read() {
            routes.extend(bound_plan_read_contracts());
        }
        if self.has_subscription_read() {
            routes.extend(bound_subscription_read_contracts());
        }
        if let Some(profile) = &self.payment_link_read {
            routes.extend(super::bound_payment_link_read::contracts(
                profile.max_records,
            ));
        }
        routes
    }
}
/// Explicit independent capabilities. Checkout/Portal/Subscription creation require publishers.
pub struct BoundBillingV1 {
    checkout: Option<BoundCheckoutV1>,
    portal: Option<BoundPortalV1>,
    portal_local_expiration: Option<BoundPortalConfig>,
    plan_read: Option<BoundPlanCreationProfile>,
    subscription_read: Option<BoundSubscriptionCreationProfile>,
    payment_link_read: Option<PaymentLinkReadConfig>,
    payment_link: Option<BoundPaymentLinkCreationProfile>,
    payment_link_expiration:
        Option<super::bound_payment_link_expiration::BoundPaymentLinkExpirationV1>,
    plan: Option<BoundPlanCreationProfile>,
    subscription: Option<super::bound_subscription::BoundSubscriptionV1>,
}
impl BoundBillingV1 {
    /// # Errors
    /// Requires matching publishers only for selected Checkout/Portal creation; no fallback.
    pub fn trusted(
        configuration: BoundBillingConfig,
        checkout: Option<TrustedCheckoutPublisher>,
        portal: Option<TrustedPortalPublisher>,
    ) -> BlueprintResult<Self> {
        Self::trusted_with_subscription(configuration, checkout, portal, None)
    }
    /// Adds an explicitly separate subscription publisher without changing the legacy constructor.
    /// The selector attests internal service-level tenant authority, not end-user ownership.
    /// # Errors
    /// Rejects any missing or surplus publisher; never falls back to ordinary runtime credentials.
    pub fn trusted_with_subscription(
        configuration: BoundBillingConfig,
        checkout: Option<TrustedCheckoutPublisher>,
        portal: Option<TrustedPortalPublisher>,
        subscription: Option<TrustedSubscriptionPublisher>,
    ) -> BlueprintResult<Self> {
        Self::trusted_with_payment_link_expiration(
            configuration,
            checkout,
            portal,
            subscription,
            None,
        )
    }
    /// Adds explicit metadata-only expiration publication authority; no fallback.
    /// # Errors
    /// Rejects missing or surplus publishers for every selected capability.
    pub fn trusted_with_payment_link_expiration(
        configuration: BoundBillingConfig,
        checkout: Option<TrustedCheckoutPublisher>,
        portal: Option<TrustedPortalPublisher>,
        subscription: Option<TrustedSubscriptionPublisher>,
        expiration: Option<TrustedPaymentLinkExpirationPublisher>,
    ) -> BlueprintResult<Self> {
        if (configuration.has_payment_link_expiration()
            || configuration.has_payment_link_expiration_recovery())
            != expiration.is_some()
            || configuration.has_checkout() != checkout.is_some()
            || configuration.has_portal() != portal.is_some()
            || (configuration.has_subscription() || configuration.has_subscription_recovery())
                != subscription.is_some()
        {
            return Err(self::configuration());
        }
        Ok(Self {
            portal_local_expiration: configuration.portal_local_expiration,
            plan_read: configuration.plan_read,
            subscription_read: configuration.subscription_read,
            payment_link_read: configuration.payment_link_read,
            payment_link: configuration.payment_link,
            payment_link_expiration: expiration.map(|publisher| {
                super::bound_payment_link_expiration::BoundPaymentLinkExpirationV1 {
                    expiration: configuration.payment_link_expiration,
                    recovery: configuration.payment_link_expiration_recovery,
                    publisher,
                }
            }),
            plan: configuration.plan,
            subscription: subscription.map(|publisher| {
                super::bound_subscription::BoundSubscriptionV1 {
                    creation: configuration.subscription,
                    recovery: configuration.subscription_recovery,
                    publisher,
                }
            }),
            checkout: configuration
                .checkout
                .zip(checkout)
                .map(|(c, p)| BoundCheckoutV1::trusted(c, p)),
            portal: configuration
                .portal
                .zip(portal)
                .map(|(c, p)| BoundPortalV1::trusted(c, p)),
        })
    }
}
impl BillingBackend {
    /// Select once after backup configuration; all selected authorities validate before listening.
    /// # Errors
    /// Rejects mixed builders, base/managed/backup and any invalid publisher authority.
    pub async fn with_bound_billing(mut self, profile: BoundBillingV1) -> BlueprintResult<Self> {
        if self.bound_checkout.is_some()
            || self.bound_portal.is_some()
            || self.bound_portal_expiration.is_some()
            || self.bound_plan_read.is_some()
            || self.bound_subscription_read.is_some()
            || self.bound_payment_link_read.is_some()
            || self.bound_payment_link.is_some()
            || self.bound_payment_link_expiration.is_some()
            || self.bound_plan.is_some()
            || self.bound_subscription.is_some()
        {
            return Err(configuration());
        }
        if let Some(subscription) = profile.subscription {
            self = self.with_bound_subscription(subscription).await?;
        }
        if let Some(plan) = profile.plan {
            self = self.with_bound_plan(plan).await?;
        }
        if let Some(expiration) = profile.payment_link_expiration {
            self = self.with_bound_payment_link_expiration(expiration).await?;
        }
        if let Some(payment_link) = profile.payment_link {
            self = self.with_bound_payment_link(payment_link).await?;
        }
        if let Some(checkout) = profile.checkout {
            self = self.with_bound_checkout(checkout).await?;
        }
        if let Some(portal) = profile.portal {
            self = self.with_bound_portal(portal).await?;
        }
        if let Some(expiration) = profile.portal_local_expiration {
            self = self
                .with_bound_portal_expiration(expiration.profile)
                .await?;
        }
        if let Some(plan_read) = profile.plan_read {
            self = self.with_bound_plan_read(plan_read).await?;
        }
        if let Some(subscription_read) = profile.subscription_read {
            self = self.with_bound_subscription_read(subscription_read).await?;
        }
        if let Some(payment_link_read) = profile.payment_link_read {
            self = self.with_bound_payment_link_read(payment_link_read).await?;
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn payment_link_recovery_is_independent_and_shared_inspection_has_one_policy() {
        let mut value = json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"paymentLinkExpirationRecovery":{"trustedPrices":["price_owned"],"authority":"internal_service_tenant_v1"}});
        let config = BoundBillingConfig::from_json(&value.to_string()).unwrap();
        assert!(config.has_payment_link_expiration_recovery());
        assert!(!config.has_payment_link_expiration());
        assert!(config.requires_publisher());
        assert_eq!(config.contracts().len(), 2);
        assert!(
            !config
                .contracts()
                .iter()
                .any(|r| r.operation_name == "expireBoundPaymentLink")
        );
        assert!(BoundBillingV1::trusted(config, None, None).is_err());
        value["paymentLinkExpiration"] = value["paymentLinkExpirationRecovery"].clone();
        let config = BoundBillingConfig::from_json(&value.to_string()).unwrap();
        assert_eq!(config.contracts().len(), 3);
        value["paymentLinkExpiration"]["trustedPrices"] = json!(["price_other"]);
        assert!(BoundBillingConfig::from_json(&value.to_string()).is_err());
    }
    #[test]
    fn payment_link_expiration_requires_explicit_publisher_and_authority() {
        let value = json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"paymentLinkExpiration":{"trustedPrices":["price_owned"],"authority":"internal_service_tenant_v1"}});
        let config = BoundBillingConfig::from_json(&value.to_string()).unwrap();
        assert!(config.has_payment_link_expiration());
        assert!(config.requires_publisher());
        assert!(!config.has_payment_link());
        assert!(!config.has_payment_link_read());
        assert_eq!(config.contracts().len(), 2);
        assert!(BoundBillingV1::trusted(config, None, None).is_err());
        for bad in [
            json!(null),
            json!({}),
            json!({"trustedPrices":["price_owned"]}),
            json!({"trustedPrices":["price_owned"],"authority":"end_user"}),
            json!({"trustedPrices":["price_owned"],"authority":"internal_service_tenant_v1","maxRecords":10}),
        ] {
            let mut invalid = value.clone();
            invalid["paymentLinkExpiration"] = bad;
            assert!(BoundBillingConfig::from_json(&invalid.to_string()).is_err());
        }
    }
    fn valid() -> Value {
        json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"portal":{"configurationId":"bpc_owned","returnUrl":"https://example.test/return"}})
    }
    #[test]
    fn subscription_recovery_is_distinct_and_inspection_is_not_duplicated() {
        let selected = serde_json::json!({"plans":[{"localPlanId":Uuid::new_v4(),"creationOperationId":Uuid::new_v4()}],"collectionPolicy":"automatic_default_incomplete_v1","authority":"internal_service_tenant_v1"});
        let mut raw = serde_json::json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"subscriptionRecovery":selected});
        let config = BoundBillingConfig::from_json(&raw.to_string()).unwrap();
        assert!(config.has_subscription_recovery());
        assert!(!config.has_subscription());
        assert!(config.requires_publisher());
        assert_eq!(
            config.contracts(),
            super::super::bound_subscription_recovery_contracts()
        );
        assert!(BoundBillingV1::trusted_with_subscription(config, None, None, None).is_err());
        raw["subscription"] = selected;
        let config = BoundBillingConfig::from_json(&raw.to_string()).unwrap();
        assert_eq!(config.contracts().len(), 3);
        assert_eq!(
            config
                .contracts()
                .iter()
                .filter(|r| r.operation_name == "inspectBoundSubscriptionOperation")
                .count(),
            1
        );
        assert!(
            config
                .contracts()
                .iter()
                .any(|r| r.operation_name == "createBoundSubscription")
        );
    }
    #[test]
    fn subscription_creation_requires_explicit_authority_and_publisher() {
        let raw = serde_json::json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"subscription":{"plans":[{"localPlanId":Uuid::new_v4(),"creationOperationId":Uuid::new_v4()}],"collectionPolicy":"automatic_default_incomplete_v1","authority":"internal_service_tenant_v1"}});
        let config = BoundBillingConfig::from_json(&raw.to_string()).unwrap();
        assert!(config.has_subscription());
        assert!(config.requires_publisher());
        assert_eq!(
            config.contracts(),
            super::super::bound_subscription_contracts()
        );
        assert!(BoundBillingV1::trusted(config, None, None).is_err());
        let config = BoundBillingConfig::from_json(&raw.to_string()).unwrap();
        assert!(BoundBillingV1::trusted_with_subscription(config, None, None, None).is_err());
        let mut invalid = raw;
        invalid["subscription"]
            .as_object_mut()
            .unwrap()
            .remove("authority");
        assert!(BoundBillingConfig::from_json(&invalid.to_string()).is_err());
        invalid["subscription"]["authority"] = serde_json::json!("end_user");
        assert!(BoundBillingConfig::from_json(&invalid.to_string()).is_err());
    }
    #[test]
    fn plan_creation_is_separate_from_plan_read_and_publisher() {
        let config=BoundBillingConfig::from_json(r#"{"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"plan":{"trustedProducts":["custom-product"]}}"#).unwrap();
        assert!(config.has_plan());
        assert!(!config.requires_publisher());
        assert_eq!(config.contracts(), super::super::bound_plan_contracts());
        assert!(BoundBillingV1::trusted(config, None, None).is_ok());
        let config=BoundBillingConfig::from_json(r#"{"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"planRead":{"trustedProducts":["custom-product"]}}"#).unwrap();
        assert!(!config.has_plan());
        assert!(
            !config
                .contracts()
                .iter()
                .any(|r| r.operation_name == "createBoundPlan")
        );
    }
    #[test]
    fn payment_link_creation_is_explicit_and_uses_no_separate_publisher() {
        let value = json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"paymentLink":{"trustedPrices":["price_owned"]}});
        let config = BoundBillingConfig::from_json(&value.to_string()).unwrap();
        assert!(config.has_payment_link());
        assert!(!config.has_payment_link_read());
        assert!(!config.requires_publisher());
        assert_eq!(
            config.contracts(),
            super::super::bound_payment_link_contracts()
        );
        assert!(BoundBillingV1::trusted(config, None, None).is_ok());
        let read=BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"paymentLinkRead":{"trustedPrices":["price_owned"]}}).to_string()).unwrap();
        assert!(!read.has_payment_link());
        assert!(
            read.contracts()
                .iter()
                .all(|r| r.operation_name != "createBoundPaymentLink")
        );
    }
    #[test]
    fn payment_link_read_selects_no_creation_authority() {
        let mut value = json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"paymentLinkRead":{"trustedPrices":["price_owned"]}});
        let config = BoundBillingConfig::from_json(&value.to_string()).unwrap();
        assert!(config.has_payment_link_read());
        assert!(!config.requires_publisher());
        assert_eq!(
            config.contracts(),
            super::super::bound_payment_link_read_contracts()
        );
        assert!(BoundBillingV1::trusted(config, None, None).is_ok());
        for bad in [
            Value::Null,
            json!({}),
            json!({"trustedPrices":[]}),
            json!({"trustedPrices":["price_owned"],"maxRecords":0}),
            json!({"trustedPrices":["price_owned"],"maxRecords":1001}),
            json!({"trustedPrices":["price_owned"],"maxRecords":null}),
            json!({"trustedPrices":["not-a-price"]}),
            json!({"trustedPrices":["price_owned"],"create":true}),
        ] {
            value["paymentLinkRead"] = bad;
            assert!(BoundBillingConfig::from_json(&value.to_string()).is_err());
        }
    }
    #[test]
    fn subscription_read_is_an_explicit_nonpublishing_capability() {
        let mut value = json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"subscriptionRead":{"plans":[{"localPlanId":Uuid::new_v4(),"creationOperationId":Uuid::new_v4()}],"collectionPolicy":"automatic_default_incomplete_v1"}});
        let config = BoundBillingConfig::from_json(&value.to_string()).unwrap();
        assert!(config.has_subscription_read());
        assert!(!config.requires_publisher());
        assert_eq!(config.contracts(), bound_subscription_read_contracts());
        assert!(BoundBillingV1::trusted(config, None, None).is_ok());
        let valid = value["subscriptionRead"].clone();
        for invalid in [
            Value::Null,
            json!({}),
            json!({"plans":[],"collectionPolicy":"automatic_default_incomplete_v1"}),
            json!({"plans":valid["plans"],"collectionPolicy":"send_invoice"}),
            json!({"plans":valid["plans"],"collectionPolicy":"automatic_default_incomplete_v1","create":true}),
        ] {
            value["subscriptionRead"] = invalid;
            assert!(BoundBillingConfig::from_json(&value.to_string()).is_err());
        }
        value["subscriptionRead"] = valid.clone();
        value["subscriptionRead"]["plans"] = json!([valid["plans"][0], valid["plans"][0]]);
        assert!(BoundBillingConfig::from_json(&value.to_string()).is_err());
        value["subscriptionRead"] = valid;
        value["subscriptionRead"]["plans"][0]["localPlanId"] = json!(Uuid::nil());
        assert!(BoundBillingConfig::from_json(&value.to_string()).is_err());
    }
    #[test]
    fn aggregate_selects_only_explicit_domains_and_preserves_legacy_contracts() {
        let portal = BoundBillingConfig::from_json(&valid().to_string()).unwrap();
        assert!(!portal.has_checkout());
        assert!(portal.has_portal());
        assert_eq!(portal.contracts(), bound_portal_contracts());
        let mut both = valid();
        both["checkout"] = json!({"plans":[{"localPlanId":uuid::Uuid::new_v4(),"creationOperationId":uuid::Uuid::new_v4()}],"successUrl":"https://example.test/success","cancelUrl":"https://example.test/cancel"});
        let parsed = BoundBillingConfig::from_json(&both.to_string()).unwrap();
        assert_eq!(
            parsed.contracts(),
            [bound_checkout_contracts(), bound_portal_contracts()].concat()
        );
        both.as_object_mut().unwrap().remove("portal");
        assert_eq!(
            BoundBillingConfig::from_json(&both.to_string())
                .unwrap()
                .contracts(),
            bound_checkout_contracts()
        );
        assert!(BoundBillingV1::trusted(portal, None, None).is_err());
    }
    #[test]
    fn plan_read_selects_no_publisher_or_create_capability() {
        let value = json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"planRead":{"trustedProducts":["custom-product"]}});
        let config = BoundBillingConfig::from_json(&value.to_string()).unwrap();
        assert!(config.has_plan_read());
        assert!(!config.requires_publisher());
        assert_eq!(config.contracts(), bound_plan_read_contracts());
        assert!(BoundBillingV1::trusted(config, None, None).is_ok());
        for invalid in [
            json!(null),
            json!({}),
            json!({"trustedProducts":[]}),
            json!({"trustedProducts":[1]}),
            json!({"trustedProducts":["../product"]}),
            json!({"trustedProducts":["custom-product"],"create":true}),
        ] {
            let mut bad = value.clone();
            bad["planRead"] = invalid;
            assert!(BoundBillingConfig::from_json(&bad.to_string()).is_err());
        }
    }
    #[test]
    fn aggregate_rejects_partial_unknown_and_untrusted_options() {
        let mut missing = valid();
        missing.as_object_mut().unwrap().remove("portal");
        let mut unknown = valid();
        unknown["subscription"] = json!({});
        let mut null = valid();
        null["portal"] = Value::Null;
        let mut empty = valid();
        empty["portal"] = json!({});
        let mut raw = valid();
        raw["portal"]["stripeFields"] = json!({});
        let mut bad_url = valid();
        bad_url["portal"]["returnUrl"] = json!("https://secret@private.example");
        for value in [missing, unknown, null, empty, raw, bad_url] {
            let error = BoundBillingConfig::from_json(&value.to_string())
                .err()
                .unwrap();
            assert_eq!(error.message, "Invalid bound billing configuration");
        }
        assert!(BoundBillingConfig::from_json(&" ".repeat(65_537)).is_err());
    }
    #[test]
    fn portal_local_expiration_does_not_infer_publisher_or_creation() {
        let value = json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"portalLocalExpiration":{"configurationId":"bpc_owned","returnUrl":"https://example.test/return"}});
        let parsed = BoundBillingConfig::from_json(&value.to_string()).unwrap();
        assert!(!parsed.requires_publisher());
        assert!(!parsed.has_portal());
        let routes = parsed.contracts();
        assert_eq!(routes.len(), 2);
        assert!(
            routes
                .iter()
                .all(|r| r.operation_name != "createBoundPortalSession"
                    && r.operation_name != "getStoredBoundPortalSession")
        );
        assert!(BoundBillingV1::trusted(parsed, None, None).is_ok());
        for bad in [
            Value::Null,
            json!({}),
            json!({"configurationId":"bpc_owned","returnUrl":"https://example.test/return","create":true}),
        ] {
            let mut value = value.clone();
            value["portalLocalExpiration"] = bad;
            assert!(BoundBillingConfig::from_json(&value.to_string()).is_err());
        }
    }
}
