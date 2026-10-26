//! Actual-source undocumented status comparison, with explicit 500 preservation.
use forklaunch_axum::ForkLaunchRouter;
use forklaunch_core::ServiceInfo;
use forklaunch_core::{
    contract::{BodyContract, BodyKind, FieldContract, RouteContract, SchemaRef},
    http::{FrameworkRequest, FrameworkResponse, LocalRouter},
};

pub fn router() -> ForkLaunchRouter {
    let mut router = ForkLaunchRouter::new(ServiceInfo::new("status", "1"));
    for local in [false, true] {
        let mut contract = RouteContract::get(
            if local { "Local Status" } else { "Axum Status" },
            "Status",
            if local { "/local/:mode" } else { "/axum/:mode" },
        )
        .with_param(FieldContract::required("mode", "String"));
        for status in [200, 500] {
            contract = contract.with_response(
                status,
                Some(BodyContract {
                    kind: BodyKind::Text,
                    schema: SchemaRef::named("String"),
                    content_type: Some("text/plain".into()),
                }),
            );
        }
        contract.options.openapi = false;
        if local {
            let mut handlers = LocalRouter::new();
            handlers.register(contract.clone(), |request: FrameworkRequest| {
                let (status, body) = result(&request.params["mode"]);
                Ok(FrameworkResponse::text(status, body)
                    .with_header("content-type", "text/plain; charset=utf-8"))
            });
            router = router.nest(
                "/",
                ForkLaunchRouter::from_local_router(
                    ServiceInfo::new("local-status", "1"),
                    [contract],
                    handlers,
                ),
            );
        } else {
            router = router.get(
                contract,
                |axum::extract::Path(mode): axum::extract::Path<String>| async move {
                    let (status, body) = result(&mode);
                    (axum::http::StatusCode::from_u16(status).unwrap(), body)
                },
            );
        }
    }
    router
}
fn result(mode: &str) -> (u16, &'static str) {
    match mode {
        "invalid" => (201, "PRIVATE_STATUS_PAYLOAD"),
        "client" => (418, "PRIVATE_STATUS_PAYLOAD"),
        "server" => (503, "PRIVATE_STATUS_PAYLOAD"),
        "declared" => (500, "accepted"),
        _ => (200, "accepted"),
    }
}
