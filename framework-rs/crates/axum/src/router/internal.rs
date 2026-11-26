//! Application-owned HTTP mounts, independent of exported route contracts.
use super::{ForkLaunchRouter, RouterConfig};
use axum::Router;
use forklaunch_common::join_base_path;
use forklaunch_core::contract::ContractRegistry;

pub(super) struct InternalMount {
    pub(super) prefix: String,
    pub(super) reserved_prefixes: Vec<String>,
    pub(super) router: Router,
}

impl ForkLaunchRouter {
    /// Mounts an application-owned HTTP API under a fixed, reserved prefix.
    /// Handlers own authentication and validation. Shared body limits, CORS,
    /// request IDs and telemetry still apply. These routes have no contract
    /// entries and therefore create no `OpenAPI`, SDK or MCP operations.
    ///
    /// # Panics
    /// Panics if the prefix is empty, root, parameterized or otherwise invalid.
    /// Building the router also rejects overlapping namespaces, contracts and
    /// documentation endpoints.
    #[must_use]
    pub fn nest_internal(mut self, prefix: impl AsRef<str>, router: Router) -> Self {
        let prefix = prefix.as_ref();
        assert!(
            fixed_prefix(prefix),
            "internal HTTP mount requires a fixed absolute prefix"
        );
        let prefix = join_base_path(&self.base_path, prefix);
        assert!(prefix != "/", "internal HTTP mount cannot reserve the root");
        self.internal_mounts.push(InternalMount {
            reserved_prefixes: vec![prefix.clone()],
            prefix,
            router,
        });
        self
    }
    /// Merges an application-owned API while reserving its declared namespaces.
    /// Use this for APIs mounted at the application root. The caller must list
    /// every namespace registered by `router`; contracts/docs/MCP cannot claim
    /// these prefixes. Shared HTTP middleware still applies.
    /// # Panics
    /// Rejects empty reservations, root/parameterized prefixes or invalid paths.
    /// Router construction rejects overlapping namespaces and contract/docs paths.
    #[must_use]
    pub fn merge_internal<P: AsRef<str>>(
        mut self,
        router: Router,
        namespaces: impl IntoIterator<Item = P>,
    ) -> Self {
        let reserved_prefixes = namespaces
            .into_iter()
            .map(|prefix| {
                let prefix = prefix.as_ref();
                assert!(
                    fixed_prefix(prefix) && !prefix.trim_matches('/').is_empty(),
                    "internal HTTP merge requires fixed non-root namespaces"
                );
                join_base_path(&self.base_path, prefix)
            })
            .collect::<Vec<_>>();
        assert!(
            !reserved_prefixes.is_empty(),
            "internal HTTP merge requires declared namespaces"
        );
        self.internal_mounts.push(InternalMount {
            prefix: join_base_path(&self.base_path, "/"),
            reserved_prefixes,
            router,
        });
        self
    }
}

fn fixed_prefix(prefix: &str) -> bool {
    prefix.starts_with('/')
        && !prefix.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '{' | '}' | ':' | '*' | '?' | '#' | '%' | '\\')
        })
        && !prefix.split('/').any(|part| matches!(part, "." | ".."))
}

pub(super) fn contains_path(prefix: &str, path: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn intersects_route(prefix: &str, route: &str) -> bool {
    let mut route = route.split('/').filter(|segment| !segment.is_empty());
    for expected in prefix.split('/').filter(|segment| !segment.is_empty()) {
        let Some(segment) = route.next() else {
            return false;
        };
        if segment.starts_with("{*") {
            return true;
        }
        if segment != expected && !(segment.starts_with('{') && segment.ends_with('}')) {
            return false;
        }
    }
    true
}

pub(super) fn validate(
    mounts: &[InternalMount],
    registry: &ContractRegistry,
    config: &RouterConfig,
) -> Result<(), &'static str> {
    let hash_path = format!("{}-hash", config.openapi_path);
    let docs = [
        config
            .openapi_enabled
            .then_some(config.openapi_path.as_str()),
        config.openapi_enabled.then_some(hash_path.as_str()),
        (config.openapi_enabled && config.docs_enabled).then_some(config.docs_path.as_str()),
        (config.openapi_enabled && config.swagger_enabled).then_some(config.swagger_path.as_str()),
    ];
    let mut reserved = Vec::new();
    for mount in mounts {
        if !fixed_prefix(&mount.prefix) || mount.reserved_prefixes.is_empty() {
            return Err("internal HTTP mount requires a fixed prefix and declared namespaces");
        }
        for prefix in &mount.reserved_prefixes {
            if !fixed_prefix(prefix)
                || prefix == "/"
                || (mount.prefix != "/" && !contains_path(&mount.prefix, prefix))
            {
                return Err(
                    "internal HTTP mount requires fixed non-root namespaces within its prefix",
                );
            }
            if reserved
                .iter()
                .any(|other: &&str| contains_path(prefix, other) || contains_path(other, prefix))
            {
                return Err("overlapping internal HTTP namespaces");
            }
            if registry
                .routes()
                .iter()
                .any(|route| intersects_route(prefix, &route.openapi_path))
            {
                return Err("internal HTTP namespace conflicts with a registered contract");
            }
            if docs
                .iter()
                .flatten()
                .any(|path| intersects_route(prefix, path))
            {
                return Err("internal HTTP namespace conflicts with documentation");
            }
            reserved.push(prefix.as_str());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
        routing::{get, post},
    };
    use forklaunch_core::contract::RouteContract;
    use tower::ServiceExt;

    #[tokio::test]
    async fn merged_internal_namespaces_preserve_application_routes_and_parent_paths() {
        let raw = Router::new().route("/token", get(|| async { "auth" }));
        let child = ForkLaunchRouter::anonymous()
            .with_base_path("/v1")
            .merge_internal(raw, ["/token"])
            .get(
                RouteContract::get("Discovery", "Discovery", "/discovery"),
                || async { "application" },
            );
        let app = ForkLaunchRouter::anonymous()
            .with_base_path("/platform")
            .nest("/tenant", child);
        assert_eq!(app.registry().routes().len(), 1);
        let app = app.into_router();
        for (path, expected) in [
            ("/platform/tenant/v1/token", "auth"),
            ("/platform/tenant/v1/discovery", "application"),
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                to_bytes(response.into_body(), 1024).await.unwrap(),
                expected
            );
        }
        for path in ["/token", "/platform/tenant/token"] {
            assert_eq!(
                app.clone()
                    .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .status(),
                StatusCode::NOT_FOUND
            );
        }
    }

    #[test]
    fn merged_internal_namespaces_reject_contract_docs_and_mount_collisions() {
        for route in ["/token", "/token/nested", "/{group}"] {
            let app = ForkLaunchRouter::anonymous()
                .merge_internal(Router::new(), ["/token"])
                .get(RouteContract::get("Overlap", "Overlap", route), || async {
                    "overlap"
                });
            assert!(validate(&app.internal_mounts, &app.registry, &app.config).is_err());
        }
        let app = ForkLaunchRouter::anonymous().merge_internal(Router::new(), ["/docs"]);
        assert!(validate(&app.internal_mounts, &app.registry, &app.config).is_err());
        let app = ForkLaunchRouter::anonymous()
            .merge_internal(Router::new(), ["/token", "/session"])
            .nest_internal("/session/refresh", Router::new());
        assert!(validate(&app.internal_mounts, &app.registry, &app.config).is_err());
        let app = ForkLaunchRouter::anonymous()
            .merge_internal(Router::new(), ["/token", "/token/refresh"]);
        assert!(validate(&app.internal_mounts, &app.registry, &app.config).is_err());
    }

    #[tokio::test]
    async fn nested_internal_mounts_keep_raw_bodies_limits_and_request_ids() {
        let internal = Router::new()
            .route("/session", get(|| async { "session" }))
            .route("/echo", post(|body: axum::body::Bytes| async { body }));
        let child = ForkLaunchRouter::anonymous()
            .with_base_path("/v1")
            .nest_internal("/auth", internal);
        let config = RouterConfig {
            body_limit: 8,
            ..RouterConfig::default()
        };
        let app = ForkLaunchRouter::anonymous()
            .with_base_path("/platform")
            .nest("/tenant", child)
            .with_config(config);
        assert!(app.registry().routes().is_empty());
        let app = app.into_router();
        let session = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/platform/tenant/v1/auth/session")
                    .header("x-request-id", "internal-request")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(session.status(), StatusCode::OK);
        assert_eq!(session.headers()["x-request-id"], "internal-request");
        let binary = vec![0, 255, 1, 42];
        let echoed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/platform/tenant/v1/auth/echo")
                    .body(Body::from(binary.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(echoed.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(echoed.into_body(), 8).await.unwrap().as_ref(),
            binary
        );
        let limited = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/platform/tenant/v1/auth/echo")
                    .header("content-length", "9")
                    .body(Body::from("too large"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(limited.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn namespaces_reject_overlapping_contracts_docs_and_other_mounts() {
        for route in ["/auth", "/auth/token", "/{group}/token", "/{group}"] {
            let app = ForkLaunchRouter::anonymous()
                .nest_internal("/auth", Router::new())
                .get(RouteContract::get("Overlap", "Overlap", route), || async {
                    "overlap"
                });
            assert!(
                validate(&app.internal_mounts, &app.registry, &app.config).is_err(),
                "{route}"
            );
        }
        let app = ForkLaunchRouter::anonymous()
            .nest_internal("/auth", Router::new())
            .nest_internal("/auth/deep", Router::new());
        assert!(validate(&app.internal_mounts, &app.registry, &app.config).is_err());
        let config = RouterConfig {
            openapi_path: "/auth/openapi.json".into(),
            ..RouterConfig::default()
        };
        let app = ForkLaunchRouter::anonymous()
            .nest_internal("/auth", Router::new())
            .with_config(config);
        assert!(validate(&app.internal_mounts, &app.registry, &app.config).is_err());
        let app = ForkLaunchRouter::anonymous()
            .nest_internal("/auth", Router::new())
            .nest_internal("/authentication", Router::new())
            .get(
                RouteContract::get("Other", "Other", "/authenticator"),
                || async { "other" },
            );
        assert!(validate(&app.internal_mounts, &app.registry, &app.config).is_ok());
    }

    #[test]
    fn parent_base_paths_cannot_parameterize_internal_namespaces() {
        let child = ForkLaunchRouter::anonymous().nest_internal("/auth", Router::new());
        let app = ForkLaunchRouter::anonymous().nest("/{tenant}", child);
        assert!(validate(&app.internal_mounts, &app.registry, &app.config).is_err());
        let app = ForkLaunchRouter::anonymous()
            .with_base_path("/{tenant}")
            .nest_internal("/auth", Router::new());
        assert!(validate(&app.internal_mounts, &app.registry, &app.config).is_err());
    }

    #[cfg(feature = "mcp")]
    #[test]
    fn mcp_endpoints_cannot_claim_internal_namespaces() {
        for path in ["/auth", "/auth/mcp", "/auth/nested/tools"] {
            let app = ForkLaunchRouter::anonymous().nest_internal("/auth", Router::new());
            let mut options = forklaunch_core::mcp::server::McpServerOptions::new("internal", "1");
            options.path = path.into();
            assert!(app.into_routers_with_mcp(options).is_err(), "{path}");
        }
    }
}
