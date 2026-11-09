//! Live counterpart of framework-rs/parity/fixtures/canonical-iam/service.ts: the IAM
//! module's user directory contracts served from the in-memory IAM service,
//! plus framework error/parser/validation probes (`rust_canonical_http_parity`).
#[path = "support/canonical_defaults.rs"]
mod canonical_defaults;
#[path = "support/canonical_query.rs"]
mod canonical_query;
#[path = "support/canonical_response.rs"]
mod canonical_response;
#[path = "support/canonical_status.rs"]
mod canonical_status;
use axum::{extract::Path, http::StatusCode, response::IntoResponse};
use forklaunch_axum::{
    AuthErrorFormat, ForkLaunchRouter, HandlerErrorFormat, ParserErrorFormat,
    ResponseStatusErrorFormat, RouterConfig, session::AuthenticatedSession,
};
use forklaunch_core::auth::{
    JwtClaims, issue_jwt_hs256, typescript::jwt::verify_jwt_resource, verify_jwt_hs256,
};
use forklaunch_core::domain::PageRequest;
use forklaunch_core::http::{ParseError, typescript_request_validation_error_response};
use forklaunch_core::{ServiceInfo, common::FrameworkError};
use forklaunch_core::{
    contract::{
        BodyContract, FieldContract, RequestContract, ResponseContract, RouteContract,
        VersionedContract,
    },
    http::{
        BodyParserHooks, FrameworkRequest, FrameworkResponse, LocalRouter, VersionBodyHooksRegistry,
    },
};
use forklaunch_iam_base::InMemoryIamService;
use forklaunch_iam_interface::IamService;
use forklaunch_iam_interface::User;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// The IAM module's user directory read routes over the in-memory service.
fn iam_router(service: &Arc<InMemoryIamService>) -> ForkLaunchRouter {
    let contract = |name: &str| {
        forklaunch_iam_interface::contracts::registry()
            .routes()
            .iter()
            .find(|route| route.operation_name == name)
            .cloned()
            .unwrap_or_else(|| panic!("iam contract `{name}` should exist"))
    };
    let get_user_service = Arc::clone(service);
    let list_users_service = Arc::clone(service);
    ForkLaunchRouter::new(ServiceInfo::new("iam", "0.1.0"))
        .get(contract("getUser"), move |Path(id): Path<String>| {
            let service = Arc::clone(&get_user_service);
            async move {
                match service.get_user(&id) {
                    Ok(Some(user)) => (StatusCode::OK, axum::Json(user)).into_response(),
                    Ok(None) => StatusCode::NOT_FOUND.into_response(),
                    Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                }
            }
        })
        .get(contract("listUsers"), move || {
            let service = Arc::clone(&list_users_service);
            async move {
                match service.list_users(PageRequest::first_page(100)) {
                    Ok(page) => (StatusCode::OK, axum::Json(page)).into_response(),
                    Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                }
            }
        })
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = Arc::new(InMemoryIamService::new(vec![
        User::new("user_1", "ada@example.com"),
        User::new("user_2", "grace@example.com"),
    ]));
    let mut config = RouterConfig {
        auth_error_format: AuthErrorFormat::TypeScriptText,
        handler_error_format: HandlerErrorFormat::TypeScriptText,
        parser_error_format: ParserErrorFormat::TypeScriptText,
        response_status_error_format: ResponseStatusErrorFormat::TypeScriptText,
        body_limit: 128,
        ..RouterConfig::default()
    }
    .with_response_authenticator(|request, _| async move {
        let claims = verify_jwt_resource(
            request.headers.get("authorization").map(String::as_str),
            None,
            |token| async move {
                let principal = verify_jwt_hs256(token, b"test-only-signing-key")
                    .map_err(|error| error.to_error_response())?;
                Ok(serde_json::to_value(principal).expect("serialize verified principal"))
            },
        )
        .await?;
        let principal = serde_json::from_value(claims).expect("verified principal shape");
        Ok(AuthenticatedSession::new(
            principal,
            serde_json::json!({"sub":"tester"}),
        ))
    });
    // Optional fixture output demonstrates framework-native issuance. This is
    // a test-only key/identity, never an application credential endpoint.
    if let Some(output) = std::env::args().nth(1) {
        let claims = JwtClaims {
            sub: "tester".into(),
            exp: 4_102_444_800,
            roles: vec!["reader".into()],
            permissions: vec!["users:read".into()],
            scopes: vec!["profile".into()],
            features: vec!["reports".into()],
            active_subscription: true,
        };
        let token = issue_jwt_hs256(&claims, b"test-only-signing-key")?;
        std::fs::write(
            output,
            serde_json::to_vec(&serde_json::json!({"token":token,"claims":claims}))?,
        )?;
    }
    let mut failure = RouteContract::get("Failure", "Failure", "/throw");
    failure.options.openapi = false;
    let mut handlers = LocalRouter::new();
    handlers.register(failure.clone(), |_: FrameworkRequest| {
        Err(FrameworkError::new(
            "fixture_internal",
            "secret fixture detail",
        ))
    });
    let mut validation = RouteContract::get("Validate", "Validate", "/validate")
        .with_query(FieldContract::required("display_name", "String"))
        .with_response(200, None)
        .with_response(400, None);
    validation.options.openapi = false;
    handlers.register(validation.clone(), |request: FrameworkRequest| {
        let display_name = request
            .query
            .get("display_name")
            .expect("required query validated");
        if display_name.trim().is_empty() {
            // A caller-owned domain rule supplies public ParseErrors. This does
            // not claim the native schema validator emits Zod diagnostics.
            Ok(typescript_request_validation_error_response(
                Some(&[ParseError {
                    path: vec!["query".to_owned(), "display_name".to_owned()],
                    message: "A display name is required".to_owned(),
                }]),
                Some(&request.context.correlation_id),
            ))
        } else {
            Ok(FrameworkResponse::text(200, "accepted"))
        }
    });
    let errors = ForkLaunchRouter::from_local_router(
        ServiceInfo::new("errors", "1"),
        [failure, validation],
        handlers,
    );
    let calls = Arc::new(AtomicU64::new(0));
    let mut parser_hooks = VersionBodyHooksRegistry::default();
    let mut parser_router = ForkLaunchRouter::new(ServiceInfo::new("parser", "1"));
    for local in [false, true] {
        let path = if local { "/local" } else { "/axum" };
        let mut contract = RouteContract::post(
            if local { "Local Parser" } else { "Axum Parser" },
            "Parser",
            path,
        )
        .with_version(VersionedContract {
            version: "v1".into(),
            request: RequestContract::default().with_body(BodyContract::json("Input")),
            responses: vec![ResponseContract::new(200, None)],
        });
        contract.options.openapi = false;
        contract.options.body_parser.json_wire.limit_bytes = 64;
        let hooks = BodyParserHooks::default().with_verify(|request, _, _| {
            if request.headers.get("x-deny").map(String::as_str) == Some("yes") {
                Err("PRIVATE_VERIFIER_DETAIL".into())
            } else {
                Ok(())
            }
        });
        parser_hooks.insert_parser(&contract, hooks).unwrap();
        let count = Arc::clone(&calls);
        if local {
            let mut local_handlers = LocalRouter::new();
            local_handlers.register_with_body_hooks(
                contract.clone(),
                &parser_hooks,
                move |_: FrameworkRequest| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(FrameworkResponse::json(200, r#"{"accepted":"yes"}"#))
                },
            );
            parser_router = parser_router.nest(
                "/",
                ForkLaunchRouter::from_local_router(
                    ServiceInfo::new("local", "1"),
                    [contract],
                    local_handlers,
                ),
            );
        } else {
            parser_router = parser_router.post(contract, move || {
                let count = Arc::clone(&count);
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    axum::Json(serde_json::json!({"accepted":"yes"}))
                }
            });
        }
    }
    let mut count_contract =
        RouteContract::get("Calls", "Calls", "/calls").with_response(200, None);
    count_contract.options.openapi = false;
    parser_router = parser_router.get(count_contract, move || {
        let calls = Arc::clone(&calls);
        async move { axum::Json(calls.load(Ordering::SeqCst)) }
    });
    // Hook identities use operation names and remain stable through nesting.
    parser_router = parser_router.with_version_body_hooks(parser_hooks);
    let query_calls = Arc::new(AtomicU64::new(0));
    let iam = iam_router(&service);
    let list = iam
        .registry()
        .routes()
        .iter()
        .find(|route| route.path == "/users")
        .unwrap();
    config.request_validators.insert_with_rejection(
        list,
        ResponseContract::new(
            400,
            Some(BodyContract {
                kind: forklaunch_core::contract::BodyKind::Text,
                schema: forklaunch_core::contract::SchemaRef::named("String"),
                content_type: Some("text/plain".into()),
            }),
        ),
        canonical_query::reject,
    )?;
    for route in iam.registry().routes() {
        let statuses: &[u16] = if route.path == "/users" {
            &[404, 500]
        } else {
            &[500]
        };
        config.default_error_responses.insert(
            route,
            statuses,
            1024,
            std::time::Duration::from_secs(1),
        )?;
    }
    let mut iam = iam;
    for path in ["/users", "/users/:id"] {
        iam = iam.map_route(path, |route| {
            route.layer(axum::middleware::from_fn(
                |request: axum::extract::Request, next: axum::middleware::Next| async move {
                    if let Some(status) = request
                        .headers()
                        .get("x-default-status")
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u16>().ok())
                    {
                        let kind = request
                            .headers()
                            .get("x-default-kind")
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or("text");
                        return canonical_defaults::wire(status, kind);
                    }
                    next.run(request).await
                },
            ))
        })?;
    }
    let count = Arc::clone(&query_calls);
    let iam = iam.map_route("/users", move |route| {
        let count = Arc::clone(&count);
        route.layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let count = Arc::clone(&count);
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    next.run(request).await
                }
            },
        ))
    })?;
    let mut query_count = RouteContract::get("Query Calls", "Calls", "/query/calls");
    query_count.options.openapi = false;
    let assembled = iam
        .get(query_count, move || {
            let calls = Arc::clone(&query_calls);
            async move { axum::Json(calls.load(Ordering::SeqCst)) }
        })
        .with_config(config)
        .nest("/errors", errors)
        .nest("/parser", parser_router)
        .nest("/response", canonical_response::router())
        .nest("/status", canonical_status::router())
        .nest("/defaults", canonical_defaults::router());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!("READY http://{}", listener.local_addr()?);
    axum::serve(listener, assembled.into_router())
        .with_graceful_shutdown(async {
            #[cfg(unix)]
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler")
                .recv()
                .await;
            #[cfg(not(unix))]
            tokio::signal::ctrl_c()
                .await
                .expect("install shutdown handler");
        })
        .await?;
    Ok(())
}
