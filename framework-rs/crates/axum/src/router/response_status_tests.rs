use super::*;
use forklaunch_core::contract::{
    BodyContract, RequestContract, ResponseContract, VersionedContract,
};

#[tokio::test]
async fn status_profile_preserves_defaults_declared_responses_and_validation_modes() {
    for local in [false, true] {
        for versioned in [false, true] {
            for format in [
                ResponseStatusErrorFormat::Json,
                ResponseStatusErrorFormat::TypeScriptText,
            ] {
                for mode in [
                    ValidationMode::Error,
                    ValidationMode::Warning,
                    ValidationMode::None,
                ] {
                    for status in [200, 201, 301, 400, 418, 499, 500, 501, 599] {
                        check(local, versioned, format, mode, status).await;
                    }
                }
            }
        }
    }
}

async fn check(
    local: bool,
    versioned: bool,
    format: ResponseStatusErrorFormat,
    mode: ValidationMode,
    status: u16,
) {
    let mut contract = RouteContract::post("Status", "Status", "/status")
        .with_response(200, None)
        .with_response(500, None);
    if versioned {
        contract = contract.with_version(VersionedContract {
            version: "v1".into(),
            request: RequestContract::default().with_body(BodyContract::json("Input")),
            responses: vec![
                ResponseContract::new(200, None),
                ResponseContract::new(500, None),
            ],
        });
    }
    contract.options.response_validation = mode;
    let service = ServiceInfo::new("status", "1");
    let router = if local {
        let mut handlers = LocalRouter::new();
        handlers.register(contract.clone(), move |_: FrameworkRequest| {
            Ok(FrameworkResponse::text(status, "PRIVATE_STATUS_BODY"))
        });
        ForkLaunchRouter::from_local_router(service, [contract], handlers)
    } else {
        ForkLaunchRouter::new(service).post(contract, move || async move {
            (StatusCode::from_u16(status).unwrap(), "PRIVATE_STATUS_BODY")
        })
    }
    .with_config(RouterConfig {
        response_status_error_format: format,
        ..RouterConfig::default()
    })
    .into_router();
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/status")
                .header("content-type", "application/json")
                .header("x-correlation-id", "status-test")
                .body(AxumBody::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    let media = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let rejected = mode == ValidationMode::Error && ![200, 500].contains(&status);
    assert_eq!(
        response.status().as_u16(),
        if rejected && (format == ResponseStatusErrorFormat::Json || status < 400) {
            500
        } else {
            status
        }
    );
    let body =
        String::from_utf8(to_bytes(response.into_body(), 8192).await.unwrap().to_vec()).unwrap();
    if rejected {
        assert!(!body.contains("PRIVATE_STATUS_BODY"));
        if format == ResponseStatusErrorFormat::TypeScriptText {
            assert_eq!(media, "text/plain; charset=utf-8");
            assert_eq!(
                body,
                format!(
                    "{}:\n\nCorrelation id: status-test",
                    if (400..500).contains(&status) {
                        "Request could not be processed"
                    } else {
                        "Internal server error"
                    }
                )
            );
        } else {
            assert_eq!(media, "application/json");
            assert!(serde_json::from_str::<serde_json::Value>(&body).is_ok());
        }
    } else {
        assert_eq!(body, "PRIVATE_STATUS_BODY");
    }
}
