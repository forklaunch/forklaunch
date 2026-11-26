use super::super::*;
use axum::body::{Body, to_bytes};
use axum::extract::OriginalUri;
use forklaunch_core::auth::Principal;
use forklaunch_core::contract::{
    BodyContract, RequestContract, ResponseContract, VersionedContract,
};
use forklaunch_core::http::{VersionBodyHooks, VersionBodyHooksRegistry};
use serde_json::json;

#[tokio::test]
async fn ordered_case_dispatch_preserves_methods_uri_and_overlap_options() {
    let config = RouterConfig::default()
        .with_typescript_case_insensitive_path("/head/:id")
        .with_typescript_case_insensitive_path("/overlap/:id")
        .with_typescript_case_insensitive_path("/overlap/fixed")
        .with_typescript_options_for_path("/overlap/:id")
        .with_typescript_options_for_path("/overlap/fixed");
    let app = ForkLaunchRouter::new(ServiceInfo::new("ordered", "1"))
        .get(
            RouteContract::get("GetHead", "GetHead", "/head/:id").with_response(200, None),
            |OriginalUri(uri): OriginalUri, method: axum::http::Method| async move {
                (
                    [
                        ("x-selected", "get"),
                        ("x-method", method.as_str()),
                        ("x-original", uri.path()),
                    ],
                    "get",
                )
                    .into_response()
            },
        )
        .head(
            RouteContract::head("HeadLater", "HeadLater", "/head/:id").with_response(200, None),
            || async { ([("x-selected", "head")], "head") },
        )
        .get(
            RouteContract::get("Param", "Param", "/overlap/:id").with_response(200, None),
            || async { "parameter" },
        )
        .get(
            RouteContract::get("Static", "Static", "/overlap/fixed").with_response(200, None),
            || async { "static" },
        )
        .post(
            RouteContract::post("Post", "Post", "/overlap/fixed").with_response(200, None),
            || async { "post" },
        )
        .with_config(config)
        .into_router();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/HEAD/MiXeD")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["x-selected"], "get");
    assert_eq!(response.headers()["x-method"], "HEAD");
    assert_eq!(response.headers()["x-original"], "/HEAD/MiXeD");
    assert!(
        to_bytes(response.into_body(), 4096)
            .await
            .unwrap()
            .is_empty()
    );
    for (method, expected) in [
        ("GET", "parameter"),
        ("POST", "post"),
        ("OPTIONS", "GET, HEAD, POST"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/OVERLAP/FIXED")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            to_bytes(response.into_body(), 4096).await.unwrap(),
            expected
        );
    }
    let response = app
        .oneshot(
            Request::builder()
                .method("BREW")
                .uri("/OVERLAP/FIXED")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 405);
}

#[tokio::test]
async fn ordered_head_uses_get_auth_and_version_contract_with_original_head() {
    use forklaunch_core::contract::{
        AccessLevel, RequestContract, ResponseContract, VersionedContract,
    };
    use forklaunch_core::http::SelectedVersion;
    let get = RouteContract::get("GetVersion", "GetVersion", "/version/:id")
        .with_access(AccessLevel::Authenticated)
        .with_version(VersionedContract {
            version: "get-version".into(),
            request: RequestContract::default(),
            responses: vec![ResponseContract::new(201, None)],
        });
    let head = RouteContract::head("HeadVersion", "HeadVersion", "/version/:id").with_version(
        VersionedContract {
            version: "head-version".into(),
            request: RequestContract::default(),
            responses: vec![ResponseContract::new(202, None)],
        },
    );
    let config = RouterConfig::default()
        .with_typescript_case_insensitive_path("/version/:id")
        .with_authenticator(|request, contract| async move {
            assert_eq!(request.method, HttpMethod::Head);
            assert_eq!(contract.method, HttpMethod::Get);
            assert_eq!(request.raw_path.as_deref(), Some("/VERSION/MiXeD"));
            assert_eq!(request.params.get("id").map(String::as_str), Some("MiXeD"));
            if request.headers.get("authorization").map(String::as_str) != Some("verified") {
                return Err(AuthError::MissingCredentials);
            }
            Ok(AuthenticatedSession::new(
                Principal::new("actor"),
                serde_json::json!({}),
            ))
        });
    let app = ForkLaunchRouter::anonymous()
        .get(
            get,
            |axum::Extension(version): axum::Extension<SelectedVersion>| async move {
                assert_eq!(version.version, "get-version");
                (
                    StatusCode::CREATED,
                    [("x-version", "get-version")],
                    "suppressed",
                )
            },
        )
        .head(head, || async { StatusCode::ACCEPTED })
        .with_config(config)
        .into_router();
    for authorized in [false, true] {
        let mut request = Request::builder().method("HEAD").uri("/VERSION/MiXeD");
        if authorized {
            request = request.header("authorization", "verified");
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), if authorized { 201 } else { 401 });
        if authorized {
            assert_eq!(response.headers()["x-version"], "get-version");
        }
        assert!(
            to_bytes(response.into_body(), 4096)
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
async fn case_selection_preserves_unselected_auth_and_decoded_params_without_rewriting_uri() {
    let app = ForkLaunchRouter::anonymous()
        .get(
            RouteContract::get("Strict", "Strict", "/users/fixed")
                .with_access(forklaunch_core::contract::AccessLevel::Authenticated)
                .with_response(200, None),
            || async { "strict" },
        )
        .get(
            RouteContract::get("Selected", "Selected", "/users/:id").with_response(200, None),
            |OriginalUri(uri): OriginalUri,
             axum::extract::Path(id): axum::extract::Path<String>| async move {
                format!("{id}|{uri}")
            },
        )
        .with_config(RouterConfig::default().with_typescript_case_insensitive_path("/users/:id"))
        .into_router();
    for (path, status, body) in [
        ("/users/fixed", 401, None),
        ("/USERS/fixed", 200, Some("fixed|/USERS/fixed")),
        (
            "/USERS/MiXeD%2FaBc?Keep=VaLuE",
            200,
            Some("MiXeD/aBc|/USERS/MiXeD%2FaBc?Keep=VaLuE"),
        ),
        ("/%75sers/fixed", 404, None),
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{path}");
        if let Some(body) = body {
            assert_eq!(to_bytes(response.into_body(), 4096).await.unwrap(), body);
        }
    }
}

#[tokio::test]
async fn selected_case_coexists_with_unselected_wildcards_and_typescript_fallback() {
    for format in [
        RouterOriginFormat::Native,
        RouterOriginFormat::TypeScriptHtml,
    ] {
        let app = ForkLaunchRouter::anonymous()
            .get(
                RouteContract::get("Wildcard", "Wildcard", "/files/{*rest}")
                    .with_response(200, None),
                || async { "wildcard" },
            )
            .get(
                RouteContract::get("SelectedFile", "SelectedFile", "/files/:id")
                    .with_response(200, None),
                || async { "selected" },
            )
            .get(
                RouteContract::get("Other", "Other", "/other").with_response(200, None),
                || async { "other" },
            )
            .with_config(RouterConfig {
                router_origin_format: format,
                ..RouterConfig::default().with_typescript_case_insensitive_path("/files/:id")
            })
            .into_router();
        for (path, expected) in [
            ("/files/One", "wildcard"),
            ("/files/One/Two/", "wildcard"),
            ("/FILES/One", "selected"),
            ("/other", "other"),
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), 200, "{path}");
            assert_eq!(
                to_bytes(response.into_body(), 4096).await.unwrap(),
                expected
            );
        }
        let response = app
            .oneshot(
                Request::builder()
                    .method("BREW")
                    .uri("/FILES/One")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if format == RouterOriginFormat::Native {
                405
            } else {
                404
            }
        );
    }
}

#[tokio::test]
async fn case_nested_local_version_hooks_run_once_with_original_uppercase_paths() {
    for (local, trailing, root) in [
        (false, false, false),
        (false, true, false),
        (true, false, false),
        (true, true, false),
        (false, false, true),
        (false, true, true),
        (true, false, true),
        (true, true, true),
    ] {
        let route_path = if root { "/" } else { "/hook" };
        let wire_path = nested_case_path(root, trailing);
        let mut contract = RouteContract::post("Hook", "Hook", route_path);
        contract.options.body_parser.json_strict = false;
        contract = contract.with_version(VersionedContract {
            version: "one".into(),
            request: RequestContract::default().with_body(BodyContract {
                schema: forklaunch_core::contract::SchemaRef::named("Number")
                    .with_json_schema(r#"{"type":"integer"}"#),
                ..BodyContract::json("Number")
            }),
            responses: vec![ResponseContract::new(201, None)],
        });
        let calls = Arc::new(AtomicU64::new(0));
        let count = calls.clone();
        let mut hooks = VersionBodyHooksRegistry::default();
        hooks
            .insert(
                &contract,
                "one",
                VersionBodyHooks::default()
                    .with_preprocess(move |value| {
                        count.fetch_add(1, Ordering::SeqCst);
                        value
                            .as_str()
                            .ok_or("expected string")?
                            .parse::<i64>()
                            .map(serde_json::Value::from)
                            .map_err(|error| error.to_string())
                    })
                    .with_transform(|value| Ok(json!({"result":value.as_i64().unwrap()+1}))),
            )
            .unwrap();
        let service = ServiceInfo::new("hooks", "1");
        let mut child = if local {
            let mut handlers = LocalRouter::new();
            handlers.register_with_body_hooks(
                contract.clone(),
                &hooks,
                move |request: FrameworkRequest| {
                    assert_eq!(request.raw_path.as_deref(), Some(wire_path));
                    assert_eq!(request.version.as_ref().unwrap().version, "one");
                    assert_eq!(request.body, FrameworkBody::Text("\"41\"".into()));
                    Ok(FrameworkResponse::json(
                        201,
                        request.version.unwrap().parsed_body.unwrap().to_string(),
                    ))
                },
            );
            ForkLaunchRouter::from_local_router(service.clone(), [contract], handlers)
        } else {
            ForkLaunchRouter::new(service.clone()).with_version_body_hooks(hooks.clone()).post(contract,
                |crate::VersionedBody(value): crate::VersionedBody<serde_json::Value>| async move {
                    (StatusCode::CREATED, axum::Json(value))
                })
        };
        child
            .config
            .optional_trailing_slash_paths
            .insert(route_path.into());
        child
            .config
            .typescript_case_insensitive_paths
            .insert(route_path.into());
        // Hook registration survives mounting a child router, and local selection
        // sees its original route path rather than the flattened transport prefix.
        let app = ForkLaunchRouter::new(service)
            .nest("/api", child)
            .into_router();
        let response = app
            .oneshot(
                Request::post(wire_path)
                    .header("content-type", "application/json")
                    .body(Body::from("\"41\""))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "local={local}, trailing={trailing}, root={root}"
        );
        let value: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
        assert_eq!(value, json!({"result":42}));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

fn nested_case_path(root: bool, trailing: bool) -> &'static str {
    match (root, trailing) {
        (false, false) => "/API/HOOK",
        (false, true) => "/API/HOOK/",
        (true, false) => "/API",
        (true, true) => "/API/",
    }
}
