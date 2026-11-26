//! Characterizes unmatched-version modes; diagnostic/auth ordering gaps remain explicit.
use axum::{Extension, Json};
use forklaunch_axum::{
    ForkLaunchRouter, ResponseStatusErrorFormat, ResponseValidators, RouterConfig,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    AccessLevel, BodyContract, FieldContract, RequestContract, ResponseContract, RouteContract,
    SchemaRef, ValidationMode, VersionedContract,
};
use forklaunch_core::http::{
    FrameworkRequest, FrameworkResponse, LocalRouter, SelectedVersion, VersionBodyHooksRegistry,
};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
fn contract(
    surface: &str,
    request: ValidationMode,
    response: ValidationMode,
    request_name: &str,
    response_name: &str,
    auth: bool,
) -> RouteContract {
    let mut route = RouteContract::get(
        format!("{surface}{request_name}{response_name}{auth}"),
        "Version modes",
        format!("/{request_name}/{response_name}/{auth}"),
    );
    route.options.request_validation = request;
    route.options.response_validation = response;
    if auth {
        route.access = AccessLevel::Protected;
    }
    for name in ["one", "two"] {
        let mut field = FieldContract::required("tag", "Tag");
        field.schema = SchemaRef::named("Tag").with_json_schema(json!({"const":name}).to_string());
        route = route.with_version(VersionedContract {
            version: name.into(),
            request: RequestContract {
                query: vec![field],
                ..RequestContract::default()
            },
            responses: vec![ResponseContract::new(
                200,
                Some(BodyContract::json("Output")),
            )],
        });
    }
    route
}
fn output(version: Option<&SelectedVersion>) -> Value {
    json!({"handler":true,"version":version.map(|v|&v.version)})
}
#[tokio::main]
async fn main() {
    let modes = [
        ("none", ValidationMode::None),
        ("warning", ValidationMode::Warning),
        ("error", ValidationMode::Error),
    ];
    let calls = Arc::new(AtomicUsize::new(0));
    let mut native = ForkLaunchRouter::anonymous();
    let mut local = LocalRouter::new();
    let mut contracts = vec![];
    let mut validators = ResponseValidators::default();
    let mut version_hooks = VersionBodyHooksRegistry::default();
    let pre_auth = std::env::var("FORKLAUNCH_VERSION_PREAUTH").as_deref() == Ok("1");
    for (req_name, req_mode) in modes {
        for (res_name, res_mode) in modes {
            for auth in [false, true] {
                let route = contract("native", req_mode, res_mode, req_name, res_name, auth);
                if pre_auth {
                    version_hooks
                        .enable_selection_before_authentication(&route)
                        .unwrap();
                }
                validators
                    .insert_versioned(&route, 4096, Duration::from_secs(1), |_, _, _| None)
                    .unwrap();
                validators.enable_warning(&route).unwrap();
                let counted = calls.clone();
                native = native.get(route, move |version: Option<Extension<SelectedVersion>>| {
                    counted.fetch_add(1, Ordering::SeqCst);
                    async move { Json(output(version.as_ref().map(|v| &v.0))) }
                });
                let route = contract("local", req_mode, res_mode, req_name, res_name, auth);
                if pre_auth {
                    version_hooks
                        .enable_selection_before_authentication(&route)
                        .unwrap();
                }
                validators
                    .insert_versioned(&route, 4096, Duration::from_secs(1), |_, _, _| None)
                    .unwrap();
                validators.enable_warning(&route).unwrap();
                contracts.push(route.clone());
                let counted = calls.clone();
                local.register(route, move |request: FrameworkRequest| {
                    counted.fetch_add(1, Ordering::SeqCst);
                    Ok(FrameworkResponse::json(
                        200,
                        output(request.version.as_ref()).to_string(),
                    ))
                });
            }
        }
    }
    let local =
        ForkLaunchRouter::from_local_router(ServiceInfo::new("local", "1"), contracts, local);
    let app = ForkLaunchRouter::anonymous()
        .nest("/native", native)
        .nest("/local", local)
        .with_config(RouterConfig {
            response_validators: validators,
            version_body_hooks: version_hooks,
            response_status_error_format: ResponseStatusErrorFormat::TypeScriptText,
            ..RouterConfig::default()
        })
        .into_router()
        .route(
            "/counts",
            axum::routing::get(move || {
                let value = calls.load(Ordering::SeqCst);
                async move { Json(json!({"handlerCalls":value})) }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
