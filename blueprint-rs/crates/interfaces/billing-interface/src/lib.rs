#![forbid(unsafe_code)]

pub mod customers;
pub mod module_contracts;
pub mod services;

use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::domain::Page;
use forklaunch_core::domain::PageRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    Usd,
    Eur,
    Gbp,
}

impl Currency {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Usd => "USD",
            Self::Eur => "EUR",
            Self::Gbp => "GBP",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    pub amount_minor: i64,
    pub currency: Currency,
}

impl Money {
    #[must_use]
    pub const fn usd_cents(amount_minor: i64) -> Self {
        Self {
            amount_minor,
            currency: Currency::Usd,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
}

impl Customer {
    #[must_use]
    pub fn new(id: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            email: email.into(),
            name: None,
        }
    }

    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub id: String,
    pub name: String,
    pub price: Money,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Trialing,
    Active,
    PastDue,
    Canceled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    pub id: String,
    pub customer_id: String,
    pub plan_id: String,
    pub status: SubscriptionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutSession {
    pub id: String,
    pub customer_id: String,
    pub url: String,
}

/// How often a price recurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingInterval {
    Month,
    Year,
}

impl BillingInterval {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Month => "month",
            Self::Year => "year",
        }
    }
}

/// A recurring price attached to a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Price {
    pub id: String,
    pub plan_id: String,
    pub money: Money,
    pub interval: BillingInterval,
}

/// The lifecycle status of an invoice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvoiceStatus {
    Draft,
    Open,
    Paid,
    Void,
}

/// A single billable line on an invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceLineItem {
    pub description: String,
    pub amount: Money,
}

/// A customer invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invoice {
    pub id: String,
    pub customer_id: String,
    pub status: InvoiceStatus,
    pub total: Money,
    pub line_items: Vec<InvoiceLineItem>,
}

/// A feature entitlement granted to a customer, optionally metered by a limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entitlement {
    pub customer_id: String,
    pub feature: String,
    pub limit: Option<u64>,
}

/// A recorded usage event for a metered feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageRecord {
    pub customer_id: String,
    pub feature: String,
    pub quantity: u64,
    pub recorded_at_unix_seconds: u64,
}

/// A provider webhook event (e.g. Stripe) awaiting handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookEvent {
    pub id: String,
    pub kind: String,
    pub payload_json: String,
}

pub trait BillingService {
    /// # Errors
    ///
    /// Returns an error when the billing backend cannot complete the lookup.
    fn get_customer(&self, id: &str) -> BlueprintResult<Option<Customer>>;

    /// # Errors
    ///
    /// Returns an error when the billing backend cannot list customers.
    fn list_customers(&self, request: PageRequest) -> BlueprintResult<Page<Customer>>;

    /// # Errors
    ///
    /// Returns an error when the billing backend cannot list subscriptions.
    fn list_subscriptions(&self, customer_id: &str) -> BlueprintResult<Vec<Subscription>>;

    /// # Errors
    ///
    /// Returns an error when the billing backend cannot create a checkout session.
    fn create_checkout_session(
        &self,
        customer_id: &str,
        plan_id: &str,
        context: &AuditContext,
    ) -> BlueprintResult<CheckoutSession>;

    /// # Errors
    ///
    /// Returns an error when the billing backend cannot list invoices.
    fn list_invoices(&self, _customer_id: &str) -> BlueprintResult<Vec<Invoice>> {
        Ok(Vec::new())
    }

    /// # Errors
    ///
    /// Returns an error when entitlement resolution fails.
    fn has_entitlement(&self, _customer_id: &str, _feature: &str) -> BlueprintResult<bool> {
        Ok(false)
    }

    /// # Errors
    ///
    /// Returns an error when the usage record cannot be persisted.
    fn record_usage(&self, _record: UsageRecord) -> BlueprintResult<()> {
        Ok(())
    }
}

/// Billing HTTP route contracts, the source of truth for `OpenAPI` and SDK generation.
pub mod contracts {
    use forklaunch_core::contract::{
        AccessLevel, AuthConfig, BodyContract, ContractRegistry, FieldContract, JwtAuthConfig,
        RouteContract, ServiceContract,
    };

    /// Builds the billing service contract registry.
    #[must_use]
    pub fn registry() -> ContractRegistry {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("billing", "0.1.0"));

        registry.register(
            RouteContract::get("List Customers", "List billing customers", "/customers")
                .with_access(AccessLevel::Protected)
                .with_auth(AuthConfig::jwt(JwtAuthConfig::SignatureKey))
                .with_query(FieldContract::optional("limit", "Integer"))
                .with_query(FieldContract::optional("cursor", "String"))
                .with_response(200, Some(BodyContract::json("CustomerPage"))),
        );

        registry.register(
            RouteContract::get("Get Customer", "Get a billing customer", "/customers/:id")
                .with_access(AccessLevel::Protected)
                .with_auth(AuthConfig::jwt(JwtAuthConfig::SignatureKey))
                .with_param(FieldContract::required("id", "String"))
                .with_response(200, Some(BodyContract::json("Customer")))
                .with_response(404, None),
        );

        registry.register(
            RouteContract::get(
                "List Subscriptions",
                "List a customer's subscriptions",
                "/customers/:id/subscriptions",
            )
            .with_access(AccessLevel::Protected)
            .with_auth(AuthConfig::jwt(JwtAuthConfig::SignatureKey))
            .with_param(FieldContract::required("id", "String"))
            .with_response(200, Some(BodyContract::json("SubscriptionList"))),
        );

        registry.register(
            RouteContract::post(
                "Create Checkout Session",
                "Start a checkout session",
                "/checkout",
            )
            .with_access(AccessLevel::Protected)
            .with_auth(AuthConfig::jwt(JwtAuthConfig::SignatureKey))
            .with_request_body(BodyContract::json("CheckoutRequest"))
            .with_response(201, Some(BodyContract::json("CheckoutSession"))),
        );

        registry.register(
            RouteContract::post("Receive Webhook", "Handle a provider webhook", "/webhooks")
                .with_request_body(BodyContract::json("WebhookEvent"))
                .with_response(204, None),
        );

        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_records_minor_units() {
        let money = Money::usd_cents(999);

        assert_eq!(money.amount_minor, 999);
        assert_eq!(money.currency.as_str(), "USD");
    }

    #[test]
    fn billing_contracts_cover_core_routes() {
        let registry = contracts::registry();
        let operation_names: Vec<&str> = registry
            .routes()
            .iter()
            .map(|route| route.operation_name.as_str())
            .collect();

        assert!(operation_names.contains(&"listCustomers"));
        assert!(operation_names.contains(&"createCheckoutSession"));
        assert!(operation_names.contains(&"receiveWebhook"));
    }
}
