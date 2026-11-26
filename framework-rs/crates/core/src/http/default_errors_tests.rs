use super::*;
use crate::http::{
    FrameworkRequest, LocalRouter, ParserErrorFormat, RequestContext, ResponseStatusErrorFormat,
};
use forklaunch_core::contract::{HttpMethod, RequestContract, VersionedContract};
fn route() -> RouteContract {
    RouteContract::get("Default", "Default", "/default").with_response(200, None)
}
fn profiles(route: &RouteContract) -> DefaultErrorResponses {
    let mut profiles = DefaultErrorResponses::default();
    profiles
        .insert(route, &[404, 500], 128, Duration::from_millis(50))
        .unwrap();
    profiles
}
#[test]
fn registrations_are_explicit_pinned_and_projected() {
    for statuses in [vec![], vec![200], vec![418], vec![404, 404]] {
        assert!(
            DefaultErrorResponses::default()
                .insert(&route(), &statuses, 10, Duration::from_secs(1))
                .is_err()
        );
    }
    for (bytes, time) in [(0, Duration::from_secs(1)), (1, Duration::ZERO)] {
        assert!(
            DefaultErrorResponses::default()
                .insert(&route(), &[404], bytes, time)
                .is_err()
        );
    }
    let versioned = route().with_version(VersionedContract {
        version: "v1".into(),
        request: RequestContract::default(),
        responses: vec![],
    });
    assert!(
        DefaultErrorResponses::default()
            .insert(&versioned, &[404], 10, Duration::from_secs(1))
            .is_err()
    );
    let original = route();
    let profiles = profiles(&original);
    assert!(profiles.validate_routes(&[]).is_err());
    assert!(
        profiles
            .validate_routes(&[original.clone(), original.clone()])
            .is_err()
    );
    assert!(
        profiles
            .validate_route(&original.clone().with_response(404, None))
            .is_err()
    );
    let mut registry = ContractRegistry::new();
    registry.register(original);
    let projected = profiles.project(&registry).unwrap().unwrap();
    assert_eq!(registry.routes()[0].responses.len(), 1);
    assert_eq!(projected.routes()[0].responses.len(), 3);
    assert!(projected.into_artifact().to_json().contains("text/plain"));
}
#[test]
fn local_selected_errors_accept_strings_and_empty_but_never_arbitrary_json() {
    for response in [
        FrameworkResponse::text(404, "value"),
        FrameworkResponse::json(500, "\"value\""),
        FrameworkResponse::text(500, ""),
        FrameworkResponse::new(500),
    ] {
        let contract = route();
        let profiles = profiles(&contract);
        let original = response.clone();
        let mut local = LocalRouter::new();
        local.register(contract, move |_: FrameworkRequest| Ok(response.clone()));
        let result = local
            .execute_with_response_profiles(
                FrameworkRequest::new(HttpMethod::Get, "/default", RequestContext::new("test")),
                ParserErrorFormat::Json,
                ResponseStatusErrorFormat::Json,
                &profiles,
            )
            .unwrap();
        assert_eq!(result.status_code, original.status_code);
        if original.body.is_empty() {
            assert!(result.body.is_empty());
        } else {
            assert_eq!(result.headers["content-type"], "text/plain; charset=utf-8");
            assert_eq!(
                result.body.as_text(),
                Some(if original.body.as_text() == Some("") {
                    ""
                } else {
                    "value"
                })
            );
        }
    }
    for (input, received) in [
        ("{}", "object"),
        ("[]", "array"),
        ("null", "null"),
        ("true", "boolean"),
        ("42", "number"),
    ] {
        let result = profiles(&route())
            .normalize(&route(), FrameworkResponse::json(404, input), "test")
            .unwrap_err();
        assert_eq!(result.status_code, 500);
        assert!(
            result
                .body
                .as_text()
                .unwrap()
                .contains(&format!("received {received}"))
        );
    }
    let original = FrameworkResponse::json(200, "{}");
    assert_eq!(
        profiles(&route())
            .normalize(&route(), original.clone(), "test")
            .unwrap(),
        original
    );
    for mode in [ValidationMode::Warning, ValidationMode::None] {
        let mut contract = route();
        contract.options.response_validation = mode;
        let original = FrameworkResponse::json(500, "{}");
        assert_eq!(
            profiles(&contract)
                .normalize(&contract, original.clone(), "test")
                .unwrap(),
            original
        );
    }
}

#[test]
fn direct_local_framework_errors_obey_selected_returned_response_media() {
    let contract = route();
    let profiles = profiles(&contract);
    let mut local = LocalRouter::new();
    local.register(contract, |_: FrameworkRequest| {
        Ok(FrameworkResponse::text(201, "PRIVATE"))
    });
    let response = local
        .execute_with_response_profiles(
            FrameworkRequest::new(
                HttpMethod::Get,
                "/default",
                RequestContext::new("direct-test"),
            ),
            ParserErrorFormat::Json,
            ResponseStatusErrorFormat::Json,
            &profiles,
        )
        .unwrap();
    assert_eq!(response.status_code, 500);
    assert_eq!(
        response.headers["content-type"],
        "text/plain; charset=utf-8"
    );
    assert!(!response.body.as_text().unwrap().contains("PRIVATE"));
    let contract = RouteContract::get("Uri", "Uri", "/:id")
        .with_param(forklaunch_core::contract::FieldContract::required(
            "id", "String",
        ))
        .with_response(200, None);
    let mut profiles = DefaultErrorResponses::default();
    profiles
        .insert(&contract, &[400, 500], 1024, Duration::from_secs(1))
        .unwrap();
    let mut local = LocalRouter::new();
    local.register(contract, |_: FrameworkRequest| {
        Ok(FrameworkResponse::text(200, "handler"))
    });
    let response = local
        .execute_with_response_profiles(
            FrameworkRequest::new(HttpMethod::Get, "/%GG", RequestContext::new("direct-test")),
            ParserErrorFormat::TypeScriptText,
            ResponseStatusErrorFormat::Json,
            &profiles,
        )
        .unwrap();
    assert_eq!(
        response.headers["content-type"],
        "text/plain; charset=utf-8"
    );
    // Direct local URI handling still predates the Express pre-context profile;
    // this proves selected status/envelope admission, not pre-context correlation.
    assert_eq!(response.status_code, 400);
}

#[test]
fn versioned_registrations_pin_every_branch_and_project_only_missing_statuses() {
    let original = route()
        .with_version(VersionedContract {
            version: "first".into(),
            request: RequestContract::default(),
            responses: vec![ResponseContract::new(201, None)],
        })
        .with_version(VersionedContract {
            version: "last".into(),
            request: RequestContract::default(),
            responses: vec![ResponseContract::new(202, None)],
        });
    for statuses in [vec![404], vec![400, 500], vec![201, 500], vec![500, 500]] {
        assert!(
            DefaultErrorResponses::default()
                .insert_versioned(&original, &statuses, 128, Duration::from_secs(1))
                .is_err()
        );
    }
    let mut profiles = DefaultErrorResponses::default();
    profiles
        .insert_versioned(&original, &[404, 500], 128, Duration::from_secs(1))
        .unwrap();
    profiles
        .validate_routes(std::slice::from_ref(&original))
        .unwrap();
    profiles
        .validate_execution(&original, ParserErrorFormat::Json)
        .unwrap();
    let mut stale = original.clone();
    stale.versions.get_mut("first").unwrap().request.query.push(
        forklaunch_core::contract::FieldContract::required("changed", "Changed"),
    );
    assert!(profiles.validate_route(&stale).is_err());
    let mut registry = ContractRegistry::new();
    registry.register(original.clone());
    let effective = profiles.project(&registry).unwrap().unwrap();
    assert_eq!(registry.routes()[0], original);
    assert_eq!(effective.routes()[0].responses, original.responses);
    for version in effective.routes()[0].versions.values() {
        assert_eq!(version.responses.len(), 3);
        assert_eq!(version.responses[1], declaration(404));
        assert_eq!(version.responses[2], declaration(500));
    }
    let artifact = effective.into_artifact().to_json();
    assert!(artifact.contains("text/plain"));
    assert!(artifact.contains("first") && artifact.contains("last"));
    let mut changed = ContractRegistry::new();
    changed.register(stale);
    assert!(profiles.project(&changed).is_err());
    let mut declared = original.clone();
    declared
        .versions
        .get_mut("last")
        .unwrap()
        .responses
        .push(declaration(500));
    assert!(
        DefaultErrorResponses::default()
            .insert_versioned(&declared, &[404, 500], 128, Duration::from_secs(1))
            .is_err()
    );
}

#[test]
fn local_versioned_default_errors_normalize_after_selection_and_preserve_head() {
    for method in [HttpMethod::Get, HttpMethod::Head] {
        let original = route().with_version(VersionedContract {
            version: "only".into(),
            request: RequestContract::default(),
            responses: vec![ResponseContract::new(200, None)],
        });
        let mut profiles = DefaultErrorResponses::default();
        profiles
            .insert_versioned(&original, &[404, 500], 128, Duration::from_secs(1))
            .unwrap();
        let mut router = LocalRouter::new();
        router.register(original, |request: FrameworkRequest| {
            assert_eq!(request.version.as_ref().unwrap().version, "only");
            Ok(FrameworkResponse::json(404, "\"message\"").with_header("x-test", "keep"))
        });
        let result = router
            .execute_with_response_profiles(
                FrameworkRequest::new(method, "/default", RequestContext::new("version")),
                ParserErrorFormat::Json,
                ResponseStatusErrorFormat::Json,
                &profiles,
            )
            .unwrap();
        assert_eq!(result.status_code, 404);
        assert_eq!(result.headers["x-test"], "keep");
        if method == HttpMethod::Head {
            assert_eq!(result.body, Body::Empty);
        } else {
            assert_eq!(result.body.as_text(), Some("message"));
        }
    }
}

#[test]
fn versioned_defaults_reject_unmatched_request_modes_and_stale_mode_changes() {
    let original = route().with_version(VersionedContract {
        version: "only".into(),
        request: RequestContract::default(),
        responses: vec![ResponseContract::new(200, None)],
    });
    let mut profiles = DefaultErrorResponses::default();
    profiles
        .insert_versioned(&original, &[404, 500], 128, Duration::from_secs(1))
        .unwrap();
    for mode in [ValidationMode::None, ValidationMode::Warning] {
        let mut changed = original.clone();
        changed.options.request_validation = mode;
        assert!(
            DefaultErrorResponses::default()
                .insert_versioned(&changed, &[404, 500], 128, Duration::from_secs(1))
                .is_err()
        );
        assert!(
            profiles
                .validate_routes(std::slice::from_ref(&changed))
                .is_err()
        );
        assert!(
            profiles
                .validate_execution(&changed, ParserErrorFormat::TypeScriptText)
                .is_err()
        );
        let mut registry = ContractRegistry::new();
        registry.register(changed.clone());
        assert!(profiles.project(&registry).is_err());
        let mut local = LocalRouter::new();
        local.register(changed, |_: FrameworkRequest| {
            panic!("stale profile must reject before handler")
        });
        assert!(
            local
                .execute_with_response_profiles(
                    FrameworkRequest::new(
                        HttpMethod::Get,
                        "/default",
                        RequestContext::new("stale")
                    ),
                    ParserErrorFormat::TypeScriptText,
                    ResponseStatusErrorFormat::TypeScriptText,
                    &profiles
                )
                .is_err()
        );
    }
}
