use super::super::*;
use super::*;
use axum::body::{Body, to_bytes};

#[tokio::test]
async fn fallback_profile_preserves_native_defaults_and_head_headers() {
    for format in [
        RouterOriginFormat::Native,
        RouterOriginFormat::TypeScriptHtml,
    ] {
        let app = ForkLaunchRouter::new(ServiceInfo::new("origins", "1"))
            .get(
                RouteContract::get("Read", "Read", "/users/:id").with_response(200, None),
                || async { StatusCode::OK },
            )
            .with_config(RouterConfig {
                router_origin_format: format,
                parser_error_format: ParserErrorFormat::TypeScriptText,
                ..RouterConfig::default()
            })
            .into_router();
        for method in ["GET", "HEAD", "POST", "PATCH"] {
            for path in [
                "/missing?secret=hidden",
                "/missing/a&b'c",
                "/missing/%GG",
                "/users/one",
                "/users/%GG",
            ] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(path)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let fallback = path.starts_with("/missing") || matches!(method, "POST" | "PATCH");
                if format == RouterOriginFormat::TypeScriptHtml && fallback {
                    if path == "/users/%GG" {
                        assert_eq!(response.status(), 400);
                    } else {
                        assert_eq!(response.status(), 404);
                        assert_eq!(
                            response.headers()[header::CONTENT_TYPE],
                            "text/html; charset=utf-8"
                        );
                        assert_eq!(
                            response.headers()[header::CONTENT_SECURITY_POLICY],
                            "default-src 'none'"
                        );
                        assert_eq!(
                            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
                            "nosniff"
                        );
                        assert!(response.headers().contains_key(header::CONTENT_LENGTH));
                    }
                    assert!(!response.headers().contains_key(header::ALLOW));
                } else if format == RouterOriginFormat::Native
                    && matches!(method, "POST" | "PATCH")
                    && path.starts_with("/users/")
                {
                    assert_eq!(response.status(), 405);
                    assert!(response.headers().contains_key(header::ALLOW));
                }
                let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
                if method == "HEAD" {
                    assert!(bytes.is_empty());
                }
                assert!(!String::from_utf8_lossy(&bytes).contains("secret=hidden"));
            }
        }
    }
}

#[test]
fn encoding_preserves_escapes_and_repairs_invalid_percent() {
    for (input, expected) in [
        ("/a%20b", "/a%20b"),
        ("/%GG", "/%25GG"),
        ("/%A", "/%A"),
        ("/%G%20", "/%25G%20"),
        ("/%A%20", "/%25A%2520"),
        ("/a`b{c}", "/a%60b%7Bc%7D"),
        ("/☃", "/%E2%98%83"),
    ] {
        assert_eq!(encode_path(input), expected);
    }
}

#[tokio::test]
async fn application_responses_extended_methods_options_and_internal_fallbacks_survive() {
    let mut router = ForkLaunchRouter::anonymous();
    for method in HttpMethod::ALL {
        router = router.route(
            RouteContract::new(method.as_str(), "method", method, "/methods")
                .with_response(200, None),
            move || async move { (StatusCode::OK, [("x-method", method.as_str())]) },
        );
    }
    let app = router
        .get(
            RouteContract::get("Custom", "Custom", "/custom").with_response(404, None),
            || async { (StatusCode::NOT_FOUND, "application 404") },
        )
        .map_route("/custom", |route| {
            route.fallback(|| async { (StatusCode::IM_A_TEAPOT, "application fallback") })
        })
        .unwrap()
        .nest_internal(
            "/internal",
            Router::new()
                .fallback(|| async { (StatusCode::PAYMENT_REQUIRED, "internal fallback") }),
        )
        .with_config(RouterConfig {
            router_origin_format: RouterOriginFormat::TypeScriptHtml,
            ..RouterConfig::default()
        })
        .into_router();
    for method in HttpMethod::ALL {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method.as_str())
                    .uri("/methods")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{}", method.as_str());
        assert_eq!(response.headers()["x-method"], method.as_str());
    }
    for (method, path, status, body) in [
        ("GET", "/custom", 404, "application 404"),
        ("POST", "/custom", 418, "application fallback"),
        ("GET", "/internal/unknown", 402, "internal fallback"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(to_bytes(response.into_body(), 4096).await.unwrap(), body);
    }
    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/methods")
                .header("origin", "https://example.com")
                .header("access-control-request-method", "POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["access-control-allow-methods"], "*");
}

#[tokio::test]
async fn root_internal_router_retains_fallback_ownership() {
    let app = ForkLaunchRouter::anonymous()
        .get(
            RouteContract::get("Read", "Read", "/read").with_response(200, None),
            || async { StatusCode::OK },
        )
        .merge_internal(
            Router::new().fallback(|| async { (StatusCode::GONE, "root application fallback") }),
            ["/private"],
        )
        .with_config(RouterConfig {
            router_origin_format: RouterOriginFormat::TypeScriptHtml,
            ..RouterConfig::default()
        })
        .into_router();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/unknown")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 410);
    assert_eq!(
        to_bytes(response.into_body(), 4096).await.unwrap(),
        "root application fallback"
    );
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/read")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/html; charset=utf-8"
    );
}

fn selected_options_router(cors: Option<TypeScriptCors>) -> Router {
    let guarded =
        |route: RouteContract| route.with_auth(forklaunch_core::contract::AuthConfig::hmac());
    let child = ForkLaunchRouter::anonymous()
        .put(
            guarded(RouteContract::put("Write", "Write", "/items/:id")),
            || async { "put" },
        )
        .get(
            guarded(RouteContract::get("Read", "Read", "/items/:id")),
            || async { "get" },
        )
        .head(
            guarded(RouteContract::head("Head", "Head", "/items/:id")),
            || async { StatusCode::ACCEPTED },
        )
        .get(guarded(RouteContract::get("Root", "Root", "/")), || async {
            "root"
        })
        .options(
            guarded(RouteContract::options(
                "Explicit",
                "Explicit",
                "/explicit/:id",
            )),
            || async { StatusCode::ACCEPTED },
        )
        .get(
            guarded(RouteContract::get(
                "Unselected",
                "Unselected",
                "/unselected",
            )),
            || async { "unselected" },
        )
        .with_config(
            RouterConfig::default()
                .with_optional_trailing_slash_for_path("/items/:id")
                .with_optional_trailing_slash_for_path("/")
                .with_typescript_options_for_path("/items/:id")
                .with_typescript_options_for_path("/")
                .with_typescript_options_for_path("/explicit/:id"),
        );
    let config = RouterConfig {
        typescript_cors: cors,
        ..RouterConfig::default()
    }
    .with_response_authenticator(|request, _contract| async move {
        if request.headers.get("authorization").map(String::as_str) != Some("verified") {
            return Err(forklaunch_core::http::ErrorResponse::new(
                forklaunch_core::http::ErrorKind::Unauthorized,
                "locked",
            ));
        }
        Ok(AuthenticatedSession::new(
            forklaunch_core::auth::Principal::new("tester"),
            serde_json::json!({"sub":"tester"}),
        ))
    });
    let router = ForkLaunchRouter::anonymous()
        .with_config(config)
        .nest("/api", child);
    assert_eq!(router.registry().routes().len(), 6);
    router.into_router()
}

#[tokio::test]
async fn selected_options_preserve_explicit_auth_head_and_nested_aliases() {
    let app = selected_options_router(None);
    for path in ["/api/items/owned", "/api/items/owned/", "/api", "/api/"] {
        for auth in [
            "",
            "invalid",
            "HMAC keyId=missing ts=bad nonce=x signature=wrong",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("OPTIONS")
                        .uri(path)
                        .header("authorization", auth)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let allow = if path.contains("items") {
                "GET, HEAD, PUT"
            } else {
                "GET, HEAD"
            };
            assert_eq!(response.status(), 200, "{path}");
            assert_eq!(response.headers()[header::ALLOW], allow);
            assert_eq!(response.headers()[header::CONTENT_TYPE], "text/plain");
            assert_eq!(
                response.headers()[header::X_CONTENT_TYPE_OPTIONS],
                "nosniff"
            );
            assert_eq!(
                response.headers()[header::CONTENT_LENGTH],
                allow.len().to_string()
            );
            assert_eq!(
                to_bytes(response.into_body(), 1024).await.unwrap().as_ref(),
                allow.as_bytes()
            );
        }
    }
    for (method, path, auth, status) in [
        ("OPTIONS", "/api/explicit/owned", "invalid", 401),
        ("OPTIONS", "/api/explicit/owned", "verified", 202),
        ("HEAD", "/api/items/owned/", "invalid", 401),
        ("HEAD", "/api/items/owned/", "verified", 202),
        ("OPTIONS", "/api/unselected", "", 405),
        ("OPTIONS", "/api/items/owned//", "", 404),
        ("OPTIONS", "/missing", "", 404),
        ("POST", "/api/items/owned", "", 405),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", auth)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{method} {path}");
    }
}

#[tokio::test]
async fn selected_options_remain_inside_cors_preflight_policy() {
    for continue_preflight in [false, true] {
        let app = selected_options_router(Some(TypeScriptCors {
            preflight_continue: continue_preflight,
            ..TypeScriptCors::default()
        }));
        let response = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/api/items/owned/")
                    .header("origin", "https://example.test")
                    .header("access-control-request-method", "PUT")
                    .header("authorization", "invalid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if continue_preflight { 200 } else { 204 }
        );
        assert_eq!(response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
        assert_eq!(
            response.headers().contains_key(header::ALLOW),
            continue_preflight
        );
    }
}
