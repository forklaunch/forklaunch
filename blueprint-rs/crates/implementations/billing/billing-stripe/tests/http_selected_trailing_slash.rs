#![cfg(feature = "http")]
//! Transport aliases keep one contract per operation and retain each method's policy.
use axum::extract::Path;
use forklaunch_axum::{ForkLaunchRouter, RouterConfig};
use forklaunch_core::contract::{AuthConfig, RouteContract};
use forklaunch_core::http::{ErrorKind, ErrorResponse};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn path_wide_aliases_preserve_nested_method_policy_and_canonical_registry() {
    let get = RouteContract::get("Read", "Read", "/mixed/:id").with_response(200, None);
    let put = RouteContract::put("Write", "Write", "/mixed/:id")
        .with_auth(AuthConfig::hmac())
        .with_response(200, None);
    let root = RouteContract::get("Root", "Root", "/mount").with_response(200, None);
    let child = ForkLaunchRouter::anonymous()
        .get(get, |Path(id): Path<String>| async move { id })
        .put(put, || async { "must not execute" })
        .get(root, || async { "root" })
        .with_config(
            RouterConfig::default()
                .with_optional_trailing_slash_for_path("/mixed/:id")
                .with_optional_trailing_slash_for_path("/mount"),
        );
    let observations = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&observations);
    let config = RouterConfig::default().with_response_authenticator(move |request, contract| {
        let observed = Arc::clone(&observed);
        async move {
            observed.fetch_add(1, Ordering::SeqCst);
            assert_eq!(contract.path, "/api/mixed/:id");
            assert_eq!(request.raw_path.as_deref(), Some("/api/mixed/owned/"));
            Err(ErrorResponse::new(ErrorKind::Unauthorized, "locked"))
        }
    });
    let router = ForkLaunchRouter::anonymous()
        .with_config(config)
        .nest("/api", child);
    assert_eq!(router.registry().routes().len(), 3);
    assert!(
        router
            .registry()
            .routes()
            .iter()
            .all(|route| !route.path.ends_with('/'))
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task =
        tokio::spawn(async move { axum::serve(listener, router.into_router()).await.unwrap() });
    let http = reqwest::Client::new();
    for path in ["/api/mixed/owned", "/api/mixed/owned/"] {
        let response = http.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.text().await.unwrap(), "owned");
        let response = http.head(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.text().await.unwrap(), "");
        // Current native OPTIONS handling remains a distinct Express parity gap.
        let response = http
            .request(reqwest::Method::OPTIONS, format!("{base}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 405);
    }
    for path in ["/api/mount", "/api/mount/"] {
        let response = http.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.text().await.unwrap(), "root");
    }
    assert_eq!(observations.load(Ordering::SeqCst), 0);
    let response = http
        .put(format!("{base}/api/mixed/owned/"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(observations.load(Ordering::SeqCst), 1);
    for path in ["/api/mixed/owned//", "/api/mount//", "/api/MIXED/owned/"] {
        assert_eq!(
            http.get(format!("{base}{path}"))
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
    }
    task.abort();
    let _ = task.await;
}
