//! Executes the same action trace as the actual compiled TypeScript base services.
use forklaunch_ecommerce_base::{BaseEcommerceService, store::InMemoryEcommerceStore};
use forklaunch_ecommerce_interface::{
    CartService, InventoryService, OrderService, PaymentService, ProductService, VariantService,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    io::{self, Read},
    sync::Arc,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn dto<T: DeserializeOwned>(input: &Value) -> Result<T> {
    Ok(serde_json::from_value(input.clone())?)
}
fn text<'a>(input: &'a Value, key: &str) -> Result<&'a str> {
    input[key].as_str().ok_or_else(|| "missing string".into())
}
fn ids(input: &Value) -> Result<Option<Vec<String>>> {
    input
        .get("ids")
        .map(|v| serde_json::from_value(v.clone()).map_err(Into::into))
        .transpose()
}
fn value<T: Serialize>(v: T) -> Result<Value> {
    Ok(serde_json::to_value(v)?)
}
async fn action(
    service: &BaseEcommerceService<InMemoryEcommerceStore>,
    method: &str,
    input: &Value,
) -> Result<Value> {
    match method {
        "createProduct" => value(service.create_product(dto(input)?).await?),
        "getProduct" => value(service.get_product(text(input, "id")?).await?),
        "getProductByHandle" => value(
            service
                .get_product_by_handle(text(input, "handle")?)
                .await?,
        ),
        "getProductByExternalId" => value(
            service
                .get_product_by_external_id(text(input, "externalId")?)
                .await?,
        ),
        "updateProduct" => value(service.update_product(dto(input)?).await?),
        "deleteProduct" => value(service.delete_product(text(input, "id")?).await?),
        "listProducts" => value(service.list_products(Some(dto(input)?)).await?),
        "createVariant" => value(service.create_variant(dto(input)?).await?),
        "getVariant" => value(service.get_variant(text(input, "id")?).await?),
        "getVariantByExternalId" => value(
            service
                .get_variant_by_external_id(text(input, "externalId")?)
                .await?,
        ),
        "updateVariant" => value(service.update_variant(dto(input)?).await?),
        "deleteVariant" => value(service.delete_variant(text(input, "id")?).await?),
        "listVariants" => value(service.list_variants(ids(input)?).await?),
        "listVariantsByProduct" => value(
            service
                .list_variants_by_product(text(input, "productId")?)
                .await?,
        ),
        "createInventory" => value(service.create_inventory(dto(input)?).await?),
        "getInventory" => value(service.get_inventory(text(input, "variantId")?).await?),
        "adjustStock" => value(service.adjust_stock(dto(input)?).await?),
        "checkStock" => value(service.check_stock(dto(input)?).await?),
        "createCart" => value(service.create_cart(dto(input)?).await?),
        "getCart" => value(service.get_cart(text(input, "id")?).await?),
        "addItem" => value(service.add_item(dto(input)?).await?),
        "removeItem" => value(service.remove_item(dto(input)?).await?),
        "clearCart" => value(service.clear_cart(text(input, "id")?).await?),
        "createOrder" => value(service.create_order(dto(input)?).await?),
        "getOrder" => value(service.get_order(text(input, "id")?).await?),
        "listOrders" => value(service.list_orders(ids(input)?).await?),
        "transitionOrder" => value(service.transition_order(dto(input)?).await?),
        "createPayment" => value(
            service
                .create_payment_with_reference(
                    dto(input)?,
                    input
                        .get("providerRef")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                )
                .await?,
        ),
        "getPayment" => value(service.get_payment(text(input, "id")?).await?),
        "confirmPayment" => value(service.confirm_payment(dto(input)?).await?),
        "failPayment" => value(service.fail_payment(dto(input)?).await?),
        "claimForConfirmation" => value(
            service
                .claim_for_confirmation(text(input, "providerRef")?)
                .await?,
        ),
        "releaseConfirmationClaim" => value(
            service
                .release_confirmation_claim(text(input, "providerRef")?)
                .await?,
        ),
        _ => Err("unknown action".into()),
    }
}
fn normalize(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("createdAt");
            object.remove("updatedAt");
            for v in object.values_mut() {
                normalize(v);
            }
        }
        Value::Array(values) => {
            for v in values {
                normalize(v);
            }
        }
        _ => {}
    }
}
#[tokio::main]
async fn main() -> Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let actions: Vec<Value> = serde_json::from_str(&input)?;
    let service = BaseEcommerceService::new(Arc::new(InMemoryEcommerceStore::default()));
    let mut results = Vec::new();
    for item in actions {
        let result = action(&service, text(&item, "method")?, &item["input"]).await;
        let mut output = match result {
            Ok(v) => serde_json::json!({"ok":v}),
            Err(_) => serde_json::json!({"error":true}),
        };
        normalize(&mut output);
        results.push(output);
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
