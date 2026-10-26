use super::*;
use crate::http::{Body, FrameworkRequest, HttpMethod, RequestContext};
use forklaunch_core::contract::{
    BodyContract, RequestContract, ResponseContract, RouteContract, VersionedContract,
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
fn route() -> RouteContract {
    let mut route = RouteContract::post("Wire", "Wire", "/wire");
    for version in ["first", "last"] {
        let mut body = BodyContract::json("Input");
        body.schema = body.schema.with_json_schema(
            json!({"type":"object","required":["flag"],"properties":{"flag":{"const":"ok"}}})
                .to_string(),
        );
        route = route.with_version(VersionedContract {
            version: version.into(),
            request: RequestContract::default().with_body(body),
            responses: vec![ResponseContract::new(200, None)],
        });
    }
    route
}
fn request(media: &str, body: &str, framed: bool) -> FrameworkRequest {
    let mut request = FrameworkRequest::new(HttpMethod::Post, "/wire", RequestContext::new("wire"))
        .with_body(Body::Text(body.into()));
    request.headers.insert("content-type".into(), media.into());
    if framed {
        request
            .headers
            .insert("content-length".into(), body.len().to_string());
    }
    request
}
#[test]
fn json_wire_skips_before_callbacks_and_defaults_without_changing_raw_bytes() {
    let route = route();
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks.enable_typescript_json_wire(&route).unwrap();
    let verifies = Arc::new(AtomicUsize::new(0));
    let count = verifies.clone();
    hooks
        .insert_parser(
            &route,
            BodyParserHooks::default().with_verify(move |_, _, charset| {
                assert_eq!(charset, Some("utf-8"));
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
    for version in ["first", "last"] {
        hooks
            .insert(
                &route,
                version,
                VersionBodyHooks::default().with_optional_preprocess(|value| {
                    Ok(Some(value.cloned().unwrap_or_else(|| json!({"flag":"ok"}))))
                }),
            )
            .unwrap();
    }
    let plan = hooks.compile(&route).unwrap();
    for (media, body, framed, expected_verify) in [
        ("text/plain", "{", true, 0),
        ("application/problem+json", "{", true, 0),
        ("application/json", "{", false, 0),
        ("application/json", r#"{"flag":"ok"}"#, true, 1),
    ] {
        let request = request(media, body, framed);
        let original = request.body.clone();
        let before = verifies.load(Ordering::SeqCst);
        let (_, selected) = plan.select(&request).unwrap().unwrap();
        assert_eq!(selected.matched_versions, ["first", "last"]);
        assert_eq!(selected.version, "last");
        assert_eq!(selected.parsed_body.as_deref(), Some(&json!({"flag":"ok"})));
        assert_eq!(request.body, original);
        assert_eq!(verifies.load(Ordering::SeqCst) - before, expected_verify);
    }
    assert!(
        plan.select(&request("application/json", "{", true))
            .unwrap_err()
            .iter()
            .all(ContractValidationError::is_parser_error)
    );
    assert!(plan.select(&request("application/json", "", true)).is_err());
    // Native default still parses wire bytes regardless of framing/media.
    assert!(
        VersionedRoutePlan::compile(&route)
            .unwrap()
            .select(&request("text/plain", "{", true))
            .is_err()
    );
}
#[test]
fn json_wire_registration_is_pinned_and_prepared_selection_does_not_repeat_verifier() {
    let route = route();
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks.enable_typescript_json_wire(&route).unwrap();
    assert!(hooks.enable_typescript_json_wire(&route).is_err());
    assert!(
        VersionBodyHooksRegistry::default()
            .enable_typescript_json_wire(&RouteContract::get("No", "No", "/no"))
            .is_err()
    );
    assert!(hooks.validate_routes(&[]).is_err());
    let mut changed = route.clone();
    changed.options.body_parser.json_strict = false;
    assert!(hooks.compile(&changed).is_err());
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    hooks
        .insert_parser(
            &route,
            BodyParserHooks::default().with_verify(move |_, _, _| {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
    let plan = hooks.compile(&route).unwrap();
    let mut request = request("application/json", r#"{"flag":"ok"}"#, true);
    let prepared = plan.prepare_selection(&request, MultipartLimits::default());
    plan.select_reusing(&mut request, Some(&prepared))
        .unwrap()
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn prepared_selection_is_invalidated_by_consuming_plan_mutators() {
    let mut route = route();
    for version in route.versions.values_mut() {
        version
            .request
            .query
            .push(forklaunch_core::contract::FieldContract::optional(
                "query", "String",
            ));
    }
    let plan = VersionedRoutePlan::compile(&route).unwrap();
    let request = request("application/json", r#"{"flag":"ok"}"#, true);
    let prepared = plan.prepare_selection(&request, MultipartLimits::default());
    assert!(plan.clone().resolve_prepared(&prepared, &request).is_ok());
    for changed in [
        plan.clone()
            .with_body_hooks("first", VersionBodyHooks::default())
            .unwrap(),
        plan.clone()
            .with_parser_hooks(BodyParserHooks::default())
            .unwrap(),
        plan.clone()
            .with_field_hooks(
                "first",
                ContractValidationLocation::Query,
                "query",
                VersionFieldHooks::default(),
            )
            .unwrap(),
        plan.with_typescript_json_wire().unwrap(),
    ] {
        assert!(changed.resolve_prepared(&prepared, &request).is_err());
    }
    let mut registry = VersionBodyHooksRegistry::default();
    registry.enable_typescript_json_wire(&route).unwrap();
    let plan = registry.compile(&route).unwrap();
    let prepared = plan.prepare_selection(&request, MultipartLimits::default());
    let changed = plan
        .with_parser_hooks(
            BodyParserHooks::default().with_verify(|_, _, _| Err("REPLACED_VERIFIER".into())),
        )
        .unwrap();
    assert!(changed.resolve_prepared(&prepared, &request).is_err());
    assert!(changed.select(&request).is_err());
}

#[test]
fn unmatched_projection_survives_once_only_preparation_and_ignores_caller_projection() {
    for mode in [
        forklaunch_core::contract::ValidationMode::None,
        forklaunch_core::contract::ValidationMode::Warning,
    ] {
        let mut route = route();
        route.options.request_validation = mode;
        let mut registry = VersionBodyHooksRegistry::default();
        registry.enable_typescript_json_wire(&route).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        registry
            .insert_parser(
                &route,
                BodyParserHooks::default().with_verify(move |_, _, _| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
            )
            .unwrap();
        let plan = registry.compile(&route).unwrap();
        let mut request = request("application/json", r#"{"flag":"invalid"}"#, true);
        let original = request.body.clone();
        let prepared = plan.prepare_selection(&request, MultipartLimits::default());
        assert!(
            plan.apply_prepared(&prepared, &mut request)
                .unwrap()
                .is_err()
        );
        assert!(request.version.is_none());
        let parsed = request.version_parsed_body.as_ref().unwrap();
        assert_eq!(parsed.value(), Some(&json!({"flag":"invalid"})));
        assert_eq!(parsed.raw_body(), Some(br#"{"flag":"invalid"}"#.as_slice()));
        request.version_parsed_body = None;
        assert!(
            plan.apply_prepared(&prepared, &mut request)
                .unwrap()
                .is_err()
        );
        assert!(request.version_parsed_body.is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(request.body, original);
        request.body = Body::Text("changed".into());
        assert!(plan.apply_prepared(&prepared, &mut request).is_err());
    }
}

#[test]
fn parsed_projection_distinguishes_skips_null_and_reviver_root_removal() {
    for (media, bytes, remove, expected, raw) in [
        ("text/plain", "null", false, None, None),
        (
            "application/json",
            "null",
            false,
            Some(json!(null)),
            Some("null"),
        ),
        ("application/json", "{}", true, None, Some("{}")),
    ] {
        let mut route = route();
        route.options.body_parser.json_strict = false;
        let mut registry = VersionBodyHooksRegistry::default();
        registry.enable_typescript_json_wire(&route).unwrap();
        registry
            .insert_parser(
                &route,
                BodyParserHooks::default().with_json_reviver(move |key, value| {
                    Ok(if remove && key.is_empty() {
                        None
                    } else {
                        Some(value)
                    })
                }),
            )
            .unwrap();
        let plan = registry.compile(&route).unwrap();
        let mut request = request(media, bytes, true);
        let prepared = plan.prepare_selection(&request, MultipartLimits::default());
        assert!(
            plan.apply_prepared(&prepared, &mut request)
                .unwrap()
                .is_err()
        );
        let parsed = request.version_parsed_body.unwrap();
        assert_eq!(parsed.value(), expected.as_ref());
        assert_eq!(parsed.raw_body(), raw.map(str::as_bytes));
    }
}

#[test]
fn parsed_projection_keeps_decompressed_input_separate_from_original_wire() {
    let route = route();
    let mut registry = VersionBodyHooksRegistry::default();
    registry.enable_typescript_json_wire(&route).unwrap();
    let plan = registry.compile(&route).unwrap();
    let bytes = vec![
        31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 171, 86, 74, 203, 73, 76, 87, 178, 82, 202, 204, 43, 75,
        204, 201, 76, 81, 170, 5, 0, 121, 3, 144, 118, 18, 0, 0, 0,
    ];
    let mut request = request("application/json", "", true);
    request
        .headers
        .insert("content-encoding".into(), "gzip".into());
    request
        .headers
        .insert("content-length".into(), bytes.len().to_string());
    request.body = Body::Binary(bytes.clone());
    let prepared = plan.prepare_selection(&request, MultipartLimits::default());
    assert!(
        plan.apply_prepared(&prepared, &mut request)
            .unwrap()
            .is_err()
    );
    assert_eq!(
        request.version_parsed_body.unwrap().raw_body(),
        Some(br#"{"flag":"invalid"}"#.as_slice())
    );
    assert_eq!(request.body, Body::Binary(bytes));
}

#[test]
fn direct_selection_overwrites_forged_projection_and_native_default_clears_it() {
    let route = route();
    let mut registry = VersionBodyHooksRegistry::default();
    registry.enable_typescript_json_wire(&route).unwrap();
    let plan = registry.compile(&route).unwrap();
    let mut request = request("application/json", r#"{"flag":"invalid"}"#, true);
    assert!(
        plan.select_into_request(&mut request, MultipartLimits::default())
            .is_err()
    );
    let forged = request.version_parsed_body.clone();
    request.body = Body::Text(r#"{"flag":"ok"}"#.into());
    request.headers.insert("content-length".into(), "13".into());
    assert!(
        plan.select_into_request(&mut request, MultipartLimits::default())
            .is_ok()
    );
    assert_eq!(
        request.version_parsed_body.as_ref().unwrap().value(),
        Some(&json!({"flag":"ok"}))
    );
    request.version_parsed_body = forged;
    let native = VersionedRoutePlan::compile(&route).unwrap();
    assert!(
        native
            .select_into_request(&mut request, MultipartLimits::default())
            .is_ok()
    );
    assert!(request.version_parsed_body.is_none());
}

#[test]
fn optional_presence_does_not_admit_null_and_stale_optional_metadata_rejects() {
    let mut route = route();
    route.options.body_parser.json_strict = false;
    for version in route.versions.values_mut() {
        version.request.body_optional = true;
    }
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks.enable_typescript_json_wire(&route).unwrap();
    let plan = hooks.compile(&route).unwrap();
    let absent = request("application/json", "", false);
    let selected = plan.select(&absent).unwrap().unwrap().1;
    assert_eq!(selected.version, "last");
    assert!(selected.parsed_body.is_none());
    assert!(
        plan.select(&request("application/json", "null", true))
            .is_err()
    );
    assert!(
        plan.select(&request("application/json", "{}", true))
            .is_err()
    );
    let mut changed = route.clone();
    changed
        .versions
        .get_mut("last")
        .unwrap()
        .request
        .body_optional = false;
    assert!(hooks.compile(&changed).is_err());
}
