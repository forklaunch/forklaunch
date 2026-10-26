use super::*;
use serde_json::json;
#[test]
fn catalog_bounds_and_numeric_spellings_match_safe_javascript_integers() {
    let now = 1_800_000_000_000;
    let valid = json!({"models":[{"id":"model_.:-AZ09"}],"preview":{"expiresAt":now+1,"maxCalls":1,"maxUnits":100_000}});
    assert!(catalog(&valid, now).is_ok());
    let fractional:Value=serde_json::from_str(r#"{"models":[{"id":"a"}],"preview":{"expiresAt":1800000000001.0,"maxCalls":2e1,"maxUnits":1e5}}"#).unwrap();
    assert_eq!(catalog(&fractional, now).unwrap().preview.max_calls, 20);
    for (field, value) in [
        ("expiresAt", json!(now)),
        ("expiresAt", json!(now + 900_001)),
        ("expiresAt", json!(9_007_199_254_740_992_u64)),
        ("maxCalls", json!(0)),
        ("maxCalls", json!(21)),
        ("maxCalls", json!(1.5)),
        ("maxUnits", json!(0)),
        ("maxUnits", json!(100_001)),
        ("extra", json!(true)),
    ] {
        let mut bad = valid.clone();
        bad["preview"][field] = value;
        assert!(catalog(&bad, now).is_err());
    }
    for id in ["", "a/b", "é", "a b"] {
        let mut bad = valid.clone();
        bad["models"][0]["id"] = json!(id);
        assert!(catalog(&bad, now).is_err());
    }
    let mut boundary = valid.clone();
    boundary["preview"]["expiresAt"] = json!(now + 900_000);
    boundary["models"][0]["id"] = json!("x".repeat(160));
    assert!(catalog(&boundary, now).is_ok());
    boundary["models"][0]["id"] = json!("x".repeat(161));
    assert!(catalog(&boundary, now).is_err());
    for bad in [
        json!(null),
        json!([]),
        json!({"models":[],"preview":valid["preview"]}),
        json!({"models":[{"id":"a","baa":true}],"preview":valid["preview"]}),
        json!({"models":[{"id":"a"}],"preview":valid["preview"],"monthlyTokenBudget":1}),
    ] {
        assert!(catalog(&bad, now).is_err());
    }
}
