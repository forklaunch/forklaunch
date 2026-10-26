//! Selected default error-string routes and deterministic service-failure injection.
use axum::response::{IntoResponse, Response};
use forklaunch_axum::{DefaultErrorResponses, ForkLaunchRouter, RouterConfig};
use forklaunch_core::ServiceInfo;
use forklaunch_core::{
    contract::{BodyContract, FieldContract, RouteContract},
    http::{Body, FrameworkRequest, FrameworkResponse, LocalRouter},
};
use std::time::Duration;
pub fn response(status: u16, kind: &str) -> FrameworkResponse {
    match kind {
        "text" => FrameworkResponse::text(status, "value"),
        "jsonstring" => FrameworkResponse::json(status, "\"value\""),
        "object" => FrameworkResponse::json(status, "{\"secret\":\"PRIVATE\"}"),
        "number" => FrameworkResponse::json(status, "42"),
        "binary" => FrameworkResponse {
            status_code: status,
            headers: std::collections::BTreeMap::from([(
                "content-type".into(),
                "application/octet-stream".into(),
            )]),
            body: Body::Binary(b"PRIVATE".to_vec()),
        },
        "emptytext" => FrameworkResponse::text(status, ""),
        _ => FrameworkResponse::new(status),
    }
}
pub fn wire(status: u16, kind: &str) -> Response {
    let source = response(status, kind);
    let body = match source.body {
        Body::Empty => axum::body::Body::empty(),
        Body::Text(value) => axum::body::Body::from(value),
        Body::Binary(value) => axum::body::Body::from(value),
    };
    let mut response = (axum::http::StatusCode::from_u16(status).unwrap(), body).into_response();
    response.headers_mut().clear();
    for (name, value) in source.headers {
        response.headers_mut().insert(
            axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
    }
    response
}
pub fn router() -> ForkLaunchRouter {
    let mut router = ForkLaunchRouter::new(ServiceInfo::new("defaults", "1"));
    let mut profiles = DefaultErrorResponses::default();
    for local in [false, true] {
        let mut contract = RouteContract::get(
            if local {
                "Local Defaults"
            } else {
                "Axum Defaults"
            },
            "Defaults",
            if local {
                "/local/:status/:kind"
            } else {
                "/axum/:status/:kind"
            },
        )
        .with_param(FieldContract::required("status", "String"))
        .with_param(FieldContract::required("kind", "String"))
        .with_response(200, Some(BodyContract::json("Output")));
        contract.options.openapi = false;
        profiles
            .insert(
                &contract,
                &[400, 401, 403, 404, 500],
                1024,
                Duration::from_secs(1),
            )
            .unwrap();
        if local {
            let mut handlers = LocalRouter::new();
            handlers.register(contract.clone(), |request: FrameworkRequest| {
                Ok(response(
                    request.params["status"].parse().unwrap(),
                    &request.params["kind"],
                ))
            });
            router = router.nest(
                "/",
                ForkLaunchRouter::from_local_router(
                    ServiceInfo::new("local-defaults", "1"),
                    [contract],
                    handlers,
                ),
            );
        } else {
            router=router.get(contract,|axum::extract::Path((status,kind)):axum::extract::Path<(u16,String)>|async move{wire(status,&kind)});
        }
    }
    router.with_config(RouterConfig {
        default_error_responses: profiles,
        ..RouterConfig::default()
    })
}
