//! Trusted request-bound version outcomes for authentication-before-dispatch adapters.
use super::{Selection, SelectionOutcome, VersionedRoutePlan};
use crate::http::{FrameworkRequest, MultipartLimits};
use std::sync::Arc;

/// A compiled plan's selection result, bound to the original request inputs.
/// Fields are private: caller-supplied selected-version metadata cannot replace
/// schema execution. Clones share the input snapshot and outcome. Mount paths
/// and subsequently authenticated sessions may change; schema inputs may not.
#[derive(Clone)]
pub struct PreparedVersionSelection {
    identity: Arc<()>,
    request: Arc<FrameworkRequest>,
    outcome: Arc<SelectionOutcome>,
}
impl std::fmt::Debug for PreparedVersionSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedVersionSelection")
            .finish_non_exhaustive()
    }
}
impl VersionedRoutePlan {
    pub(crate) fn select_reusing(
        &self,
        request: &mut FrameworkRequest,
        prepared: Option<&PreparedVersionSelection>,
    ) -> forklaunch_common::FrameworkResult<Selection> {
        if let Some(prepared) = prepared {
            self.apply_prepared(prepared, request).map_err(|message| {
                forklaunch_common::FrameworkError::new("invalid_prepared_version", message)
            })
        } else {
            Ok(self.select_into_request(request, MultipartLimits::default()))
        }
    }
    /// Select once and replace derived request projections without retaining a snapshot.
    /// # Errors
    /// Returns parser or version schema errors, with first-parser output retained.
    pub fn select_into_request(
        &self,
        request: &mut FrameworkRequest,
        limits: MultipartLimits,
    ) -> Selection {
        let outcome = self.selection_outcome(request, limits);
        request.version = None;
        request.version_parsed_body = outcome.parsed;
        outcome.selection
    }
    /// Resolve trusted selection and replace any caller-supplied derived projections.
    /// # Errors
    /// Rejects changed schema inputs or a different compiled plan.
    pub fn apply_prepared(
        &self,
        prepared: &PreparedVersionSelection,
        request: &mut FrameworkRequest,
    ) -> Result<Selection, String> {
        let selection = self.resolve_prepared(prepared, request)?;
        request.version = None;
        request
            .version_parsed_body
            .clone_from(&prepared.outcome.parsed);
        Ok(selection)
    }
    /// Run selection once and retain a trusted outcome for later dispatch.
    /// The opt-in adapter holds one request snapshot until dispatch completes;
    /// transport body/admission limits must already have been applied.
    #[must_use]
    pub fn prepare_selection(
        &self,
        request: &FrameworkRequest,
        limits: MultipartLimits,
    ) -> PreparedVersionSelection {
        PreparedVersionSelection {
            identity: self.identity.clone(),
            request: Arc::new(request.clone()),
            outcome: Arc::new(self.selection_outcome(request, limits)),
        }
    }
    /// Reuse an outcome only with the same compiled plan and schema inputs.
    /// Authentication may add a session, and nested transport adapters may strip
    /// their path prefix. Path captures and every other input remain bound.
    /// # Errors
    /// Rejects outcomes from a different plan or changed request input.
    pub fn resolve_prepared(
        &self,
        prepared: &PreparedVersionSelection,
        request: &FrameworkRequest,
    ) -> Result<Selection, String> {
        let before = &prepared.request;
        if !Arc::ptr_eq(&self.identity, &prepared.identity)
            || before.context != request.context
            || before.method != request.method
            || before.params != request.params
            || before.query != request.query
            || before.raw_query != request.raw_query
            || before.headers != request.headers
            || before.body != request.body
            || before.parsed_fields != request.parsed_fields
            || before.parsed_json != request.parsed_json
        {
            return Err("Prepared version selection does not match this plan and request".into());
        }
        Ok(prepared.outcome.selection.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpMethod, RequestContext};
    use forklaunch_core::contract::{RequestContract, RouteContract, VersionedContract};
    #[test]
    fn prepared_selection_rejects_changed_inputs_and_other_compiled_plans() {
        let route = RouteContract::get("prepared", "prepared", "/prepared").with_version(
            VersionedContract {
                version: "one".into(),
                request: RequestContract::default(),
                responses: vec![],
            },
        );
        let plan = VersionedRoutePlan::compile(&route).unwrap();
        let request =
            FrameworkRequest::new(HttpMethod::Get, "/prepared", RequestContext::new("id"));
        let prepared = plan.prepare_selection(&request, MultipartLimits::default());
        assert!(plan.clone().resolve_prepared(&prepared, &request).is_ok());
        assert!(
            VersionedRoutePlan::compile(&route)
                .unwrap()
                .resolve_prepared(&prepared, &request)
                .is_err()
        );
        for mutate in [
            |r: &mut FrameworkRequest| {
                r.query.insert("x".into(), "changed".into());
            },
            |r: &mut FrameworkRequest| {
                r.headers.insert("authorization".into(), "changed".into());
            },
            |r: &mut FrameworkRequest| {
                r.body = crate::http::Body::Text("changed".into());
            },
            |r: &mut FrameworkRequest| {
                r.params.insert("id".into(), "changed".into());
            },
            |r: &mut FrameworkRequest| {
                r.raw_query = Some("changed".into());
            },
            |r: &mut FrameworkRequest| {
                r.context.correlation_id = "changed".into();
            },
        ] {
            let mut changed = request.clone();
            mutate(&mut changed);
            assert!(plan.resolve_prepared(&prepared, &changed).is_err());
        }
        let mut authenticated = request;
        authenticated.path = "/mounted/prepared".into();
        authenticated.session = Some(serde_json::json!({"sub":"verified"}));
        assert!(plan.resolve_prepared(&prepared, &authenticated).is_ok());
    }
    #[test]
    fn permissive_no_match_cannot_reuse_caller_supplied_selected_version() {
        use crate::http::{
            DefaultErrorResponses, LocalRouter, ParserErrorFormat, ResponseStatusErrorFormat,
        };
        use forklaunch_core::contract::{
            FieldContract, ResponseContract, SchemaRef, ValidationMode,
        };
        let mut field = FieldContract::required("tag", "Tag");
        field.schema = SchemaRef::named("Tag").with_json_schema(r#"{"const":"valid"}"#);
        let mut route =
            RouteContract::get("spoof", "spoof", "/spoof").with_version(VersionedContract {
                version: "one".into(),
                request: RequestContract {
                    query: vec![field],
                    ..RequestContract::default()
                },
                responses: vec![ResponseContract::new(200, None)],
            });
        route.options.request_validation = ValidationMode::None;
        route.options.response_validation = ValidationMode::None;
        let mut local = LocalRouter::new();
        local.register(route.clone(), |request: FrameworkRequest| {
            assert!(request.version.is_none());
            Ok(crate::http::FrameworkResponse::text(200, "ok"))
        });
        let plan = local.version_plan_for(&route).unwrap();
        let mut valid = FrameworkRequest::new(HttpMethod::Get, "/spoof", RequestContext::new("id"));
        valid.query.insert("tag".into(), "valid".into());
        let forged = plan.select(&valid).unwrap().unwrap().1;
        let mut invalid = valid;
        invalid.query.insert("tag".into(), "invalid".into());
        let prepared = plan.prepare_selection(&invalid, MultipartLimits::default());
        invalid.version = Some(forged);
        let (response, outcome) = local
            .execute_with_prepared_version(
                invalid,
                &prepared,
                ParserErrorFormat::Json,
                ResponseStatusErrorFormat::Json,
                &DefaultErrorResponses::default(),
                true,
            )
            .unwrap();
        assert_eq!(response.status_code, 200);
        assert!(outcome.matched.is_none());
        assert!(outcome.handler_invoked);
    }
}
