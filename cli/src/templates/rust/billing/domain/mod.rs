//! Billing domain types and services come from the interface, base and Stripe
//! crates. Application-specific domain code belongs beside these re-exports.
pub mod schemas {
    pub use forklaunch_billing_interface::services::{
        BillingPortalDto, CheckoutSessionDto, CreateBillingPortalDto, CreateCheckoutSessionDto, CreatePaymentLinkDto,
        CreatePlanDto, CreateSubscriptionDto, PaymentLinkDto, PlanDto, SubscriptionDto, UpdateBillingPortalDto,
        UpdateCheckoutSessionDto, UpdatePaymentLinkDto, UpdatePlanDto, UpdateSubscriptionDto,
    };
    pub use forklaunch_billing_interface::{CheckoutSession, Currency, Customer, Money, Plan, Subscription, SubscriptionStatus};
}

pub mod services {
{{#stripe}}    pub use forklaunch_billing_stripe::services::StripeBillingService;
{{/stripe}}    pub use forklaunch_billing_base::services::{BaseBillingService, postgres::PostgresBillingStore};
    pub use forklaunch_billing_interface::services::{
        BillingPortalService, CheckoutSessionService, PaymentLinkService, PlanService, SubscriptionService,
    };
}
