//! Pre-authentication preparation and trusted local dispatch reuse.
use super::{
    Arc, DefaultErrorResponses, FrameworkRequest, FrameworkResponse, LocalRouter,
    ParserErrorFormat, Parts, Response, ResponseStatusErrorFormat, RouteContract, ValidationState,
    framework_response_to_axum, response_validation, route_key, select_contract_version,
    validation_error_response_with_parser_format,
};

pub(super) fn dispatch_local(
    handlers: &LocalRouter,
    request: FrameworkRequest,
    parts: &Parts,
) -> forklaunch_common::FrameworkResult<(
    FrameworkResponse,
    Option<forklaunch_core::http::VersionedResponseOutcome>,
)> {
    let parser_format = parts
        .extensions
        .get::<ParserErrorFormat>()
        .copied()
        .unwrap_or_default();
    let response_status_format = parts
        .extensions
        .get::<ResponseStatusErrorFormat>()
        .copied()
        .unwrap_or_default();
    let default_errors = parts
        .extensions
        .get::<Arc<DefaultErrorResponses>>()
        .cloned()
        .unwrap_or_default();
    let versioned = parts
        .extensions
        .get::<response_validation::VersionedResponseValidation>()
        .is_some();
    let prepared = parts
        .extensions
        .get::<forklaunch_core::http::PreparedVersionSelection>();
    if let Some(prepared) = prepared {
        handlers
            .execute_with_prepared_version(
                request,
                prepared,
                parser_format,
                response_status_format,
                &default_errors,
                versioned,
            )
            .map(|(response, outcome)| (response, versioned.then_some(outcome)))
    } else if versioned {
        handlers
            .execute_with_matched_response_versions(
                request,
                parser_format,
                response_status_format,
                &default_errors,
            )
            .map(|(response, outcome)| (response, Some(outcome)))
    } else {
        handlers
            .execute_with_response_profiles(
                request,
                parser_format,
                response_status_format,
                &default_errors,
            )
            .map(|response| (response, None))
    }
}

pub(super) fn prepare_before_authentication(
    state: &ValidationState,
    contract: &RouteContract,
    parts: &mut Parts,
    framework_request: &mut FrameworkRequest,
) -> Option<Response> {
    let key = route_key(contract.method, &contract.openapi_path);
    if state.pre_auth_versions.contains(&key) {
        let plan = &state.versions[&key];
        let limits = parts
            .extensions
            .get::<forklaunch_core::http::MultipartLimits>()
            .copied()
            .unwrap_or_default();
        let prepared = plan.prepare_selection(framework_request, limits);
        parts.extensions.insert(prepared);
        if let Err(errors) =
            select_contract_version(plan, contract.clone(), parts, framework_request)
        {
            return Some(framework_response_to_axum(
                validation_error_response_with_parser_format(
                    &errors,
                    &framework_request.context.correlation_id,
                    state.parser_error_format,
                ),
            ));
        }
    }
    None
}
