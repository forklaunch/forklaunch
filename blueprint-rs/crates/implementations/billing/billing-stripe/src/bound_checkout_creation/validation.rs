use super::invalid;
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
pub(super) fn price(value: &Value, receipt: &Value, mode: bool) -> BlueprintResult<()> {
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
pub(super) fn integer(value: &Value) -> BlueprintResult<u64> {
    let n = value
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0 && *n <= 9_007_199_254_740_991.0 && n.fract() == 0.0)
        .ok_or_else(invalid)?;
    format!("{n:.0}").parse().map_err(|_| invalid())
}
pub(super) fn timestamp(value: &Value) -> BlueprintResult<DateTime<Utc>> {
    let n = integer(value)?;
    let n = i64::try_from(n).map_err(|_| invalid())?;
    DateTime::from_timestamp(n, 0)
        .filter(|d| *d > DateTime::UNIX_EPOCH)
        .ok_or_else(invalid)
}
pub(super) fn session(
    value: &Value,
    profile: &super::BoundCheckoutCreationProfile,
    customer: &str,
    receipt: &Value,
    mode: bool,
    total: u64,
) -> BlueprintResult<()> {
    if !value["id"].as_str().is_some_and(safe_reference)
        || value["object"] != "checkout.session"
        || value["livemode"].as_bool() != Some(mode)
        || value["customer"] != customer
        || value["mode"] != "subscription"
        || value["ui_mode"] != "hosted_page"
        || value["status"] != "open"
        || value["payment_status"] != "unpaid"
        || value["payment_method_types"] != serde_json::json!(["card"])
        || value["success_url"] != profile.success_url
        || value["cancel_url"] != profile.cancel_url
        || value["currency"]
            != receipt["currency"]
                .as_str()
                .ok_or_else(invalid)?
                .to_ascii_lowercase()
        || integer(&value["amount_subtotal"])? != total
        || integer(&value["amount_total"])? != total
        || value["automatic_tax"]["enabled"] != false
        || value["allow_promotion_codes"] != false
        || !value["url"].as_str().is_some_and(|s| {
            s.len() <= 65_536
                && !s
                    .trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
                    .is_empty()
        })
    {
        return Err(invalid());
    }
    for field in [
        "subscription",
        "payment_intent",
        "setup_intent",
        "payment_link",
        "invoice",
        "recovered_from",
    ] {
        if value.get(field) != Some(&Value::Null) {
            return Err(invalid());
        }
    }
    absent_or_null(
        value,
        &[
            "customer_account",
            "return_url",
            "client_secret",
            "shipping_cost",
        ],
    )?;
    empty_arrays(value, &["discounts", "shipping_options"])?;
    for field in ["amount_discount", "amount_shipping", "amount_tax"] {
        if integer(&value["total_details"][field])? != 0 {
            return Err(invalid());
        }
    }
    let created = timestamp(&value["created"])?;
    let expires = timestamp(&value["expires_at"])?;
    let duration = expires.signed_duration_since(created).num_seconds();
    if !(1800..=86400).contains(&duration) || expires <= Utc::now() {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn line_items(
    value: &Value,
    id: &str,
    receipt: &Value,
    mode: bool,
    quantity: u64,
    total: u64,
) -> BlueprintResult<()> {
    if value["object"] != "list"
        || value["has_more"] != false
        || value["url"] != format!("/v1/checkout/sessions/{id}/line_items")
    {
        return Err(invalid());
    }
    let data = value["data"]
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or_else(invalid)?;
    let item = &data[0];
    if item["object"] != "item"
        || !item["id"].as_str().is_some_and(safe_reference)
        || integer(&item["quantity"])? != quantity
        || integer(&item["amount_subtotal"])? != total
        || integer(&item["amount_total"])? != total
        || integer(&item["amount_discount"])? != 0
        || integer(&item["amount_tax"])? != 0
        || item["currency"]
            != receipt["currency"]
                .as_str()
                .ok_or_else(invalid)?
                .to_ascii_lowercase()
    {
        return Err(invalid());
    }
    price(&item["price"], receipt, mode)
}
