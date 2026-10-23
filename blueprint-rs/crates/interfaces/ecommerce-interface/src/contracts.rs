//! Ecommerce HTTP contracts for the catalog/cart/order/payment module.
use crate::{
    AddCartItemDto, AdjustStockDto, CartDto, CreateCartDto, CreateInventoryDto, CreateOrderDto,
    CreateProductDto, CreateVariantDto, InventoryDto, OrderDto, PaymentDto, ProductDto,
    ProductSearchDto, StockCheckDto, StockCheckResultDto, UpdateProductDto, UpdateVariantDto,
    VariantDto,
    catalog::{CatalogPage, CatalogQuery},
    module::{CreateProviderPayment, CreatedPayment, OrderTransition},
};
use forklaunch_core::contract::{
    AccessLevel, AuthConfig, BodyContract, BodyKind, ContractRegistry, FieldContract, HmacAuth,
    RouteContract, SchemaRef, ServiceContract,
};
use schemars::JsonSchema;
fn json<T: JsonSchema>(name: &str) -> BodyContract {
    let mut body = BodyContract::json(name);
    let mut settings = schemars::generate::SchemaSettings::draft07();
    settings.inline_subschemas = true;
    body.schema.json_schema = Some(
        serde_json::to_string(&settings.into_generator().into_root_schema_for::<T>())
            .expect("schema serializes"),
    );
    body
}
fn text() -> BodyContract {
    BodyContract {
        kind: BodyKind::Text,
        schema: SchemaRef::named("String").with_json_schema(r#"{"type":"string"}"#),
        content_type: Some("text/plain".into()),
    }
}
fn internal(route: RouteContract) -> RouteContract {
    route
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
}
fn param(route: RouteContract, name: &str) -> RouteContract {
    route.with_param(FieldContract::required(name, "String"))
}
fn query<T: JsonSchema>(mut route: RouteContract) -> RouteContract {
    let schema = schemars::generate::SchemaSettings::draft07()
        .into_generator()
        .into_root_schema_for::<T>();
    if let Some(properties) = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        for (name, schema) in properties {
            let mut field = FieldContract::optional(name, name);
            field.schema.json_schema =
                Some(serde_json::to_string(schema).expect("query schema serializes"));
            route = route.with_query(field);
        }
    }
    route
}
fn ids(route: RouteContract) -> RouteContract {
    let mut field = FieldContract::optional("ids", "StringArray");
    field.schema.json_schema = Some(r#"{"type":"array","items":{"type":"string"}}"#.into());
    route.with_query(field)
}
/// Core module surface shared by contracts, both SDKs and MCP discovery.
/// Checkout/import/provider webhooks and compliance add their own registries.
/// # Panics
/// Panics only if package-owned JSON schemas fail serialization.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn registry() -> ContractRegistry {
    let mut registry = ContractRegistry::for_service(ServiceContract::new("ecommerce", "0.1.0"));
    macro_rules! route {
        ($method:ident,$name:literal,$path:literal,$output:ty,$schema:literal) => {
            internal(RouteContract::$method($name, $name, $path))
                .with_response(200, Some(json::<$output>($schema)))
        };
    }
    registry.register(
        route!(post, "Create Product", "/product/", ProductDto, "Product")
            .with_request_body(json::<CreateProductDto>("CreateProduct")),
    );
    registry.register(query::<ProductSearchDto>(route!(
        get,
        "List Products",
        "/product/",
        Vec<ProductDto>,
        "Products"
    )));
    registry.register(query::<CatalogQuery>(route!(
        get,
        "List Catalog",
        "/product/catalog",
        CatalogPage,
        "CatalogPage"
    )));
    registry.register(param(
        route!(
            get,
            "Get Product By Handle",
            "/product/handle/:handle",
            ProductDto,
            "Product"
        ),
        "handle",
    ));
    registry.register(param(
        route!(get, "Get Product", "/product/:id", ProductDto, "Product"),
        "id",
    ));
    registry.register(
        route!(put, "Update Product", "/product/", ProductDto, "Product")
            .with_request_body(json::<UpdateProductDto>("UpdateProduct")),
    );
    registry.register(
        param(
            internal(RouteContract::delete(
                "Delete Product",
                "Delete a product",
                "/product/:id",
            )),
            "id",
        )
        .with_response(200, Some(text())),
    );
    registry.register(
        route!(post, "Create Variant", "/variant/", VariantDto, "Variant")
            .with_request_body(json::<CreateVariantDto>("CreateVariant")),
    );
    registry.register(ids(route!(
        get,
        "List Variants",
        "/variant/",
        Vec<VariantDto>,
        "Variants"
    )));
    registry.register(param(
        route!(
            get,
            "List Variants By Product",
            "/variant/product/:productId",
            Vec<VariantDto>,
            "Variants"
        ),
        "productId",
    ));
    registry.register(param(
        route!(get, "Get Variant", "/variant/:id", VariantDto, "Variant"),
        "id",
    ));
    registry.register(
        route!(put, "Update Variant", "/variant/", VariantDto, "Variant")
            .with_request_body(json::<UpdateVariantDto>("UpdateVariant")),
    );
    registry.register(
        param(
            internal(RouteContract::delete(
                "Delete Variant",
                "Delete a variant",
                "/variant/:id",
            )),
            "id",
        )
        .with_response(200, Some(text())),
    );
    registry.register(
        route!(
            post,
            "Create Inventory",
            "/inventory/",
            InventoryDto,
            "Inventory"
        )
        .with_request_body(json::<CreateInventoryDto>("CreateInventory")),
    );
    registry.register(param(
        route!(
            get,
            "Get Inventory",
            "/inventory/:variantId",
            InventoryDto,
            "Inventory"
        ),
        "variantId",
    ));
    registry.register(
        route!(
            put,
            "Adjust Stock",
            "/inventory/adjust",
            InventoryDto,
            "Inventory"
        )
        .with_request_body(json::<AdjustStockDto>("AdjustStock")),
    );
    registry.register(
        route!(
            post,
            "Check Stock",
            "/inventory/check",
            StockCheckResultDto,
            "StockCheckResult"
        )
        .with_request_body(json::<StockCheckDto>("StockCheck")),
    );
    registry.register(
        route!(post, "Create Cart", "/cart/", CartDto, "Cart").with_request_body(json::<
            CreateCartDto,
        >(
            "CreateCart"
        )),
    );
    registry.register(param(
        route!(get, "Get Cart", "/cart/:id", CartDto, "Cart"),
        "id",
    ));
    registry.register(
        route!(post, "Add Cart Item", "/cart/items", CartDto, "Cart")
            .with_request_body(json::<AddCartItemDto>("AddCartItem"))
            .with_response(400, Some(text())),
    );
    registry.register(param(
        param(
            route!(
                delete,
                "Remove Cart Item",
                "/cart/:cartId/items/:variantId",
                CartDto,
                "Cart"
            ),
            "cartId",
        ),
        "variantId",
    ));
    registry.register(
        param(
            internal(RouteContract::delete(
                "Clear Cart",
                "Clears a cart",
                "/cart/:id",
            )),
            "id",
        )
        .with_response(200, Some(text())),
    );
    registry.register(
        route!(post, "Create Order", "/order/", OrderDto, "Order").with_request_body(json::<
            CreateOrderDto,
        >(
            "CreateOrder",
        )),
    );
    registry.register(ids(route!(
        get,
        "List Orders",
        "/order/",
        Vec<OrderDto>,
        "Orders"
    )));
    registry.register(param(
        route!(get, "Get Order", "/order/:id", OrderDto, "Order"),
        "id",
    ));
    registry.register(
        param(
            route!(
                put,
                "Transition Order",
                "/order/:id/transition",
                OrderDto,
                "Order"
            ),
            "id",
        )
        .with_request_body(json::<OrderTransition>("OrderTransition"))
        .with_response(400, Some(text())),
    );
    registry.register(
        route!(
            post,
            "Create Payment",
            "/payment/",
            CreatedPayment,
            "CreatedPayment"
        )
        .with_request_body(json::<CreateProviderPayment>("CreatePayment")),
    );
    registry.register(param(
        route!(get, "Get Payment", "/payment/:id", PaymentDto, "Payment"),
        "id",
    ));
    registry
}

/// Complete core runtime registry, including framework and typed-handler errors.
#[must_use]
pub fn module_registry() -> ContractRegistry {
    let mut base = registry();
    base.register(import_contract());
    base.register(checkout_contract());
    base.register(stripe_webhook_contract());
    base.register(paypal_webhook_contract());
    let mut registry = ContractRegistry::for_service(ServiceContract::new("ecommerce", "0.1.0"));
    for source in base.routes() {
        let mut route = source.clone();
        for status in [400, 404, 409, 500, 502] {
            if !route
                .responses
                .iter()
                .any(|response| response.status_code == status)
            {
                route = route.with_response(status, Some(text()));
            }
        }
        registry.register(route);
    }
    registry.register(health_contract());
    registry
}
#[must_use]
pub fn health_contract() -> RouteContract {
    RouteContract::get("Get Health", "Reports service health", "/health")
        .with_response(200, Some(BodyContract::json("Health")))
}

/// Normalized bulk catalog import contract; additional runtime errors are added by `module_registry`.
/// # Panics
/// Panics only if package-owned schemas fail to serialize.
#[must_use]
pub fn import_contract() -> RouteContract {
    internal(RouteContract::post(
        "Import Catalog",
        "Bulk import a normalized product catalog",
        "/catalog-import/",
    ))
    .with_request_body(json::<crate::import::CatalogImport>("CatalogImport"))
    .with_response(
        200,
        Some(json::<crate::import::CatalogImportResult>(
            "CatalogImportResult",
        )),
    )
}

/// Unified cart pricing, order creation and provider payment initiation.
/// # Panics
/// Panics only if package-owned schemas fail to serialize.
#[must_use]
pub fn checkout_contract() -> RouteContract {
    internal(RouteContract::post(
        "Checkout",
        "Convert a cart into an order and initiate payment",
        "/checkout/",
    ))
    .with_request_body(json::<crate::module::Checkout>("Checkout"))
    .with_response(
        200,
        Some(json::<crate::module::CheckoutResult>("CheckoutResult")),
    )
    .with_response(400, Some(text()))
    .with_response(502, Some(text()))
}

/// Provider-authenticated callback with exact raw JSON body verification.
/// # Panics
/// Panics only if package-owned schemas fail to serialize.
#[must_use]
pub fn stripe_webhook_contract() -> RouteContract {
    RouteContract::post(
        "Handle Stripe Webhook",
        "Handle a Stripe payment webhook event",
        "/webhook/stripe",
    )
    .with_access(AccessLevel::Public)
    .with_header(FieldContract::required("stripe-signature", "String"))
    .with_request_body(json::<serde_json::Value>("StripeWebhookEvent"))
    .with_response(200, Some(text()))
    .with_response(400, Some(text()))
}
/// Provider-authenticated callback; signature verification receives the complete raw event.
/// # Panics
/// Panics only if package-owned schemas fail to serialize.
#[must_use]
pub fn paypal_webhook_contract() -> RouteContract {
    let mut route = RouteContract::post(
        "Handle PayPal Webhook",
        "Handle a PayPal approval/capture-denial webhook",
        "/webhook/paypal",
    )
    .with_access(AccessLevel::Public)
    .with_request_body(json::<crate::webhook::PaypalWebhookEvent>(
        "PaypalWebhookEvent",
    ))
    .with_response(200, Some(text()))
    .with_response(400, Some(text()));
    for header in [
        "paypal-transmission-id",
        "paypal-transmission-time",
        "paypal-transmission-sig",
        "paypal-cert-url",
        "paypal-auth-algo",
    ] {
        route = route.with_header(FieldContract::required(header, "String"));
    }
    route
}
