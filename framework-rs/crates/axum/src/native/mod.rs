//! Additive helpers for ordinary Axum routers and application lifecycles.
//! Enable `native` with default features disabled; no legacy router is constructed.
/// Axum routing and state primitives exposed through the managed authoring facade.
pub use axum::{Router, extract::State};

mod bindings;
mod contracts;
mod docs;
pub use docs::{DocumentationUi, with_docs, with_docs_ui, with_openapi};
mod endpoint;
pub use endpoint::{
    ContractRouter, EndpointError, EndpointPolicy, NativeBody, PolicyCapabilities,
    VersionedHandlers,
};
#[cfg(feature = "native-streams")]
pub use endpoint::{
    ServerEvent, SocketSession, StreamContext, StreamOptions, VersionedSseHandlers,
};
pub use forklaunch_core::contract::endpoint::{FileMetadata, MultipartData};
pub use forklaunch_core::contract::{ContractRegistry, RouteContract};
mod messages;
pub use messages::JsonMessage;
mod stream;
pub use stream::{StreamClosed, StreamLifecycle};
mod security;
mod telemetry;
mod tracing;
pub use contracts::{export_contracts_from_env, write_contract_artifacts};
pub use security::{NativeHttpPolicy, NativeHttpPolicyError, NativeHttpRejection, secure};
use std::future::Future;
pub use telemetry::with_telemetry;
use tokio::sync::watch;
pub use tracing::instrument;

// Native router tests share tracing callsites. Serialize probes that execute
// those callsites while the privacy test installs its scoped capture subscriber.
#[cfg(test)]
static TRACE_TEST: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A cloneable, latched cancellation signal shared by servers and long-lived work.
#[derive(Clone, Debug)]
pub struct Shutdown(watch::Sender<bool>);
impl Default for Shutdown {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}
impl Shutdown {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancel existing work and any work that subscribes afterward.
    pub fn trigger(&self) {
        self.0.send_replace(true);
    }

    /// Wait for cancellation. Safe to drop and recreate in a select loop.
    pub async fn cancelled(&self) {
        let mut receiver = self.0.subscribe();
        loop {
            if *receiver.borrow_and_update() {
                return;
            }
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }

    /// Run work until completion or cancellation. Cancellation drops the future,
    /// including an in-flight network/database operation; application transactions
    /// and external side effects retain their own rollback/idempotency semantics.
    pub async fn run_until_cancelled<F: Future>(&self, work: F) -> Option<F::Output> {
        tokio::select! { biased;
            () = self.cancelled() => None,
            output = work => Some(output),
        }
    }

    /// Wait for SIGTERM or Ctrl-C, then cancel subscribers. Signal registration
    /// failure also triggers cancellation and emits a fixed, non-sensitive error.
    pub async fn signal(self) {
        #[cfg(unix)]
        {
            if let Ok(mut terminate) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            } else {
                ::tracing::error!("could not install shutdown signal handler");
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        self.trigger();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn cancellation_is_latched_and_prevents_new_work() {
        let shutdown = Shutdown::new();
        shutdown.trigger();
        assert_eq!(
            shutdown
                .run_until_cancelled(async {
                    assert!(
                        std::hint::black_box(false),
                        "cancelled work must not be polled"
                    );
                })
                .await,
            None::<()>
        );
        timeout(Duration::from_secs(1), shutdown.cancelled())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn cancellation_drops_inflight_work_and_releases_all_waiters() {
        struct Dropped(Arc<AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let shutdown = Shutdown::new();
        let dropped = Arc::new(AtomicBool::new(false));
        let started = Arc::new(tokio::sync::Notify::new());
        let task = {
            let stop = shutdown.clone();
            let dropped = dropped.clone();
            let started = started.clone();
            tokio::spawn(async move {
                stop.run_until_cancelled(async move {
                    let _guard = Dropped(dropped);
                    started.notify_one();
                    std::future::pending::<()>().await;
                })
                .await
            })
        };
        started.notified().await;
        let waiter = {
            let stop = shutdown.clone();
            tokio::spawn(async move { stop.cancelled().await })
        };
        shutdown.trigger();
        assert_eq!(
            timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap(),
            None
        );
        timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
        assert!(dropped.load(Ordering::SeqCst));
    }
}

#[cfg(feature = "native-server")]
mod application;
#[cfg(feature = "native-server")]
pub use application::{ApplicationResult, NativeApplication, run_application, with_cleanup};

mod session_policy;
pub use session_policy::SessionPolicy;
#[cfg(feature = "native-jwt")]
mod jwt;
#[cfg(feature = "native-jwt")]
pub use jwt::{jwt_policy, jwt_policy_from_env, optional_jwt_policy_from_env};
