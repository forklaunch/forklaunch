//! Ordered matched-version response declarations, separate from request selection.
use crate::http::SelectedVersion;
use forklaunch_core::contract::{ResponseContract, RouteContract};
use std::{collections::BTreeMap, sync::Arc};

/// One matching request version's response declarations, in declaration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionResponseBranch {
    pub version: String,
    pub responses: Vec<ResponseContract>,
}

/// Local execution provenance for version-aware response validation.
/// A missing match after a handler ran is distinct from a framework rejection
/// before the handler. Transport adapters must preserve this distinction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionedResponseOutcome {
    pub matched: Option<MatchedVersionResponses>,
    pub handler_invoked: bool,
}

/// Trusted runtime selection for opt-in response union validation.
/// Duplicate status declarations remain distinct ordered schema branches.
/// This type does not interpret response schemas or change request/auth selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedVersionResponses {
    pub selected: SelectedVersion,
    pub branches: Vec<Arc<VersionResponseBranch>>,
}
impl MatchedVersionResponses {
    /// Header names that describe a replaced representation, rather than app metadata.
    #[must_use]
    pub fn is_representation_header(name: &str) -> bool {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "content-type"
                | "content-length"
                | "content-encoding"
                | "content-range"
                | "transfer-encoding"
                | "etag"
                | "digest"
                | "content-digest"
                | "repr-digest"
                | "content-md5"
                | "trailer"
                | "last-modified"
        )
    }

    /// Replace response declarations only, admitting statuses from every match.
    /// Only status codes are copied here; full schemas remain in shared branches
    /// for the trusted validator. Never export this runtime-only admission view.
    #[must_use]
    pub fn status_contract(&self, selected: &RouteContract) -> RouteContract {
        let mut response = selected.clone();
        response.responses = self
            .branches
            .iter()
            .flat_map(|branch| {
                branch
                    .responses
                    .iter()
                    .map(|response| ResponseContract::new(response.status_code, None))
            })
            .collect();
        response
    }
}

/// Immutable schema snapshots compiled once alongside a versioned route plan.
#[derive(Clone, Default)]
pub(crate) struct VersionResponsePlan {
    branches: BTreeMap<String, Arc<VersionResponseBranch>>,
}
impl VersionResponsePlan {
    pub(crate) fn compile(route: &RouteContract) -> Self {
        Self {
            branches: route
                .versions
                .keys()
                .filter_map(|name| {
                    let contract = route.for_version(name)?;
                    Some((
                        name.clone(),
                        Arc::new(VersionResponseBranch {
                            version: name.clone(),
                            responses: contract.responses,
                        }),
                    ))
                })
                .collect(),
        }
    }
    pub(crate) fn select(&self, selected: &SelectedVersion) -> Option<MatchedVersionResponses> {
        if selected.matched_versions.is_empty()
            || selected.matched_versions.last() != Some(&selected.version)
        {
            return None;
        }
        let branches = selected
            .matched_versions
            .iter()
            .map(|name| self.branches.get(name).cloned())
            .collect::<Option<Vec<_>>>()?;
        Some(MatchedVersionResponses {
            selected: selected.clone(),
            branches,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{
        FrameworkRequest, FrameworkResponse, HttpMethod, LocalRouter, ParserErrorFormat,
        RequestContext, ResponseStatusErrorFormat, VersionBodyHooks, VersionBodyHooksRegistry,
        VersionedRoutePlan,
    };
    use forklaunch_core::contract::{RequestContract, VersionedContract};
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn route() -> RouteContract {
        let mut route = RouteContract::get("Union", "Union", "/union");
        for (version, status) in [("z-first", 201), ("a-last", 202)] {
            route = route.with_version(VersionedContract {
                version: version.into(),
                request: RequestContract::default(),
                responses: vec![
                    ResponseContract::new(status, None),
                    ResponseContract::new(203, None),
                ],
            });
        }
        route
    }
    fn request() -> FrameworkRequest {
        FrameworkRequest::new(HttpMethod::Get, "/union", RequestContext::new("union"))
    }
    #[test]
    fn shared_branches_keep_order_duplicates_and_original_artifact() {
        let route = route();
        let before = route.to_json();
        let plan = VersionedRoutePlan::compile(&route).unwrap();
        let (_, selected) = plan.select(&request()).unwrap().unwrap();
        let first = plan.matched_response_versions(&selected).unwrap();
        let second = plan.matched_response_versions(&selected).unwrap();
        assert!(Arc::ptr_eq(&first.branches[0], &second.branches[0]));
        assert_eq!(
            first
                .branches
                .iter()
                .map(|branch| branch.version.as_str())
                .collect::<Vec<_>>(),
            vec!["z-first", "a-last"]
        );
        assert_eq!(
            first
                .status_contract(&route)
                .responses
                .iter()
                .map(|response| response.status_code)
                .collect::<Vec<_>>(),
            vec![201, 203, 202, 203]
        );
        assert_eq!(route.to_json(), before);
    }
    #[test]
    fn local_opt_in_keeps_last_request_selection_and_runs_hooks_once() {
        let mut route = route();
        for version in route.versions.values_mut() {
            version.request.body = Some(forklaunch_core::contract::BodyContract::json("Input"));
        }
        let input = request()
            .with_header("content-type", "application/json")
            .with_body(crate::http::Body::Text("{}".into()));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut hooks = VersionBodyHooksRegistry::default();
        for version in route.versions.keys() {
            let calls = calls.clone();
            hooks
                .insert(
                    &route,
                    version,
                    VersionBodyHooks::default().with_optional_transform(move |value| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(value.cloned())
                    }),
                )
                .unwrap();
        }
        let mut local = LocalRouter::new();
        local.register_with_body_hooks(route, &hooks, |request: FrameworkRequest| {
            assert_eq!(request.version.as_ref().unwrap().version, "a-last");
            Ok(FrameworkResponse::text(
                request
                    .query
                    .get("status")
                    .map_or(201, |value| value.parse().unwrap()),
                "earlier",
            )
            .with_header("x-app", "kept")
            .with_header("etag", "old"))
        });
        assert_eq!(local.execute(input.clone()).unwrap().status_code, 500);
        assert_eq!(calls.swap(0, Ordering::SeqCst), 2);
        let (response, matched) = local
            .execute_with_matched_response_versions(
                input.clone(),
                ParserErrorFormat::default(),
                ResponseStatusErrorFormat::default(),
                &crate::http::DefaultErrorResponses::default(),
            )
            .unwrap();
        assert_eq!(response.status_code, 201);
        assert_eq!(matched.matched.unwrap().branches.len(), 2);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let (failure, _) = local
            .execute_with_matched_response_versions(
                input.with_query("status", "204"),
                ParserErrorFormat::default(),
                ResponseStatusErrorFormat::TypeScriptText,
                &crate::http::DefaultErrorResponses::default(),
            )
            .unwrap();
        assert_eq!(failure.status_code, 500);
        assert_eq!(failure.headers["x-app"], "kept");
        assert!(!failure.headers.contains_key("etag"));
        assert!(!failure.body.as_text().unwrap().contains("earlier"));
    }
}
