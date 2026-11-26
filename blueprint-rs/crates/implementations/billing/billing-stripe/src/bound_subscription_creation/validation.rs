use super::{BoundSubscriptionCreateRequest, invalid};
use crate::bound_plan_creation::safe_reference;
use chrono::{DateTime, Utc};
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;

fn absent_or_null(value: &Value, fields: &[&str]) -> BlueprintResult<()> {
    if fields
        .iter()
        .any(|field| value.get(*field).is_some_and(|v| !v.is_null()))
    {
        return Err(invalid());
    }
    Ok(())
}
fn empty_arrays(value: &Value, fields: &[&str]) -> BlueprintResult<()> {
    if fields.iter().any(|field| {
        value
            .get(*field)
            .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
    }) {
        return Err(invalid());
    }
    Ok(())
}
fn deleted(value: &Value) -> BlueprintResult<()> {
    if value
        .get("deleted")
        .is_some_and(|v| v.as_bool() != Some(false))
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn customer(value: &Value, id: &str, mode: bool) -> BlueprintResult<()> {
    if value["id"] != id
        || value["object"] != "customer"
        || value["livemode"].as_bool() != Some(mode)
    {
        return Err(invalid());
    }
    deleted(value)?;
    absent_or_null(value, &["discount", "test_clock"])?;
    empty_arrays(value, &["discounts"])
}
fn product(value: &Value, id: &str, mode: bool, expanded: bool) -> BlueprintResult<()> {
    if !expanded && value.as_str() == Some(id) {
        return Ok(());
    }
    if value["id"] != id
        || value["object"] != "product"
        || value["livemode"].as_bool() != Some(mode)
        || value
            .get("active")
            .is_some_and(|v| v.as_bool() != Some(true))
    {
        return Err(invalid());
    }
    deleted(value)
}
fn expected_product(receipt: &Value) -> BlueprintResult<&str> {
    receipt["providerFields"]["id"]
        .as_str()
        .filter(|v| safe_reference(v))
        .ok_or_else(invalid)
}
pub(super) fn plan(
    value: &Value,
    receipt: &Value,
    mode: bool,
    expanded: bool,
) -> BlueprintResult<()> {
    let amount = receipt["price"]
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0 && v.fract() == 0.0 && *v <= 9_007_199_254_740_991.0)
        .ok_or_else(invalid)?;
    let currency = receipt["currency"]
        .as_str()
        .ok_or_else(invalid)?
        .to_ascii_lowercase();
    if value["id"] != receipt["externalId"]
        || value["object"] != "plan"
        || value["livemode"].as_bool() != Some(mode)
        || value["active"].as_bool() != Some(true)
        || receipt["active"].as_bool() != Some(true)
        || value["amount"].as_f64() != Some(amount)
        || value["currency"] != currency
        || value["interval"] != receipt["cadence"]
        || value["interval_count"].as_f64() != Some(1.0)
        || value["billing_scheme"] != "per_unit"
        || value["usage_type"] != "licensed"
    {
        return Err(invalid());
    }
    deleted(value)?;
    absent_or_null(
        value,
        &["trial_period_days", "tiers_mode", "transform_usage"],
    )?;
    product(
        &value["product"],
        expected_product(receipt)?,
        mode,
        expanded,
    )
}
fn price(value: &Value, receipt: &Value, mode: bool) -> BlueprintResult<()> {
    if value["id"] != receipt["externalId"]
        || value["object"] != "price"
        || value["livemode"].as_bool() != Some(mode)
        || value["active"].as_bool() != Some(true)
        || value["type"] != "recurring"
        || value["billing_scheme"] != "per_unit"
        || value["unit_amount"].as_f64() != receipt["price"].as_f64()
        || value["currency"]
            != receipt["currency"]
                .as_str()
                .ok_or_else(invalid)?
                .to_ascii_lowercase()
        || value["recurring"]["interval"] != receipt["cadence"]
        || value["recurring"]["interval_count"].as_f64() != Some(1.0)
        || value["recurring"]["usage_type"] != "licensed"
    {
        return Err(invalid());
    }
    absent_or_null(
        value,
        &["tiers_mode", "transform_quantity", "custom_unit_amount"],
    )?;
    absent_or_null(&value["recurring"], &["trial_period_days"])?;
    product(&value["product"], expected_product(receipt)?, mode, false)
}
fn integral(value: &Value) -> Option<u64> {
    let number = value.as_f64().filter(|v| {
        v.is_finite() && *v >= 0.0 && *v <= 9_007_199_254_740_991.0 && v.fract() == 0.0
    })?;
    format!("{number:.0}").parse().ok()
}
fn timestamp(value: &Value) -> BlueprintResult<DateTime<Utc>> {
    let number = value
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 253_402_300_799.0 && v.fract() == 0.0)
        .ok_or_else(invalid)?;
    let seconds = format!("{number:.0}")
        .parse::<i64>()
        .map_err(|_| invalid())?;
    DateTime::from_timestamp(seconds, 0).ok_or_else(invalid)
}
pub(super) fn subscription(
    value: &Value,
    expected_id: Option<&str>,
    customer_id: &str,
    receipt: &Value,
    request: &BoundSubscriptionCreateRequest,
    mode: bool,
) -> BlueprintResult<(String, DateTime<Utc>)> {
    let id = value["id"]
        .as_str()
        .filter(|id| safe_reference(id) && id.starts_with("sub_"))
        .ok_or_else(invalid)?;
    if expected_id.is_some_and(|v| v != id)
        || value["object"] != "subscription"
        || value["livemode"].as_bool() != Some(mode)
        || value["collection_method"] != "charge_automatically"
        || !matches!(value["status"].as_str(), Some("active" | "incomplete"))
    {
        return Err(invalid());
    }
    deleted(value)?;
    if let Some(id) = value["customer"].as_str() {
        if id != customer_id {
            return Err(invalid());
        }
    } else {
        customer(&value["customer"], customer_id, mode)?;
    }
    absent_or_null(
        value,
        &[
            "application",
            "application_fee_percent",
            "on_behalf_of",
            "transfer_data",
            "schedule",
            "pause_collection",
            "trial_start",
            "trial_end",
            "cancel_at",
            "canceled_at",
            "ended_at",
            "pending_update",
            "test_clock",
            "discount",
        ],
    )?;
    empty_arrays(value, &["discounts", "default_tax_rates"])?;
    if value.get("automatic_tax").is_some_and(|v| {
        v["enabled"].as_bool() != Some(false) || v.get("liability").is_some_and(|v| !v.is_null())
    }) || value
        .get("cancel_at_period_end")
        .is_some_and(|v| v.as_bool() != Some(false))
    {
        return Err(invalid());
    }
    match request.description.as_deref() {
        Some(expected) if value["description"].as_str() != Some(expected) => return Err(invalid()),
        None if value.get("description").is_some_and(|v| !v.is_null()) => return Err(invalid()),
        _ => {}
    }
    let items = value["items"]["data"]
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or_else(invalid)?;
    if value["items"]["object"] != "list" || value["items"]["has_more"].as_bool() != Some(false) {
        return Err(invalid());
    }
    let item = &items[0];
    if item["object"] != "subscription_item"
        || item["id"].as_str().is_none_or(|v| !safe_reference(v))
        || item["subscription"] != id
        || integral(&item["quantity"]) != Some(request.quantity)
    {
        return Err(invalid());
    }
    deleted(item)?;
    empty_arrays(item, &["discounts"])?;
    if item
        .get("tax_rates")
        .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
    {
        return Err(invalid());
    }
    absent_or_null(item, &["current_trial", "billing_thresholds"])?;
    plan(&item["plan"], receipt, mode, false)?;
    // This finite profile requires coherent legacy Plan/Price views. A different
    // price ID or unsupported pricing model is unresolved, never silently used.
    price(&item["price"], receipt, mode)?;
    Ok((id.into(), timestamp(&value["start_date"])?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (Value, Value, BoundSubscriptionCreateRequest) {
        let receipt = json!({"externalId":"custom-plan","price":1200,"currency":"USD","cadence":"month","active":true,"providerFields":{"id":"custom-product","object":"product","livemode":false}});
        let plan = json!({"id":"custom-plan","object":"plan","livemode":false,"active":true,"amount":1200,"currency":"usd","interval":"month","interval_count":1,"usage_type":"licensed","billing_scheme":"per_unit","product":"custom-product"});
        let price = json!({"id":"custom-plan","object":"price","livemode":false,"active":true,"type":"recurring","unit_amount":1200,"currency":"usd","billing_scheme":"per_unit","recurring":{"interval":"month","interval_count":1,"usage_type":"licensed"},"product":"custom-product"});
        let response = json!({"id":"sub_owned","object":"subscription","livemode":false,"customer":"cus_owned","status":"incomplete","collection_method":"charge_automatically","start_date":1_791_936_000,"description":null,"discounts":[],"default_tax_rates":[],"automatic_tax":{"enabled":false},"cancel_at_period_end":false,"items":{"object":"list","has_more":false,"data":[{"id":"si_owned","object":"subscription_item","subscription":"sub_owned","quantity":2,"plan":plan,"price":price,"discounts":[],"tax_rates":null}]}});
        (
            receipt,
            response,
            BoundSubscriptionCreateRequest {
                local_customer_id: uuid::Uuid::new_v4(),
                local_plan_id: uuid::Uuid::new_v4(),
                quantity: 2,
                description: None,
            },
        )
    }
    #[test]
    fn finite_subscription_readback_denies_incoherent_consumed_state() {
        let (receipt, good, request) = fixture();
        assert!(subscription(&good, None, "cus_owned", &receipt, &request, false).is_ok());
        for (path, bad) in [
            ("/id", json!("other")),
            ("/object", json!("invoice")),
            ("/livemode", json!(true)),
            ("/customer", json!("cus_other")),
            ("/customer", json!(null)),
            ("/status", json!("trialing")),
            ("/status", json!("paid")),
            ("/collection_method", json!("send_invoice")),
            ("/start_date", json!(-1)),
            ("/start_date", json!(1.1)),
            ("/start_date", json!(253_402_300_800_u64)),
            ("/items/has_more", json!(true)),
            ("/items/object", json!("price")),
            ("/items/data", json!([])),
            ("/items/data/0/id", json!(" ")),
            ("/items/data/0/object", json!("plan")),
            ("/items/data/0/subscription", json!("sub_other")),
            ("/items/data/0/quantity", json!(1)),
            ("/items/data/0/quantity", json!(2.1)),
            ("/items/data/0/plan/id", json!("different-plan")),
            ("/items/data/0/plan/amount", json!(1201)),
            ("/items/data/0/plan/product", json!("other-product")),
            ("/items/data/0/price/id", json!("price_unrelated")),
            ("/items/data/0/price/unit_amount", json!(1201)),
            ("/items/data/0/price/recurring/usage_type", json!("metered")),
            ("/items/data/0/price/recurring/interval", json!("year")),
            ("/items/data/0/price/product", json!("other-product")),
            ("/items/data/0/price/livemode", json!(true)),
            ("/discounts", json!(["di_extra"])),
            ("/default_tax_rates", json!(["txr_extra"])),
            ("/automatic_tax/enabled", json!(true)),
            ("/cancel_at_period_end", json!(true)),
            ("/items/data/0/discounts", json!(["di_extra"])),
            ("/items/data/0/tax_rates", json!(["txr_extra"])),
        ] {
            let mut value = good.clone();
            *value.pointer_mut(path).unwrap() = bad;
            assert!(
                subscription(
                    &value,
                    Some("sub_owned"),
                    "cus_owned",
                    &receipt,
                    &request,
                    false
                )
                .is_err(),
                "{path}"
            );
        }
        for field in [
            "application_fee_percent",
            "on_behalf_of",
            "transfer_data",
            "trial_start",
            "trial_end",
            "schedule",
            "pause_collection",
            "pending_update",
            "test_clock",
        ] {
            let mut value = good.clone();
            value[field] = json!("unsupported");
            assert!(
                subscription(&value, None, "cus_owned", &receipt, &request, false).is_err(),
                "{field}"
            );
        }
        for literal in ["1791936000.0", "1e3", "-0.0"] {
            let mut value = good.clone();
            value["start_date"] = serde_json::from_str(literal).unwrap();
            value["items"]["data"][0]["quantity"] = serde_json::from_str("2.0").unwrap();
            assert!(subscription(&value, None, "cus_owned", &receipt, &request, false).is_ok());
        }
        let mut active = good;
        active["status"] = json!("active");
        assert!(subscription(&active, None, "cus_owned", &receipt, &request, false).is_ok());
    }
    #[test]
    fn customer_and_expanded_product_are_pre_dispatch_evidence() {
        let (receipt, response, _) = fixture();
        let mut plan = response["items"]["data"][0]["plan"].clone();
        assert!(super::plan(&plan, &receipt, false, true).is_err());
        plan["product"] = receipt["providerFields"].clone();
        assert!(super::plan(&plan, &receipt, false, true).is_ok());
        plan["product"]["deleted"] = json!(true);
        assert!(super::plan(&plan, &receipt, false, true).is_err());
        let good = json!({"id":"cus_owned","object":"customer","livemode":false});
        assert!(customer(&good, "cus_owned", false).is_ok());
        for field in ["discount", "test_clock", "deleted"] {
            let mut value = good.clone();
            value[field] = json!("unsupported");
            assert!(customer(&value, "cus_owned", false).is_err());
        }
    }
}
