//! JSON harness for actual TypeScript billing-service conformance.
use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind, BillingStore, Scope, memory::MemoryBillingStore,
};
use forklaunch_billing_interface::services::{
    BillingPortalService, CheckoutSessionService, PaymentLinkService, PlanService,
    SubscriptionService,
};
use forklaunch_billing_stripe::{client::StripeBillingClient, services::StripeBillingService};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use serde_json::{Value, json};
use std::time::Duration;
use std::{
    io::{self, Read},
    sync::Arc,
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let input: Value = serde_json::from_str(&input)?;
    let store = Arc::new(MemoryBillingStore::new());
    let base = BaseBillingService::new(store).with_database_backup(input["backup"] == true);
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(
            input["base"].as_str().ok_or("Missing base")?,
            Duration::from_secs(2),
            1024 * 1024,
        )?,
    )?;
    let service = StripeBillingService::new(base, client);
    let mut results = Vec::new();
    for action in input["actions"].as_array().ok_or("Missing actions")? {
        results.push(match run(&service, action).await {
            Ok(result) => json!({"ok":result}),
            Err(_) => json!({"error":true}),
        });
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
#[allow(clippy::too_many_lines)]
async fn run(
    service: &StripeBillingService<MemoryBillingStore>,
    action: &Value,
) -> BlueprintResult<Value> {
    let mut input = action.get("input").cloned().unwrap_or_else(|| json!({}));
    if let Some(fields) = input.as_object_mut().and_then(|v| v.remove("stripeFields")) {
        input["providerFields"] = fields;
    }
    let id = input["id"].as_str().unwrap_or("");
    let ids: Option<Vec<String>> = input
        .get("ids")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .map_err(|_| invalid())?;
    macro_rules! request {
        ($method:ident) => {
            serde_json::to_value(
                service
                    .$method(serde_json::from_value(input.clone()).map_err(|_| invalid())?)
                    .await?,
            )
            .map_err(|_| invalid())
        };
    }
    macro_rules! get {
        ($method:ident) => {
            serde_json::to_value(service.$method(id).await?).map_err(|_| invalid())
        };
    }
    macro_rules! list {
        ($method:ident) => {
            serde_json::to_value(service.$method(ids.as_deref()).await?).map_err(|_| invalid())
        };
    }
    match (
        action["service"].as_str().unwrap_or(""),
        action["method"].as_str().unwrap_or(""),
    ) {
        ("Plan", "createPlan") => request!(create_plan),
        ("Plan", "getPlan") => get!(get_plan),
        ("Plan", "updatePlan") => request!(update_plan),
        ("Plan", "deletePlan") => get!(delete_plan),
        ("Plan", "listPlans") => list!(list_plans),
        ("Subscription", "createSubscription") => request!(create_subscription),
        ("Subscription", "getSubscription") => get!(get_subscription),
        ("Subscription", "updateSubscription") => request!(update_subscription),
        ("Subscription", "deleteSubscription") => get!(delete_subscription),
        ("Subscription", "listSubscriptions") => list!(list_subscriptions),
        ("Subscription", "getUserSubscription") => get!(get_user_subscription),
        ("Subscription", "getOrganizationSubscription") => get!(get_organization_subscription),
        ("Subscription", "cancelSubscription") => get!(cancel_subscription),
        ("Subscription", "resumeSubscription") => get!(resume_subscription),
        ("Subscription", "findSubscriptionIdByExternalId") => Ok(service
            .find_subscription_id_by_external_id(input["externalId"].as_str().ok_or_else(invalid)?)
            .await?
            .map_or(Value::Null, |id| json!({"id":id}))),
        ("CheckoutSession", "createCheckoutSession") => request!(create_checkout_session),
        ("CheckoutSession", "getCheckoutSession") => get!(get_checkout_session),
        ("CheckoutSession", "expireCheckoutSession") => get!(expire_checkout_session),
        ("CheckoutSession", "handleCheckoutSuccess") => get!(handle_checkout_success),
        ("CheckoutSession", "handleCheckoutFailure") => get!(handle_checkout_failure),
        ("PaymentLink", "createPaymentLink") => request!(create_payment_link),
        ("PaymentLink", "getPaymentLink") => get!(get_payment_link),
        ("PaymentLink", "updatePaymentLink") => request!(update_payment_link),
        ("PaymentLink", "expirePaymentLink") => get!(expire_payment_link),
        ("PaymentLink", "handlePaymentSuccess") => get!(handle_payment_success),
        ("PaymentLink", "handlePaymentFailure") => get!(handle_payment_failure),
        ("PaymentLink", "listPaymentLinks") => list!(list_payment_links),
        ("BillingPortal", "createBillingPortalSession") => request!(create_billing_portal_session),
        ("BillingPortal", "getBillingPortalSession") => get!(get_billing_portal_session),
        ("BillingPortal", "updateBillingPortalSession") => request!(update_billing_portal_session),
        ("BillingPortal", "expireBillingPortalSession") => get!(expire_billing_portal_session),
        ("Inspect", kind) => {
            let kind = match kind {
                "Plan" => BillingKind::Plan,
                "Subscription" => BillingKind::Subscription,
                "CheckoutSession" => BillingKind::CheckoutSession,
                "PaymentLink" => BillingKind::PaymentLink,
                "BillingPortal" => BillingKind::BillingPortal,
                _ => return Err(invalid()),
            };
            Ok(service
                .base()
                .store()
                .get(Scope::Durable, kind, id)
                .await?
                .unwrap_or(Value::Null))
        }
        _ => Err(invalid()),
    }
}
fn invalid() -> BlueprintError {
    BlueprintError::new("fixture_invalid_input", "Invalid billing fixture input")
}
