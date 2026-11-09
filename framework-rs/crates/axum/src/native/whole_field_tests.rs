use super::*;
use forklaunch_core::contract::{
    HttpMethod,
    endpoint::{Fields, NoBody, TextBody},
};
use tower::ServiceExt;
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    start: Option<String>,
    end: Option<String>,
}
#[derive(serde::Deserialize)]
struct Headers {
    #[serde(rename = "X-Start")]
    start: Option<String>,
    #[serde(rename = "X-End")]
    end: Option<String>,
}
#[tokio::test]
async fn whole_field_constraints_validate_decoded_groups() {
    let endpoint = Endpoint::<Fields<NoBody, Query, Headers>, TextBody>::new(
        "Range",
        "Range",
        HttpMethod::Get,
        "/range",
        200,
    )
    .unwrap();
    assert!(endpoint.contract().request.query_schema.is_some());
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let app = ContractRouter::<()>::new(registry)
        .bind(endpoint, |_, request| async move {
            Ok(format!(
                "{:?}:{:?}:{:?}:{:?}",
                request.query.start, request.query.end, request.headers.start, request.headers.end
            ))
        })
        .unwrap()
        .finish()
        .unwrap();
    for (path, start, end, status) in [
        ("/range", false, false, 200),
        ("/range?start=a", false, false, 400),
        ("/range?start=a&end=b", false, false, 200),
        ("/range?start=a&end=b&unknown=c", false, false, 400),
        ("/range", true, false, 400),
        ("/range", true, true, 200),
    ] {
        let mut builder = axum::http::Request::builder().uri(path);
        if start {
            builder = builder.header("x-start", "a");
        }
        if end {
            builder = builder.header("x-end", "b");
        }
        let response = app
            .clone()
            .oneshot(builder.body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status, "{path} {start} {end}");
    }
}

impl schemars::JsonSchema for Query {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Query".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        serde_json::from_value(serde_json::json!({"type":"object","properties":{"start":{"type":"string"},"end":{"type":"string"}},"additionalProperties":false,"dependencies":{"start":["end"]}})).unwrap()
    }
}
impl schemars::JsonSchema for Headers {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Headers".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        serde_json::from_value(serde_json::json!({"type":"object","properties":{"X-Start":{"type":"string"},"X-End":{"type":"string"}},"additionalProperties":false,"dependencies":{"X-Start":["X-End"]}})).unwrap()
    }
}
