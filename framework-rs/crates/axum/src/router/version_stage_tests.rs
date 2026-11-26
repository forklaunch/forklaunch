use super::*;
use axum::{body::Body, http::Request};
use forklaunch_core::contract::{
    BodyContract, FieldContract, RequestContract, ResponseContract, SchemaRef, VersionedContract,
};
use forklaunch_core::http::{
    BodyParserHooks, ContractValidationLocation, VersionBodyHooksRegistry, VersionFieldHooks,
};
use serde_json::json;
use std::sync::atomic::AtomicUsize;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn pre_auth_version_selection_preserves_security_and_runs_actual_hooks_once() {
    for wire in [false, true] {
        for local in [false, true] {
            for mode in [
                ValidationMode::None,
                ValidationMode::Warning,
                ValidationMode::Error,
            ] {
                for (tag, body, credentials) in [
                    ("one", "{}", false),
                    ("bad", "{}", false),
                    ("one", "{}", true),
                    ("bad", "{}", true),
                    ("one", "{", false),
                ] {
                    let mut field = FieldContract::required("tag", "Tag");
                    field.schema = SchemaRef::named("Tag").with_json_schema(r#"{"const":"one"}"#);
                    let mut route = RouteContract::post("Stage", "Stage", "/stage")
                        .with_access(AccessLevel::Protected)
                        .with_version(VersionedContract {
                            version: "one".into(),
                            request: RequestContract {
                                query: vec![field],
                                body: Some(BodyContract::json("Input")),
                                ..RequestContract::default()
                            },
                            responses: vec![ResponseContract::new(200, None)],
                        });
                    route.options.request_validation = mode;
                    route.options.response_validation = ValidationMode::None;
                    let parser_calls = Arc::new(AtomicUsize::new(0));
                    let field_calls = Arc::new(AtomicUsize::new(0));
                    let auth_calls = Arc::new(AtomicUsize::new(0));
                    let handler_calls = Arc::new(AtomicUsize::new(0));
                    let mut hooks = VersionBodyHooksRegistry::default();
                    hooks
                        .enable_selection_before_authentication(&route)
                        .unwrap();
                    if wire {
                        hooks.enable_typescript_json_wire(&route).unwrap();
                    }
                    let counted = parser_calls.clone();
                    hooks
                        .insert_parser(
                            &route,
                            BodyParserHooks::default().with_verify(move |_, _, _| {
                                counted.fetch_add(1, Ordering::SeqCst);
                                Ok(())
                            }),
                        )
                        .unwrap();
                    let counted = field_calls.clone();
                    hooks
                        .insert_field(
                            &route,
                            "one",
                            ContractValidationLocation::Query,
                            "tag",
                            VersionFieldHooks::default()
                                .with_preprocess(move |input| {
                                    counted.fetch_add(1, Ordering::SeqCst);
                                    Ok(input.clone())
                                })
                                .with_transform(|_| Ok(json!("ONE"))),
                        )
                        .unwrap();
                    let counted = handler_calls.clone();
                    let child = if local {
                        let mut handlers = LocalRouter::new();
                        handlers.register_with_body_hooks(
                            route.clone(),
                            &hooks,
                            move |request: FrameworkRequest| {
                                counted.fetch_add(1, Ordering::SeqCst);
                                assert!(request.session.is_some());
                                assert_eq!(
                                    request
                                        .version_parsed_body
                                        .as_ref()
                                        .and_then(forklaunch_core::http::VersionParsedBody::value),
                                    wire.then(|| json!({})).as_ref()
                                );
                                Ok(FrameworkResponse::text(200, "ok"))
                            },
                        );
                        // Config hooks differ intentionally: local selection must use its actual plan.
                        hooks
                            .insert_field(
                                &route,
                                "one",
                                ContractValidationLocation::Query,
                                "tag",
                                VersionFieldHooks::default().with_preprocess(|_| {
                                    panic!("foreign local hooks must not run")
                                }),
                            )
                            .unwrap();
                        ForkLaunchRouter::from_local_router(
                            ServiceInfo::new("stage", "1"),
                            [route],
                            handlers,
                        )
                    } else {
                        ForkLaunchRouter::anonymous().post(
                            route,
                            move |parsed: Option<
                                axum::Extension<forklaunch_core::http::VersionParsedBody>,
                            >| {
                                assert_eq!(
                                    parsed.as_ref().and_then(|parsed| parsed.0.value()),
                                    wire.then(|| json!({})).as_ref()
                                );
                                counted.fetch_add(1, Ordering::SeqCst);
                                async { "ok" }
                            },
                        )
                    };
                    let counted = auth_calls.clone();
                    let config = RouterConfig {
                        version_body_hooks: hooks,
                        ..RouterConfig::default()
                    }
                    .with_authenticator(move |request, _| {
                        counted.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(
                            request
                                .version_parsed_body
                                .as_ref()
                                .and_then(forklaunch_core::http::VersionParsedBody::value),
                            wire.then(|| json!({})).as_ref()
                        );
                        if request.query["tag"] == "one" {
                            assert_eq!(
                                request.version.as_ref().unwrap().parsed_fields.query["tag"],
                                json!("ONE")
                            );
                        } else {
                            assert!(request.version.is_none());
                        }
                        assert_eq!(
                            request.headers.get("authorization").map(String::as_str),
                            credentials.then_some("Bearer raw")
                        );
                        async move {
                            if credentials {
                                Ok(AuthenticatedSession::new(
                                    forklaunch_core::auth::Principal::new("actor"),
                                    json!({"sub":"actor"}),
                                ))
                            } else {
                                Err(AuthError::MissingCredentials)
                            }
                        }
                    });
                    let app = ForkLaunchRouter::anonymous()
                        .nest("/nested", child)
                        .with_config(config)
                        .into_router();
                    let mut request = Request::builder()
                        .method("POST")
                        .uri(format!("/nested/stage?tag={tag}"))
                        .header("content-type", "application/json")
                        .header("content-length", body.len());
                    if credentials {
                        request = request.header("authorization", "Bearer raw");
                    }
                    let response = app
                        .oneshot(request.body(Body::from(body)).unwrap())
                        .await
                        .unwrap();
                    let rejected = body == "{" || (tag == "bad" && mode == ValidationMode::Error);
                    assert_eq!(
                        response.status(),
                        if rejected {
                            400
                        } else if credentials {
                            200
                        } else {
                            401
                        },
                        "local={local} mode={mode:?} tag={tag} body={body}"
                    );
                    assert_eq!(parser_calls.load(Ordering::SeqCst), 1);
                    assert_eq!(field_calls.load(Ordering::SeqCst), usize::from(body != "{"));
                    assert_eq!(auth_calls.load(Ordering::SeqCst), usize::from(!rejected));
                    assert_eq!(
                        handler_calls.load(Ordering::SeqCst),
                        usize::from(!rejected && credentials)
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn default_version_selection_stays_after_authentication() {
    let route = RouteContract::post("Default", "Default", "/default")
        .with_access(AccessLevel::Protected)
        .with_version(VersionedContract {
            version: "one".into(),
            request: RequestContract::default().with_body(BodyContract::json("Input")),
            responses: vec![ResponseContract::new(200, None)],
        });
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks
        .insert_parser(
            &route,
            BodyParserHooks::default().with_verify(move |_, _, _| {
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
    let app = ForkLaunchRouter::anonymous()
        .with_version_body_hooks(hooks)
        .post(route, || async { "must not run" })
        .into_router();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/default")
                .header("content-type", "application/json")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn selected_version_never_bypasses_rbac() {
    let route = RouteContract::get("Rbac", "Rbac", "/rbac")
        .with_access(AccessLevel::Protected)
        .with_rbac(forklaunch_core::contract::RbacConfig {
            allowed_roles: vec!["admin".into()],
            ..forklaunch_core::contract::RbacConfig::default()
        })
        .with_version(VersionedContract {
            version: "one".into(),
            request: RequestContract::default(),
            responses: vec![ResponseContract::new(200, None)],
        });
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks
        .enable_selection_before_authentication(&route)
        .unwrap();
    let config = RouterConfig {
        version_body_hooks: hooks,
        ..RouterConfig::default()
    }
    .with_authenticator(|request, _| async move {
        assert_eq!(request.version.unwrap().version, "one");
        Ok(AuthenticatedSession::new(
            forklaunch_core::auth::Principal::new("no-admin-role"),
            json!({}),
        ))
    });
    let app = ForkLaunchRouter::anonymous()
        .get(route, || async { "RBAC must reject before this handler" })
        .with_config(config)
        .into_router();
    let response = app
        .oneshot(Request::builder().uri("/rbac").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[test]
fn pre_auth_registry_rejects_missing_stale_duplicate_and_unversioned_contracts() {
    let mut hooks = VersionBodyHooksRegistry::default();
    let plain = RouteContract::get("plain", "plain", "/plain");
    assert!(
        hooks
            .enable_selection_before_authentication(&plain)
            .is_err()
    );
    let route = plain.with_version(VersionedContract {
        version: "one".into(),
        request: RequestContract::default(),
        responses: vec![],
    });
    hooks
        .enable_selection_before_authentication(&route)
        .unwrap();
    assert!(
        hooks
            .enable_selection_before_authentication(&route)
            .is_err()
    );
    assert!(hooks.validate_routes(&[]).is_err());
    let mut changed = route.clone();
    changed.access = AccessLevel::Protected;
    assert!(hooks.compile(&changed).is_err());
    changed = route;
    changed.versions.clear();
    assert!(hooks.compile(&changed).is_err());
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn text_wire_pre_auth_preserves_denial_and_once_only_callbacks_in_all_modes() {
    for local in [false, true] {
        for mode in [
            ValidationMode::None,
            ValidationMode::Warning,
            ValidationMode::Error,
        ] {
            for (media, body, bad_encoding) in [
                ("text/plain", "ok", false),
                ("text/plain", "bad", false),
                ("application/json", "ok", false),
                ("text/plain", "ok", true),
            ] {
                for credentials in [false, true] {
                    let mut declared = BodyContract::json("Text");
                    declared.kind = BodyKind::Text;
                    declared.content_type = Some("text/plain".into());
                    declared.schema = declared
                        .schema
                        .with_json_schema(r#"{"type":"string","const":"ok"}"#);
                    let mut route = RouteContract::post("TextStage", "TextStage", "/text")
                        .with_access(AccessLevel::Protected)
                        .with_version(VersionedContract {
                            version: "one".into(),
                            request: RequestContract::default().with_body(declared),
                            responses: vec![ResponseContract::new(200, None)],
                        });
                    route.options.request_validation = mode;
                    route.options.response_validation = ValidationMode::None;
                    let parser_calls = Arc::new(AtomicUsize::new(0));
                    let auth_calls = Arc::new(AtomicUsize::new(0));
                    let handler_calls = Arc::new(AtomicUsize::new(0));
                    let mut hooks = VersionBodyHooksRegistry::default();
                    hooks.enable_typescript_text_wire(&route).unwrap();
                    hooks
                        .enable_selection_before_authentication(&route)
                        .unwrap();
                    let counted = parser_calls.clone();
                    hooks
                        .insert_parser(
                            &route,
                            BodyParserHooks::default().with_verify(move |_, _, _| {
                                counted.fetch_add(1, Ordering::SeqCst);
                                Ok(())
                            }),
                        )
                        .unwrap();
                    let counted = handler_calls.clone();
                    let child = if local {
                        let mut handlers = LocalRouter::new();
                        handlers.register_with_body_hooks(
                            route.clone(),
                            &hooks,
                            move |request: FrameworkRequest| {
                                counted.fetch_add(1, Ordering::SeqCst);
                                assert!(request.session.is_some());
                                assert_eq!(
                                    request
                                        .version_parsed_body
                                        .as_ref()
                                        .and_then(forklaunch_core::http::VersionParsedBody::value),
                                    (media == "text/plain").then(|| json!(body)).as_ref()
                                );
                                Ok(FrameworkResponse::text(200, "ok"))
                            },
                        );
                        ForkLaunchRouter::from_local_router(
                            ServiceInfo::new("text", "1"),
                            [route],
                            handlers,
                        )
                    } else {
                        ForkLaunchRouter::anonymous().post(
                            route,
                            move |parsed: Option<
                                axum::Extension<forklaunch_core::http::VersionParsedBody>,
                            >| {
                                counted.fetch_add(1, Ordering::SeqCst);
                                assert_eq!(
                                    parsed.as_ref().and_then(|parsed| parsed.0.value()),
                                    (media == "text/plain").then(|| json!(body)).as_ref()
                                );
                                async { "ok" }
                            },
                        )
                    };
                    let counted = auth_calls.clone();
                    let config = RouterConfig {
                        version_body_hooks: hooks,
                        ..RouterConfig::default()
                    }
                    .with_authenticator(move |request, _| {
                        counted.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(
                            request
                                .version_parsed_body
                                .as_ref()
                                .and_then(forklaunch_core::http::VersionParsedBody::value),
                            (media == "text/plain").then(|| json!(body)).as_ref()
                        );
                        async move {
                            if credentials {
                                Ok(AuthenticatedSession::new(
                                    forklaunch_core::auth::Principal::new("actor"),
                                    json!({"sub":"actor"}),
                                ))
                            } else {
                                Err(AuthError::MissingCredentials)
                            }
                        }
                    });
                    let app = child.with_config(config).into_router();
                    let mut request = Request::builder()
                        .method("POST")
                        .uri("/text")
                        .header("content-type", media)
                        .header("content-length", body.len());
                    if credentials {
                        request = request.header("authorization", "Bearer raw");
                    }
                    if bad_encoding {
                        request = request.header("content-encoding", "bogus");
                    }
                    let response = app
                        .oneshot(request.body(Body::from(body)).unwrap())
                        .await
                        .unwrap();
                    let rejected = bad_encoding
                        || (mode == ValidationMode::Error
                            && (media != "text/plain" || body != "ok"));
                    assert_eq!(
                        response.status(),
                        if bad_encoding {
                            415
                        } else if rejected {
                            400
                        } else if credentials {
                            200
                        } else {
                            401
                        }
                    );
                    assert_eq!(
                        parser_calls.load(Ordering::SeqCst),
                        usize::from(media == "text/plain" && !bad_encoding)
                    );
                    assert_eq!(auth_calls.load(Ordering::SeqCst), usize::from(!rejected));
                    assert_eq!(
                        handler_calls.load(Ordering::SeqCst),
                        usize::from(!rejected && credentials)
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn optional_absence_keeps_required_fields_and_authentication() {
    for local in [false, true] {
        for (query, credentials, status) in
            [(false, false, 400), (true, false, 401), (true, true, 200)]
        {
            let body = BodyContract::json("Input");
            let mut request = RequestContract::default().with_optional_body(body);
            request.query.push(FieldContract::required("tag", "String"));
            let route = RouteContract::post("OptionalSecurity", "Optional security", "/optional")
                .with_access(AccessLevel::Protected)
                .with_version(VersionedContract {
                    version: "one".into(),
                    request,
                    responses: vec![ResponseContract::new(200, None)],
                });
            let mut hooks = VersionBodyHooksRegistry::default();
            hooks.enable_typescript_json_wire(&route).unwrap();
            hooks
                .enable_selection_before_authentication(&route)
                .unwrap();
            let calls = Arc::new(AtomicUsize::new(0));
            let counted = calls.clone();
            let child = if local {
                let mut router = LocalRouter::new();
                router.register_with_body_hooks(
                    route.clone(),
                    &hooks,
                    move |request: FrameworkRequest| {
                        assert!(request.version.as_ref().unwrap().parsed_body.is_none());
                        assert!(request.session.is_some());
                        counted.fetch_add(1, Ordering::SeqCst);
                        Ok(FrameworkResponse::text(200, "ok"))
                    },
                );
                ForkLaunchRouter::from_local_router(
                    ServiceInfo::new("optional", "1"),
                    [route],
                    router,
                )
            } else {
                ForkLaunchRouter::anonymous().route(route, move || {
                    counted.fetch_add(1, Ordering::SeqCst);
                    async { "ok" }
                })
            };
            let auth_calls = Arc::new(AtomicUsize::new(0));
            let counted = auth_calls.clone();
            let config = RouterConfig {
                version_body_hooks: hooks,
                ..RouterConfig::default()
            }
            .with_authenticator(move |request, _| {
                assert!(request.version.as_ref().unwrap().parsed_body.is_none());
                counted.fetch_add(1, Ordering::SeqCst);
                async move {
                    if credentials {
                        Ok(AuthenticatedSession::new(
                            forklaunch_core::auth::Principal::new("actor"),
                            json!({"sub":"actor"}),
                        ))
                    } else {
                        Err(AuthError::MissingCredentials)
                    }
                }
            });
            let response = child
                .with_config(config)
                .into_router()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(if query {
                            "/optional?tag=ok"
                        } else {
                            "/optional"
                        })
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            assert_eq!(auth_calls.load(Ordering::SeqCst), usize::from(query));
            assert_eq!(
                calls.load(Ordering::SeqCst),
                usize::from(query && credentials)
            );
        }
    }
}
