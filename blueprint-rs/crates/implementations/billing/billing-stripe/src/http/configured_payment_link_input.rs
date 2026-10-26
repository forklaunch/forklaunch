//! Selected configured `PaymentLink` update's declared Number preprocessing.
use forklaunch_axum::RouterConfig;
use forklaunch_core::contract::RouteContract;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::typescript_request_validation_error_response;
use forklaunch_core::http::{BodyParserHooks, FrameworkResponse};
use forklaunch_validator::typescript_coerce::number;
use serde_json::{Value, json};
use std::sync::Arc;

/// Request metadata for configured Stripe update only. Number preprocessing
/// accepts JSON values beyond numbers; the callback determines successful inputs.
/// Response amounts and all other operations retain their existing schemas.
/// # Errors
/// Returns an error for malformed package-owned request schemas.
pub fn project_configured_payment_link_update(route: &mut RouteContract) -> BlueprintResult<()> {
    if route.operation_name != "updatePaymentLink" {
        return Ok(());
    }
    let Some(text) = route
        .request
        .body
        .as_mut()
        .and_then(|body| body.schema.json_schema.as_mut())
    else {
        return Ok(());
    };
    let mut schema: Value = serde_json::from_str(text).map_err(|_| super::invalid())?;
    schema["properties"]["amount"] = json!({
        "description":"Optional JSON value preprocessed with JavaScript Number. NaN conversions fail admission. Requested amount is not forwarded to Stripe and is replaced by the returned line-item sum.",
        "x-forklaunch-coercion":"typescript-number"
    });
    *text = schema.to_string();
    Ok(())
}

pub(super) fn register(config: &mut RouterConfig, route: &mut RouteContract) {
    let original = route.clone();
    project_configured_payment_link_update(route).expect("package-owned configured amount schema");
    config
        .json_request_parsers
        .insert(route, BodyParserHooks::default())
        .expect("package-owned configured update parser");
    let rejection = route
        .responses
        .iter()
        .find(|response| response.status_code == 400)
        .expect("declared billing rejection")
        .clone();
    config
        .request_validators
        .insert_json_transform(
            route,
            &config.json_request_parsers,
            rejection,
            move |request| {
                let mut validated = request.clone();
                let Some(parsed) = validated.parsed_json.as_mut() else {
                    return Err(FrameworkResponse::text(400, "Invalid billing request"));
                };
                let Some(value) = parsed.value.as_deref() else {
                    return Err(FrameworkResponse::text(400, "Invalid billing request"));
                };
                let mut value = value.clone();
                if let Some(fields) = value.as_object_mut()
                    && let Some(amount) = fields.get("amount")
                {
                    let coerced = number(Some(amount)).map_err(|mut error| {
                        error.path = vec!["body".into(), "amount".into()];
                        typescript_request_validation_error_response(
                            Some(&[error]),
                            Some(&request.context.correlation_id),
                        )
                    })?;
                    if let Some(amount) = serde_json::Number::from_f64(coerced) {
                        fields.insert("amount".into(), Value::Number(amount));
                    } else {
                        // JSON has no Infinity. This field has no downstream effect:
                        // the configured wrapper validates it, does not forward it and
                        // replaces it with the provider's finite line-item total.
                        fields.remove("amount");
                    }
                }
                parsed.value = Some(Arc::new(value.clone()));
                let errors =
                    forklaunch_core::http::validate_framework_request(&original, &validated);
                if !errors.is_empty() {
                    return Err(FrameworkResponse::text(
                        400,
                        forklaunch_core::http::validation_errors_message(&errors),
                    ));
                }
                Ok(Some(value))
            },
        )
        .expect("package-owned configured update transformation");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_changes_only_selected_request_amount() {
        for stripe in [false, true] {
            for original in
                forklaunch_billing_interface::module_contracts::module_registry(stripe).routes()
            {
                let mut projected = original.clone();
                project_configured_payment_link_update(&mut projected).unwrap();
                if original.operation_name != "updatePaymentLink" {
                    assert_eq!(&projected, original);
                    continue;
                }
                assert_eq!(projected.responses, original.responses);
                let mut before: Value = serde_json::from_str(
                    original
                        .request
                        .body
                        .as_ref()
                        .unwrap()
                        .schema
                        .json_schema
                        .as_ref()
                        .unwrap(),
                )
                .unwrap();
                let mut after: Value = serde_json::from_str(
                    projected
                        .request
                        .body
                        .as_ref()
                        .unwrap()
                        .schema
                        .json_schema
                        .as_ref()
                        .unwrap(),
                )
                .unwrap();
                before["properties"]
                    .as_object_mut()
                    .unwrap()
                    .remove("amount");
                after["properties"]
                    .as_object_mut()
                    .unwrap()
                    .remove("amount");
                assert_eq!(before, after);
            }
        }
    }
}
