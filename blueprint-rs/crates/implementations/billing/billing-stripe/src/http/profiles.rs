//! Request-date and bound-capability selections read from trusted process
//! configuration, and the contract registry they select. Contract generation
//! and service startup share these so they always agree.
use super::{
    BillingIdentityProfile, BillingRequestDateProfile, BoundBillingConfig, BoundCheckoutConfig,
    bound_checkout_contracts, project_configured_billing_hmac,
    project_configured_payment_link_update,
};
use forklaunch_billing_interface::module_contracts;
use forklaunch_core::contract::{ContractRegistry, ServiceContract};
use forklaunch_core::domain::{BlueprintError, BlueprintResult};
use std::env;

fn configuration(message: &str) -> BlueprintError {
    BlueprintError::new("billing_configuration", message)
}

/// `BILLING_REQUEST_DATE_PROFILE`, `legacy` by default.
///
/// # Errors
/// Rejects an unknown profile name.
pub fn request_date_profile_from_env() -> BlueprintResult<BillingRequestDateProfile> {
    env::var("BILLING_REQUEST_DATE_PROFILE")
        .unwrap_or_else(|_| "legacy".into())
        .parse()
}

/// `BILLING_IDENTITY_PROFILE`, `legacy` by default, validated against the
/// bound Checkout, aggregate and publisher settings it requires.
///
/// # Errors
/// Rejects incomplete, stray or non-Stripe bound configuration.
pub fn identity_profile_from_env(stripe: bool) -> BlueprintResult<BillingIdentityProfile> {
    let selected: BillingIdentityProfile = env::var("BILLING_IDENTITY_PROFILE")
        .unwrap_or_else(|_| "legacy".into())
        .parse()?;
    let config = env::var("BILLING_BOUND_CHECKOUT_CONFIG").ok();
    let publisher = env::var("BILLING_PUBLISHER_DATABASE_URL").ok();
    let aggregate = env::var("BILLING_BOUND_BILLING_CONFIG").ok();
    let invalid = || configuration("Invalid bound Checkout startup configuration");
    match selected {
        BillingIdentityProfile::Legacy
            if config.is_some() || aggregate.is_some() || publisher.is_some() =>
        {
            return Err(invalid());
        }
        BillingIdentityProfile::BoundCheckoutV1 => {
            if aggregate.is_some()
                || !stripe
                || publisher.as_ref().is_none_or(|v| v.trim().is_empty())
            {
                return Err(invalid());
            }
            BoundCheckoutConfig::from_json(config.as_deref().ok_or_else(invalid)?)?;
        }
        BillingIdentityProfile::BoundBillingV1 => {
            if config.is_some() || !stripe {
                return Err(invalid());
            }
            let parsed = BoundBillingConfig::from_json(aggregate.as_deref().ok_or_else(invalid)?)?;
            if parsed.requires_publisher() {
                if publisher.as_ref().is_none_or(|v| v.trim().is_empty()) {
                    return Err(invalid());
                }
            } else if publisher.is_some() {
                return Err(invalid());
            }
        }
        BillingIdentityProfile::Legacy => {}
    }
    Ok(selected)
}

/// `BILLING_BOUND_CHECKOUT_CONFIG` when bound Checkout is selected.
///
/// # Errors
/// Rejects the configuration [`identity_profile_from_env`] rejects.
pub fn bound_checkout_config_from_env(
    stripe: bool,
) -> BlueprintResult<Option<BoundCheckoutConfig>> {
    match identity_profile_from_env(stripe)? {
        BillingIdentityProfile::Legacy | BillingIdentityProfile::BoundBillingV1 => Ok(None),
        BillingIdentityProfile::BoundCheckoutV1 => Ok(Some(BoundCheckoutConfig::from_json(
            &env::var("BILLING_BOUND_CHECKOUT_CONFIG")
                .map_err(|_| configuration("Missing bound Checkout configuration"))?,
        )?)),
    }
}

/// `BILLING_BOUND_BILLING_CONFIG` when aggregate bound billing is selected.
///
/// # Errors
/// Rejects the configuration [`identity_profile_from_env`] rejects.
pub fn bound_billing_config_from_env(stripe: bool) -> BlueprintResult<Option<BoundBillingConfig>> {
    if identity_profile_from_env(stripe)? != BillingIdentityProfile::BoundBillingV1 {
        return Ok(None);
    }
    Ok(Some(BoundBillingConfig::from_json(
        &env::var("BILLING_BOUND_BILLING_CONFIG")
            .map_err(|_| configuration("Missing bound billing configuration"))?,
    )?))
}

/// The module's billing routes under `profile`, plus bound Checkout routes
/// when `identity` selects them. Aggregate bound billing adds its routes from
/// an explicit [`BoundBillingConfig`] with [`register_bound_billing`].
///
/// # Errors
/// Rejects aggregate selection and bound Checkout without Stripe.
pub fn module_registry(
    service: ServiceContract,
    stripe: bool,
    profile: BillingRequestDateProfile,
    identity: BillingIdentityProfile,
) -> BlueprintResult<ContractRegistry> {
    if identity == BillingIdentityProfile::BoundBillingV1 {
        return Err(configuration(
            "Aggregate registry requires explicit configuration",
        ));
    }
    if !stripe && identity == BillingIdentityProfile::BoundCheckoutV1 {
        return Err(configuration("Bound Checkout requires Stripe billing"));
    }
    let mut registry = ContractRegistry::for_service(service);
    for route in module_contracts::module_registry(stripe).routes() {
        let mut route = route.clone();
        profile.project(&mut route)?;
        if stripe {
            project_configured_billing_hmac(&mut route);
            project_configured_payment_link_update(&mut route)?;
        }
        registry.register(route);
    }
    if identity == BillingIdentityProfile::BoundCheckoutV1 {
        for route in bound_checkout_contracts() {
            registry.register(route);
        }
    }
    Ok(registry)
}

/// Add the routes an aggregate bound billing configuration selects.
///
/// # Errors
/// Bound billing requires Stripe.
pub fn register_bound_billing(
    registry: &mut ContractRegistry,
    stripe: bool,
    configuration: &BoundBillingConfig,
) -> BlueprintResult<()> {
    if !stripe {
        return Err(self::configuration("Bound billing requires Stripe"));
    }
    for route in configuration.contracts() {
        registry.register(route);
    }
    Ok(())
}
