use super::*;
use crate::http::{FrameworkRequest, HttpMethod, RequestContext};
use forklaunch_core::contract::{FieldContract, RequestContract, SchemaRef, VersionedContract};
use serde_json::json;
fn route() -> RouteContract {
    let mut route = RouteContract::get("query", "query", "/query");
    for (name, schema) in [
        ("array", json!({"type":"array","items":{"type":"string"}})),
        ("scalar", json!({"type":"string"})),
    ] {
        let mut field = FieldContract::required("tag", "Tag");
        field.schema = SchemaRef::named("Tag").with_json_schema(schema.to_string());
        route = route.with_version(VersionedContract {
            version: name.into(),
            request: RequestContract {
                query: vec![field],
                ..RequestContract::default()
            },
            responses: vec![],
        });
    }
    route
}
fn request(raw: Option<&str>) -> FrameworkRequest {
    let mut request =
        FrameworkRequest::new(HttpMethod::Get, "/query", RequestContext::new("query"));
    request.raw_query = raw.map(str::to_owned);
    request.query.insert("tag".into(), "original-map".into());
    request
}
#[test]
fn simple_query_is_opt_in_and_hooks_receive_duplicates_without_changing_wire() {
    let route = route();
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks.enable_typescript_simple_query(&route).unwrap();
    hooks
        .insert_field(
            &route,
            "array",
            ContractValidationLocation::Query,
            "tag",
            VersionFieldHooks::default().with_transform(|input| {
                assert_eq!(input, &json!(["a", "b"]));
                Ok(json!(["transformed"]))
            }),
        )
        .unwrap();
    let request = request(Some("tag=a&tag=b"));
    let before = request.clone();
    let (_, selected) = hooks
        .compile(&route)
        .unwrap()
        .select(&request)
        .unwrap()
        .unwrap();
    assert_eq!(selected.version, "array");
    assert_eq!(selected.parsed_fields.query["tag"], json!(["transformed"]));
    assert_eq!(request, before);
    let (_, native) = VersionedRoutePlan::compile(&route)
        .unwrap()
        .select(&request)
        .unwrap()
        .unwrap();
    assert_eq!(native.version, "scalar");
    assert_eq!(native.parsed_fields.query["tag"], json!("original-map"));
}
#[test]
fn query_limit_counts_empty_segments_and_absence_never_uses_native_map() {
    let route = route();
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks.enable_typescript_simple_query(&route).unwrap();
    let plan = hooks.compile(&route).unwrap();
    assert!(plan.select(&request(None)).is_err());
    let inside = format!("{}tag=yes", "&".repeat(999));
    let outside = format!("{}tag=yes", "&".repeat(1000));
    assert!(plan.select(&request(Some(&inside))).is_ok());
    assert!(plan.select(&request(Some(&outside))).is_err());
    for (raw, expected) in [
        ("tag=%FF&tag=%GG", json!(["�", "%GG"])),
        ("tag=&tag=", json!(["", ""])),
    ] {
        let (_, selected) = plan.select(&request(Some(raw))).unwrap().unwrap();
        assert_eq!(selected.parsed_fields.query["tag"], expected);
    }
}
#[test]
fn query_registration_rejects_duplicate_stale_missing_and_unversioned_routes() {
    let mut route = route();
    let mut hooks = VersionBodyHooksRegistry::default();
    assert!(
        hooks
            .enable_typescript_simple_query(&RouteContract::get("plain", "plain", "/plain"))
            .is_err()
    );
    hooks.enable_typescript_simple_query(&route).unwrap();
    assert!(hooks.enable_typescript_simple_query(&route).is_err());
    assert!(hooks.validate_routes(&[]).is_err());
    assert!(
        hooks
            .validate_routes(&[route.clone(), route.clone()])
            .is_err()
    );
    route.path = "/mounted/query".into();
    assert!(hooks.compile(&route).is_ok());
    route
        .versions
        .get_mut("scalar")
        .unwrap()
        .request
        .query
        .clear();
    assert!(hooks.compile(&route).is_err());
}

#[test]
fn absent_http_query_can_use_optional_default_without_reading_collapsed_map() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let mut route = route();
    route.versions.get_mut("scalar").unwrap().request.query[0].required = false;
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks.enable_typescript_simple_query(&route).unwrap();
    hooks
        .insert_field(
            &route,
            "scalar",
            ContractValidationLocation::Query,
            "tag",
            VersionFieldHooks::default().with_optional_preprocess(move |input| {
                counted.fetch_add(1, Ordering::SeqCst);
                assert!(input.is_none());
                Ok(Some(json!("schema-default")))
            }),
        )
        .unwrap();
    let (_, selected) = hooks
        .compile(&route)
        .unwrap()
        .select(&request(None))
        .unwrap()
        .unwrap();
    assert_eq!(selected.parsed_fields.query["tag"], json!("schema-default"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn no_match_rejects_before_handler_without_repeating_field_callbacks() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let route = route();
    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = Arc::new(AtomicUsize::new(0));
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks.enable_typescript_simple_query(&route).unwrap();
    for version in ["array", "scalar"] {
        let calls = calls.clone();
        hooks
            .insert_field(
                &route,
                version,
                ContractValidationLocation::Query,
                "tag",
                VersionFieldHooks::default().with_optional_preprocess(move |input| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    assert!(input.is_none());
                    Ok(None)
                }),
            )
            .unwrap();
    }
    let counted = handler_calls.clone();
    let mut local = crate::http::LocalRouter::new();
    local.register_with_body_hooks(route, &hooks, move |_: FrameworkRequest| {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(crate::http::FrameworkResponse::text(
            200,
            "handler must not run",
        ))
    });
    assert_eq!(local.execute(request(None)).unwrap().status_code, 400);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(handler_calls.load(Ordering::SeqCst), 0);
}
