//! Installed-registry smoke service; no repository path dependencies.
use axum::Json;
use forklaunch_axum::ForkLaunchRouter;
use forklaunch_core::contract::{BodyContract, RouteContract};
use forklaunch_core::ServiceInfo;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "bounded-regex")]
    check_bounded_regex();
    let host = std::env::var("SERVICE_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("SERVICE_PORT").unwrap_or_else(|_| "0".into());
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await?;
    let router = ForkLaunchRouter::new(ServiceInfo::new("registry-smoke", "0.1.0")).get(
        RouteContract::get("Health", "Registry-installed service health", "/health")
            .with_response(200, Some(BodyContract::json("Health"))),
        || async { Json(json!({"ok": true})) },
    );
    println!(
        "registry-smoke listening on http://{}",
        listener.local_addr()?
    );
    router.serve_with_default_mcp(listener, shutdown()).await?;
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .expect("install shutdown handler");
}

// Exercise the optional API from registry-installed crates before serving requests.
#[cfg(feature = "bounded-regex")]
fn check_bounded_regex() {
    use forklaunch_common::zod_regex::{
        RegexCompileError, RegexCompileLimits, RegexMatchLimits, RegexResourceLimit, ZodRegex,
    };
    let pattern: Vec<u16> = r"^\p{Letter}+$".encode_utf16().collect();
    let regex = ZodRegex::compile(&pattern, "u", RegexCompileLimits::default()).unwrap();
    assert_eq!(regex.test("Rust", RegexMatchLimits::default()), Ok(true));
    assert_eq!(regex.test("42", RegexMatchLimits::default()), Ok(false));
    assert_eq!(
        regex.test(
            "Rust",
            RegexMatchLimits {
                work: 0,
                ..RegexMatchLimits::default()
            }
        ),
        Err(RegexResourceLimit::Work),
    );
    assert!(matches!(
        ZodRegex::compile(&pattern, "uv", RegexCompileLimits::default()),
        Err(RegexCompileError::InvalidFlags)
    ));
}
