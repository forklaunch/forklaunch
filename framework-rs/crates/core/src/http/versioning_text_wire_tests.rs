use super::*;
use crate::http::{Body, FrameworkRequest, HttpMethod, RequestContext};
use forklaunch_core::contract::{
    BodyContract, RequestContract, ResponseContract, RouteContract, ValidationMode,
    VersionedContract,
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
fn route() -> RouteContract {
    let mut body = BodyContract::json("Text");
    body.kind = BodyKind::Text;
    body.content_type = Some("text/plain".into());
    body.schema = body
        .schema
        .with_json_schema(r#"{"type":"string","const":"ok"}"#);
    RouteContract::post("Text", "Text", "/text").with_version(VersionedContract {
        version: "one".into(),
        request: RequestContract::default().with_body(body),
        responses: vec![ResponseContract::new(200, None)],
    })
}
fn request(media: &str, body: &str, framed: bool) -> FrameworkRequest {
    let mut request = FrameworkRequest::new(HttpMethod::Post, "/text", RequestContext::new("text"))
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
fn text_wire_distinguishes_absent_empty_and_unmatched_without_replaying_callbacks() {
    for mode in [
        ValidationMode::None,
        ValidationMode::Warning,
        ValidationMode::Error,
    ] {
        let mut route = route();
        route.options.request_validation = mode;
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let mut hooks = VersionBodyHooksRegistry::default();
        hooks.enable_typescript_text_wire(&route).unwrap();
        hooks
            .insert_parser(
                &route,
                BodyParserHooks::default().with_verify(move |_, _, charset| {
                    assert_eq!(charset, Some("utf-8"));
                    counted.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
            )
            .unwrap();
        hooks
            .insert(
                &route,
                "one",
                VersionBodyHooks::default().with_optional_preprocess(|value| {
                    Ok(Some(value.cloned().unwrap_or(json!("ok"))))
                }),
            )
            .unwrap();
        let plan = hooks.compile(&route).unwrap();
        for (media, body, framed, matched, value, raw) in [
            (
                "text/plain",
                "ok",
                true,
                true,
                Some(json!("ok")),
                Some("ok"),
            ),
            (
                "text/plain",
                "bad",
                true,
                false,
                Some(json!("bad")),
                Some("bad"),
            ),
            ("text/plain", "", true, false, Some(json!("")), Some("")),
            ("text/plain", "", false, true, None, None),
            ("application/json", "bad", true, true, None, None),
        ] {
            let mut request = request(media, body, framed);
            let original = request.body.clone();
            let before = calls.load(Ordering::SeqCst);
            let prepared = plan.prepare_selection(&request, MultipartLimits::default());
            let selected = plan.apply_prepared(&prepared, &mut request).unwrap();
            assert_eq!(selected.is_ok(), matched);
            let parsed = request.version_parsed_body.as_ref().unwrap();
            assert_eq!(parsed.value(), value.as_ref());
            assert_eq!(parsed.raw_body(), raw.map(str::as_bytes));
            assert_eq!(
                plan.apply_prepared(&prepared, &mut request)
                    .unwrap()
                    .is_ok(),
                matched
            );
            assert_eq!(
                calls.load(Ordering::SeqCst) - before,
                usize::from(raw.is_some())
            );
            assert_eq!(request.body, original);
        }
        let request = request("text/plain", "ok", true);
        let prepared = plan.prepare_selection(&request, MultipartLimits::default());
        let changed = plan
            .clone()
            .with_parser_hooks(
                BodyParserHooks::default().with_verify(|_, _, _| Err("private".into())),
            )
            .unwrap();
        assert!(changed.resolve_prepared(&prepared, &request).is_err());
        assert!(changed.select(&request).unwrap_err()[0].is_parser_error());
    }
}
#[test]
fn text_wire_rejects_stale_conflicting_profiles_and_json_revivers() {
    let route = route();
    let mut registry = VersionBodyHooksRegistry::default();
    registry.enable_typescript_text_wire(&route).unwrap();
    assert!(registry.enable_typescript_text_wire(&route).is_err());
    assert!(registry.validate_routes(&[]).is_err());
    let mut stale = route.clone();
    stale.options.body_parser.text_wire.limit_bytes = 42;
    assert!(registry.compile(&stale).is_err());
    let mut json_route = route.clone();
    let version = json_route.versions.get_mut("one").unwrap();
    version.request.body.as_mut().unwrap().kind = BodyKind::Json;
    assert!(registry.enable_typescript_json_wire(&json_route).is_err());
    let mut reviver = VersionBodyHooksRegistry::default();
    reviver.enable_typescript_text_wire(&route).unwrap();
    assert!(
        reviver
            .insert_parser(
                &route,
                BodyParserHooks::default().with_json_reviver(|_, value| Ok(Some(value))),
            )
            .is_err()
    );
    assert!(reviver.compile(&route).is_ok());
    assert!(
        VersionBodyHooksRegistry::default()
            .enable_typescript_text_wire(&RouteContract::get("No", "No", "/no"))
            .is_err()
    );
    let mut media = route;
    media
        .versions
        .get_mut("one")
        .unwrap()
        .request
        .body
        .as_mut()
        .unwrap()
        .content_type = Some("text/*".into());
    assert!(
        VersionBodyHooksRegistry::default()
            .enable_typescript_text_wire(&media)
            .is_err()
    );
}

#[test]
fn text_wire_retains_original_compressed_bytes_and_bounds_before_verification() {
    let route = route();
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks.enable_typescript_text_wire(&route).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    hooks
        .insert_parser(
            &route,
            BodyParserHooks::default().with_verify(move |_, bytes, _| {
                assert_eq!(bytes, b"ok");
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
    let plan = hooks.compile(&route).unwrap();
    let compressed = vec![
        31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 203, 207, 6, 0, 71, 221, 220, 121, 2, 0, 0, 0,
    ];
    let mut request = request("text/plain", "", true);
    request.body = Body::Binary(compressed.clone());
    request
        .headers
        .insert("content-encoding".into(), "gzip".into());
    request
        .headers
        .insert("content-length".into(), compressed.len().to_string());
    let prepared = plan.prepare_selection(&request, MultipartLimits::default());
    assert!(
        plan.apply_prepared(&prepared, &mut request)
            .unwrap()
            .is_ok()
    );
    assert_eq!(
        request.version_parsed_body.as_ref().unwrap().raw_body(),
        Some(b"ok".as_slice())
    );
    assert_eq!(request.body, Body::Binary(compressed));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let mut bounded = route;
    bounded.options.body_parser.text_wire.limit_bytes = 1;
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks.enable_typescript_text_wire(&bounded).unwrap();
    hooks
        .insert_parser(
            &bounded,
            BodyParserHooks::default()
                .with_verify(|_, _, _| panic!("oversize input must not reach verifier")),
        )
        .unwrap();
    assert!(
        hooks
            .compile(&bounded)
            .unwrap()
            .select(&request)
            .unwrap_err()[0]
            .is_parser_error()
    );
}
