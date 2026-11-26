//! Billing application contracts shared by generated HTTP, MCP and SDK surfaces.
use crate::services::{
    BillingPortalDto, CheckoutSessionDto, CreateBillingPortalDto, CreateCheckoutSessionDto,
    CreatePaymentLinkDto, CreatePlanDto, CreateSubscriptionDto, PaymentLinkDto, PlanDto,
    SubscriptionDto, UpdateBillingPortalDto, UpdatePaymentLinkDto, UpdatePlanDto,
    UpdateSubscriptionDto,
};
use forklaunch_core::contract::{
    AccessLevel, AuthConfig, BodyContract, BodyKind, ContractRegistry, FieldContract, HmacAuth,
    RouteContract, SchemaRef, ServiceContract,
};
use schemars::JsonSchema;
use serde_json::{Value, json};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Create,
    Update,
    Read,
}
fn body<T: JsonSchema>(name: &str, family: &str, form: Form, stripe: bool) -> BodyContract {
    let mut settings = schemars::generate::SchemaSettings::draft07();
    settings.inline_subschemas = true;
    let mut schema = serde_json::to_value(settings.into_generator().into_root_schema_for::<T>())
        .expect("package schema serializes");
    let object = if schema["type"] == "array" {
        &mut schema["items"]
    } else {
        &mut schema
    };
    configure(object, family, form, stripe);
    let mut body = BodyContract::json(name);
    body.schema.json_schema = Some(schema.to_string());
    body
}
fn configure(schema: &mut Value, family: &str, form: Form, stripe: bool) {
    if form == Form::Create && !stripe {
        schema["properties"]
            .as_object_mut()
            .expect("object schema")
            .remove("id");
    }
    if form == Form::Update && family == "BillingPortal" {
        schema["properties"]
            .as_object_mut()
            .expect("object schema")
            .remove("customerId");
    }
    if !stripe && form != Form::Create {
        schema["properties"]["id"]["format"] = json!("uuid");
    }
    if stripe {
        schema["properties"]
            .as_object_mut()
            .expect("object schema")
            .remove("providerFields");
        // The actual validator's type<T>() is z.any(): OpenAPI leaves it
        // unconstrained and optional even when the TS member is required.
        schema["properties"]["stripeFields"] = json!({});
        if family == "PaymentLink" && form == Form::Create {
            schema["properties"]["stripeFields"] = json!({"type":"object", "properties": {
                "line_items": {"type":"array", "items":{"type":"object", "additionalProperties":{}}}
            }, "required":["line_items"], "additionalProperties":true});
            schema["required"]
                .as_array_mut()
                .expect("required list")
                .push(json!("stripeFields"));
        }
        if family == "CheckoutSession" && form == Form::Read {
            schema["properties"]["metadata"] = json!({});
        }
    }
    if !stripe && schema["properties"].get("providerFields").is_some() {
        // validator.unknown carries no type annotation (a821c0fe8).
        schema["properties"]["providerFields"] = json!({});
    }
    for field in [
        "cadence",
        "currency",
        "billingProvider",
        "partyType",
        "status",
    ] {
        if schema["properties"].get(field).is_some()
            && let Some(values) = enum_values(field, family, stripe)
        {
            schema["properties"][field]["enum"] = values;
        }
    }
    if schema["properties"].get("paymentMethods").is_some() {
        schema["properties"]["paymentMethods"]["items"]["enum"] =
            enum_values("paymentMethods", family, stripe).expect("configured payment methods");
    }
}
fn enum_values(field: &str, family: &str, stripe: bool) -> Option<Value> {
    match (field, stripe) {
        ("cadence", false) => Some(json!(["WEEKLY", "BIWEEKLY", "MONTHLY", "ANNUALLY"])),
        ("currency", false) => Some(json!([
            "USD", "EUR", "GBP", "CAD", "AUD", "CHF", "JPY", "SEK", "NOK", "DKK", "CZK", "PLN",
            "HUF", "RON", "BGN", "RUB", "INR", "BRL", "ZAR", "MXN", "CNY", "HKD", "TWD", "SGD",
            "THB", "TRY", "RSD", "HRK", "ISK", "BAM"
        ])),
        ("billingProvider", false) => Some(json!(["none", "stripe", "adyen"])),
        ("paymentMethods", false) => Some(json!([
            "ach",
            "credit_card",
            "debit_card",
            "wire",
            "apple_pay",
            "google_pay",
            "paypal"
        ])),
        ("partyType", _) => Some(json!(["user", "organization"])),
        ("status", _) if family != "Subscription" => Some(json!([
            "PENDING",
            "SUCCESS",
            "CREATED",
            "UPDATED",
            "COMPLETED",
            "CANCELLED",
            "FAILED",
            "EXPIRED"
        ])),
        ("cadence", true) => Some(json!(["week", "month", "year"])),
        ("currency", true) => Some(json!([
            "USD", "EUR", "JPY", "GBP", "AUD", "CAD", "CHF", "CNY", "AED", "ARS", "BAM", "BDT",
            "BGN", "BOB", "BRL", "CLP", "COP", "CRC", "CZK", "DKK", "DZD", "EGP", "ETB", "GEL",
            "GTQ", "HKD", "HUF", "IDR", "ILS", "INR", "ISK", "JMD", "KES", "KHR", "KRW", "KZT",
            "LAK", "LKR", "MAD", "MMK", "MXN", "MYR", "NGN", "NOK", "NPR", "NZD", "PEN", "PHP",
            "PKR", "PLN", "PYG", "QAR", "RON", "RSD", "RUB", "SAR", "SEK", "SGD", "THB", "TRY",
            "TWD", "TZS", "UAH", "UGX", "UYU", "VND", "ZAR", "ZMW"
        ])),
        ("billingProvider", true) => Some(json!(["stripe"])),
        ("paymentMethods", true) => Some(json!([
            "affirm",
            "afterpay_clearpay",
            "alipay",
            "alma",
            "au_becs_debit",
            "bacs_debit",
            "bancontact",
            "billie",
            "blik",
            "boleto",
            "card",
            "cashapp",
            "eps",
            "fpx",
            "giropay",
            "grabpay",
            "ideal",
            "klarna",
            "konbini",
            "link",
            "mobilepay",
            "multibanco",
            "oxxo",
            "p24",
            "pay_by_bank",
            "paynow",
            "paypal",
            "pix",
            "promptpay",
            "satispay",
            "sepa_debit",
            "sofort",
            "swish",
            "twint",
            "us_bank_account",
            "wechat_pay",
            "zip"
        ])),
        _ => None,
    }
}
fn text() -> BodyContract {
    BodyContract {
        kind: BodyKind::Text,
        schema: SchemaRef::named("String").with_json_schema(r#"{"type":"string"}"#),
        content_type: Some("text/plain".into()),
    }
}
/// The thirty application operations shared by both billing variants.
/// Provider callbacks, platform events, compliance and health are separate.
/// # Panics
/// Only package-owned schemas can fail serialization.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn registry(stripe: bool) -> ContractRegistry {
    let mut registry = ContractRegistry::for_service(ServiceContract::new("billing", "0.1.0"));
    registry.register(
        RouteContract::post("Create Plan", "Create Plan", "/plan/")
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ))
            .with_request_body(body::<CreatePlanDto>(
                "CreatePlan",
                "Plan",
                Form::Create,
                stripe,
            ))
            .with_response(
                200,
                Some(body::<PlanDto>("Plan", "Plan", Form::Read, stripe)),
            ),
    );
    registry.register(
        RouteContract::get("List Plans", "List Plans", "/plan/")
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ))
            .with_query({
                let mut field = FieldContract::optional("ids", "StringArray");
                field.schema.json_schema =
                    Some(r#"{"type":"array","items":{"type":"string"}}"#.into());
                field
            })
            .with_response(
                200,
                Some(body::<Vec<PlanDto>>("PlanList", "Plan", Form::Read, stripe)),
            ),
    );
    registry.register(
        RouteContract::get("Get Plan", "Get Plan", "/plan/:id")
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ))
            .with_param(FieldContract::required("id", "String"))
            .with_response(
                200,
                Some(body::<PlanDto>("Plan", "Plan", Form::Read, stripe)),
            ),
    );
    registry.register(
        RouteContract::put("Update Plan", "Update Plan", "/plan/")
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ))
            .with_request_body(body::<UpdatePlanDto>(
                "UpdatePlan",
                "Plan",
                Form::Update,
                stripe,
            ))
            .with_response(
                200,
                Some(body::<PlanDto>("Plan", "Plan", Form::Read, stripe)),
            ),
    );
    registry.register(
        RouteContract::delete("Delete Plan", "Delete Plan", "/plan/:id")
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ))
            .with_param(FieldContract::required("id", "String"))
            .with_response(200, Some(text())),
    );
    registry.register(
        RouteContract::post(
            "Create Subscription",
            "Create Subscription",
            "/subscription/",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_request_body(body::<CreateSubscriptionDto>(
            "CreateSubscription",
            "Subscription",
            Form::Create,
            stripe,
        ))
        .with_response(
            200,
            Some(body::<SubscriptionDto>(
                "Subscription",
                "Subscription",
                Form::Read,
                stripe,
            )),
        ),
    );
    registry.register(
        RouteContract::get("List Subscriptions", "List Subscriptions", "/subscription/")
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ))
            .with_query({
                let mut field = FieldContract::optional("ids", "StringArray");
                field.schema.json_schema =
                    Some(r#"{"type":"array","items":{"type":"string"}}"#.into());
                field
            })
            .with_response(
                200,
                Some(body::<Vec<SubscriptionDto>>(
                    "SubscriptionList",
                    "Subscription",
                    Form::Read,
                    stripe,
                )),
            ),
    );
    registry.register(
        RouteContract::get("Get Subscription", "Get Subscription", "/subscription/:id")
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ))
            .with_param(FieldContract::required("id", "String"))
            .with_response(
                200,
                Some(body::<SubscriptionDto>(
                    "Subscription",
                    "Subscription",
                    Form::Read,
                    stripe,
                )),
            ),
    );
    registry.register(
        RouteContract::get(
            "Get User Subscription",
            "Get User Subscription",
            "/subscription/user/:id",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(
            200,
            Some(body::<SubscriptionDto>(
                "Subscription",
                "Subscription",
                Form::Read,
                stripe,
            )),
        ),
    );
    registry.register(
        RouteContract::get(
            "Get Organization Subscription",
            "Get Organization Subscription",
            "/subscription/organization/:id",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(
            200,
            Some(body::<SubscriptionDto>(
                "Subscription",
                "Subscription",
                Form::Read,
                stripe,
            )),
        ),
    );
    registry.register(
        RouteContract::put(
            "Update Subscription",
            "Update Subscription",
            "/subscription/:id",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_request_body(body::<UpdateSubscriptionDto>(
            "UpdateSubscription",
            "Subscription",
            Form::Update,
            stripe,
        ))
        .with_response(
            200,
            Some(body::<SubscriptionDto>(
                "Subscription",
                "Subscription",
                Form::Read,
                stripe,
            )),
        ),
    );
    registry.register(
        RouteContract::delete(
            "Delete Subscription",
            "Delete Subscription",
            "/subscription/:id",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(200, Some(text())),
    );
    registry.register(
        RouteContract::get(
            "Cancel Subscription",
            "Cancel Subscription",
            "/subscription/:id/cancel",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(200, Some(text())),
    );
    registry.register(
        RouteContract::get(
            "Resume Subscription",
            "Resume Subscription",
            "/subscription/:id/resume",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(200, Some(text())),
    );
    registry.register(
        RouteContract::post(
            "Create Checkout Session",
            "Create Checkout Session",
            "/checkout-session/",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_request_body(body::<CreateCheckoutSessionDto>(
            "CreateCheckoutSession",
            "CheckoutSession",
            Form::Create,
            stripe,
        ))
        .with_response(
            200,
            Some(body::<CheckoutSessionDto>(
                "CheckoutSession",
                "CheckoutSession",
                Form::Read,
                stripe,
            )),
        ),
    );
    registry.register(
        RouteContract::get(
            "Get Checkout Session",
            "Get Checkout Session",
            "/checkout-session/:id",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(
            200,
            Some(body::<CheckoutSessionDto>(
                "CheckoutSession",
                "CheckoutSession",
                Form::Read,
                stripe,
            )),
        ),
    );
    registry.register(
        RouteContract::get(
            "Expire Checkout Session",
            "Expire Checkout Session",
            "/checkout-session/:id/expire",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(200, Some(text())),
    );
    registry.register(
        RouteContract::get(
            "Handle Checkout Success",
            "Handle Checkout Success",
            "/checkout-session/:id/success",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(200, Some(text())),
    );
    registry.register(
        RouteContract::get(
            "Handle Checkout Failure",
            "Handle Checkout Failure",
            "/checkout-session/:id/failure",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(200, Some(text())),
    );
    registry.register(
        RouteContract::post(
            "Create Payment Link",
            "Create Payment Link",
            "/payment-link/",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_request_body(body::<CreatePaymentLinkDto>(
            "CreatePaymentLink",
            "PaymentLink",
            Form::Create,
            stripe,
        ))
        .with_response(
            200,
            Some(body::<PaymentLinkDto>(
                "PaymentLink",
                "PaymentLink",
                Form::Read,
                stripe,
            )),
        ),
    );
    registry.register(
        RouteContract::get("Get Payment Link", "Get Payment Link", "/payment-link/:id")
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ))
            .with_param(FieldContract::required("id", "String"))
            .with_response(
                200,
                Some(body::<PaymentLinkDto>(
                    "PaymentLink",
                    "PaymentLink",
                    Form::Read,
                    stripe,
                )),
            ),
    );
    registry.register(
        RouteContract::put(
            "Update Payment Link",
            "Update Payment Link",
            "/payment-link/:id",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_request_body(body::<UpdatePaymentLinkDto>(
            "UpdatePaymentLink",
            "PaymentLink",
            Form::Update,
            stripe,
        ))
        .with_response(
            200,
            Some(body::<PaymentLinkDto>(
                "PaymentLink",
                "PaymentLink",
                Form::Read,
                stripe,
            )),
        ),
    );
    registry.register(
        RouteContract::delete(
            "Expire Payment Link",
            "Expire Payment Link",
            "/payment-link/:id",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(200, Some(text())),
    );
    registry.register(
        RouteContract::get(
            "Handle Payment Success",
            "Handle Payment Success",
            "/payment-link/:id/success",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(200, Some(text())),
    );
    registry.register(
        RouteContract::get(
            "Handle Payment Failure",
            "Handle Payment Failure",
            "/payment-link/:id/failure",
        )
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(FieldContract::required("id", "String"))
        .with_response(200, Some(text())),
    );
    registry.register(
        RouteContract::get("List Payment Links", "List Payment Links", "/payment-link/")
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ))
            .with_query({
                let mut field = FieldContract::optional("ids", "StringArray");
                field.schema.json_schema =
                    Some(r#"{"type":"array","items":{"type":"string"}}"#.into());
                field
            })
            .with_response(
                200,
                Some(body::<Vec<PaymentLinkDto>>(
                    "PaymentLinkList",
                    "PaymentLink",
                    Form::Read,
                    stripe,
                )),
            ),
    );
    registry.register(
        RouteContract::post(
            "Create Billing Portal Session",
            "Create Billing Portal Session",
            "/billing-portal/",
        )
        .with_access(AccessLevel::Protected)
        .with_auth(AuthConfig::jwt(
            forklaunch_core::contract::JwtAuthConfig::JwksUrl {
                url: "${JWKS_PUBLIC_KEY_URL}".into(),
            },
        ))
        .with_rbac(forklaunch_core::contract::RbacConfig {
            allowed_roles: vec!["admin".into()],
            ..Default::default()
        })
        .with_request_body(body::<CreateBillingPortalDto>(
            "CreateBillingPortal",
            "BillingPortal",
            Form::Create,
            stripe,
        ))
        .with_response(
            200,
            Some(body::<BillingPortalDto>(
                "BillingPortal",
                "BillingPortal",
                Form::Read,
                stripe,
            )),
        ),
    );
    registry.register(
        RouteContract::get(
            "Get Billing Portal Session",
            "Get Billing Portal Session",
            "/billing-portal/:id",
        )
        .with_access(AccessLevel::Protected)
        .with_auth(AuthConfig::jwt(
            forklaunch_core::contract::JwtAuthConfig::JwksUrl {
                url: "${JWKS_PUBLIC_KEY_URL}".into(),
            },
        ))
        .with_rbac(forklaunch_core::contract::RbacConfig {
            allowed_roles: vec!["admin".into()],
            ..Default::default()
        })
        .with_param(FieldContract::required("id", "String"))
        .with_response(
            200,
            Some(body::<BillingPortalDto>(
                "BillingPortal",
                "BillingPortal",
                Form::Read,
                stripe,
            )),
        ),
    );
    registry.register(
        RouteContract::put(
            "Update Billing Portal Session",
            "Update Billing Portal Session",
            "/billing-portal/:id",
        )
        .with_access(AccessLevel::Protected)
        .with_auth(AuthConfig::jwt(
            forklaunch_core::contract::JwtAuthConfig::JwksUrl {
                url: "${JWKS_PUBLIC_KEY_URL}".into(),
            },
        ))
        .with_rbac(forklaunch_core::contract::RbacConfig {
            allowed_roles: vec!["admin".into()],
            ..Default::default()
        })
        .with_param(FieldContract::required("id", "String"))
        .with_request_body(body::<UpdateBillingPortalDto>(
            "UpdateBillingPortal",
            "BillingPortal",
            Form::Update,
            stripe,
        ))
        .with_response(
            200,
            Some(body::<BillingPortalDto>(
                "BillingPortal",
                "BillingPortal",
                Form::Read,
                stripe,
            )),
        ),
    );
    registry.register(
        RouteContract::delete(
            "Expire Billing Portal Session",
            "Expire Billing Portal Session",
            "/billing-portal/:id",
        )
        .with_access(AccessLevel::Protected)
        .with_auth(AuthConfig::jwt(
            forklaunch_core::contract::JwtAuthConfig::JwksUrl {
                url: "${JWKS_PUBLIC_KEY_URL}".into(),
            },
        ))
        .with_rbac(forklaunch_core::contract::RbacConfig {
            allowed_roles: vec!["admin".into()],
            ..Default::default()
        })
        .with_param(FieldContract::required("id", "String"))
        .with_response(200, Some(text())),
    );
    registry
}

/// Core runtime surface with native error responses and shared health metadata.
#[must_use]
pub fn module_registry(stripe: bool) -> ContractRegistry {
    let mut result = ContractRegistry::for_service(ServiceContract::new("billing", "0.1.0"));
    for source in registry(stripe).routes() {
        let mut route = source.clone();
        for status in [400, 404, 409, 500, 502] {
            route = route.with_response(status, Some(text()));
        }
        result.register(route);
    }
    for route in compliance_contracts() {
        result.register(route);
    }
    if stripe {
        for route in callback_contracts() {
            let mut route = route;
            for status in [400, 409, 500, 502, 503] {
                if !route
                    .responses
                    .iter()
                    .any(|response| response.status_code == status)
                {
                    route = route.with_response(status, Some(text()));
                }
            }
            result.register(route);
        }
    }
    result.register(health_contract());
    result
}
#[must_use]
pub fn health_contract() -> RouteContract {
    RouteContract::get("Get Health", "Service health", "/health")
        .with_response(200, Some(BodyContract::json("Health")))
}

#[derive(JsonSchema)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct BillingExport {
    user_id: String,
    entities: std::collections::BTreeMap<String, Value>,
}
#[derive(JsonSchema)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct BillingErasure {
    entities_affected: Vec<String>,
    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    records_deleted: u64,
}
fn compliance_body<T: JsonSchema>(name: &str) -> BodyContract {
    let mut settings = schemars::generate::SchemaSettings::draft07();
    settings.inline_subschemas = true;
    let mut body = BodyContract::json(name);
    body.schema.json_schema = Some(
        serde_json::to_string(&settings.into_generator().into_root_schema_for::<T>())
            .expect("package schema serializes"),
    );
    body
}
/// Current application entities classify every billing property as non-PII.
/// Export/erasure still require verified system-role JWTs and report empty data.
/// # Panics
/// Only package-owned schemas can fail serialization.
#[must_use]
pub fn compliance_contracts() -> Vec<RouteContract> {
    [
        (
            RouteContract::get(
                "ExportUserData",
                "Exports classified billing PII",
                "/compliance/export/:userId",
            ),
            compliance_body::<BillingExport>("BillingExport"),
        ),
        (
            RouteContract::delete(
                "EraseUserData",
                "Erases classified billing PII",
                "/compliance/erase/:userId",
            ),
            compliance_body::<BillingErasure>("BillingErasure"),
        ),
    ]
    .into_iter()
    .map(|(route, body)| {
        route
            .with_access(AccessLevel::Protected)
            .with_auth(AuthConfig::jwt(
                forklaunch_core::contract::JwtAuthConfig::JwksUrl {
                    url: "${JWKS_PUBLIC_KEY_URL}".into(),
                },
            ))
            .with_rbac(forklaunch_core::contract::RbacConfig {
                allowed_roles: vec!["system".into()],
                ..Default::default()
            })
            .with_param(FieldContract::required("userId", "String"))
            .with_response(200, Some(body))
            .with_response(404, Some(text()))
    })
    .collect()
}

/// Provider and platform callbacks authenticate the exact raw body in their handlers.
#[must_use]
pub fn callback_contracts() -> Vec<RouteContract> {
    let mut raw = text();
    raw.content_type = Some("application/json".into());
    vec![
        RouteContract::post(
            "handleWebhookEvent",
            "Handle a stripe event via webhook",
            "/webhook",
        )
        .with_access(AccessLevel::Public)
        .with_header(FieldContract::required("stripe-signature", "String"))
        .with_request_body(raw)
        .with_response(200, Some(text()))
        .with_response(400, Some(text())),
        RouteContract::post(
            "ReceivePlatformEvent",
            "Receives a signed event from the ForkLaunch platform",
            "/platform-events/:feature",
        )
        .with_access(AccessLevel::Public)
        .with_param(FieldContract::required("feature", "String"))
        .with_request_body(BodyContract::json("PlatformEvent"))
        .with_response(200, Some(text()))
        .with_response(401, Some(text()))
        .with_response(404, Some(text())),
    ]
}

#[cfg(test)]
mod portable_tests {
    use super::*;

    #[test]
    fn portable_billing_erasure_count_preserves_numeric_bounds() {
        const SAFE: u64 = 9_007_199_254_740_991;
        let schema = serde_json::to_value(schemars::schema_for!(BillingErasure)).unwrap();
        let count = &schema["properties"]["recordsDeleted"];
        assert_eq!(count["type"], "integer");
        let validate =
            forklaunch_core::contract::schema_validation::compile_checked(count).unwrap();
        for value in [0, 42, SAFE] {
            validate(&serde_json::json!(value)).unwrap();
        }
        for value in [
            serde_json::json!(-1),
            serde_json::json!(SAFE + 1),
            serde_json::json!(1.5),
            serde_json::json!("42"),
        ] {
            assert!(validate(&value).is_err());
        }
    }
}
