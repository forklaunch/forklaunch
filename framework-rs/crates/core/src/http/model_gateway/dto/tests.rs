use super::*;
use serde_json::json;
#[test]
fn optional_unknown_and_nullable_properties_preserve_absence_and_null() {
    for content in [None, Some(Value::Null), Some(json!("text"))] {
        let mut wire = json!({"extra": [1, null]});
        if let Some(content) = content {
            wire["content"] = content;
        }
        let dto: GatewayChatDelta = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(dto).unwrap(), wire);
    }
    for value in [
        json!({"model":"m","messages":[]}),
        json!({"model":"m","messages":[],"stop":null,"response_format":null,"tool_choice":null}),
    ] {
        let dto: GatewayChatRequest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(dto).unwrap(), value);
    }
}
#[test]
fn typed_required_fields_and_collisions_fail_without_silently_overriding() {
    assert!(serde_json::from_value::<GatewayChatMessage>(json!({"role":"user"})).is_err());
    assert!(
        serde_json::from_value::<GatewayCompletionMessage>(json!({"role":"assistant"})).is_err()
    );
    assert!(
        serde_json::from_value::<GatewayChatCompletion>(
            json!({"id":"x","object":"x","model":"m","choices":[]})
        )
        .is_err()
    );
    let mut dto: GatewayChatDelta = serde_json::from_value(json!({})).unwrap();
    dto.extra.insert("content".into(), json!("collision"));
    assert!(serde_json::to_value(dto).is_err());
    let mut dto: GatewayChatRequest =
        serde_json::from_value(json!({"model":"m","messages":[]})).unwrap();
    dto.extra.insert("model".into(), json!("collision"));
    assert!(serde_json::to_value(dto).is_err());
    let mut catalog: GatewayModels = serde_json::from_value(
        json!({"models":[],"monthlyTokenBudget":0,"tokensUsedThisMonth":0,"requestsPerMinute":0}),
    )
    .unwrap();
    catalog.extra.insert("withheldForHipaa".into(), json!([]));
    assert!(serde_json::to_value(catalog).is_err());
    assert!(
        serde_json::from_value::<GatewayChatRequest>(
            json!({"model":"m","messages":[],"temperature":null})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<GatewayChatMessage>(json!({"role":"invalid","content":null}))
            .is_err()
    );
}
#[test]
fn numeric_fields_are_finite_browser_numbers() {
    let mut dto: GatewayUsage = serde_json::from_str(
        r#"{"prompt_tokens":9007199254740993,"completion_tokens":-0,"total_tokens":1.5}"#,
    )
    .unwrap();
    assert_eq!(
        dto.prompt_tokens.to_bits(),
        9_007_199_254_740_992_f64.to_bits()
    );
    assert_eq!(dto.completion_tokens.to_bits(), (-0.0_f64).to_bits());
    dto.total_tokens = f64::INFINITY;
    assert!(serde_json::to_value(dto).is_err());
    let mut request: GatewayChatRequest =
        serde_json::from_value(json!({"model":"m","messages":[]})).unwrap();
    request.temperature = GatewayPresence::Present(f64::NAN);
    assert!(serde_json::to_value(request).is_err());
}
