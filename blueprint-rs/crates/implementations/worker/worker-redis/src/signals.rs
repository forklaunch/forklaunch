//! Tenant/namespace/queue-bound Redis Pub/Sub control. Delivery is ephemeral:
//! disconnected and late subscribers miss publications. Redis ACLs remain required.
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::persistence::TenantContext;
use forklaunch_worker_interface::{AsyncSignalBus, DistributedSignal};
use futures_util::StreamExt;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use std::{
    fmt,
    fmt::Write as _,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Debug, Clone)]
pub struct SignalOptions {
    pub capacity: usize,
    pub operation_timeout: Duration,
}
impl Default for SignalOptions {
    fn default() -> Self {
        Self {
            capacity: 256,
            operation_timeout: Duration::from_secs(5),
        }
    }
}
#[derive(Clone)]
pub struct RedisSignalPublisher {
    connection: ConnectionManager,
    channel: String,
    timeout: Duration,
}
impl fmt::Debug for RedisSignalPublisher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisSignalPublisher")
            .finish_non_exhaustive()
    }
}
impl RedisSignalPublisher {
    /// Creates an administrative publisher without subscribing to its channel.
    /// # Errors
    /// Rejects invalid configuration and returns sanitized, bounded connection errors.
    pub async fn connect(
        url: &str,
        namespace: &str,
        tenant: &TenantContext,
        queue: &str,
        options: &SignalOptions,
    ) -> BlueprintResult<Self> {
        Self::connect_scoped(
            url,
            namespace,
            tenant,
            queue,
            options,
            &crate::tenant_isolation::RedisKeyScope::from_env()?,
        )
        .await
    }

    /// Creates a publisher with an explicitly injected platform channel scope.
    /// # Errors
    /// Rejects invalid input and returns sanitized connection failures.
    pub async fn connect_scoped(
        url: &str,
        namespace: &str,
        tenant: &TenantContext,
        queue: &str,
        options: &SignalOptions,
        scope: &crate::tenant_isolation::RedisKeyScope,
    ) -> BlueprintResult<Self> {
        let channel = scope.qualify(&channel_name(namespace, tenant, queue, options)?);
        let client = redis::Client::open(url).map_err(|_| storage_error())?;
        let timeout = options.operation_timeout;
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(timeout))
            .set_response_timeout(Some(timeout))
            .set_number_of_retries(2);
        let connection =
            tokio::time::timeout(timeout, ConnectionManager::new_with_config(client, config))
                .await
                .map_err(|_| storage_error())?
                .map_err(|error| redis_error(&error))?;
        Ok(Self {
            connection,
            channel,
            timeout,
        })
    }
    /// Publishes to currently connected subscribers. The count is not an ack.
    /// # Errors
    /// Returns a sanitized error when publication fails or exceeds its deadline.
    pub async fn broadcast(&self, signal: DistributedSignal) -> BlueprintResult<usize> {
        let mut connection = self.connection.clone();
        tokio::time::timeout(
            self.timeout,
            redis::cmd("PUBLISH")
                .arg(&self.channel)
                .arg(signal.as_str())
                .query_async(&mut connection),
        )
        .await
        .map_err(|_| storage_error())?
        .map_err(|error| redis_error(&error))
    }
}
/// A single bounded subscription. Overflow, malformed publications and disconnects
/// are terminal errors; reconnect explicitly with a fresh bus. Dropping it aborts
/// its listener. Publishers cloned from it remain usable independently.
pub struct RedisSignalBus {
    publisher: RedisSignalPublisher,
    receiver: mpsc::Receiver<DistributedSignal>,
    health: Arc<AtomicU8>,
    listener: JoinHandle<()>,
}
impl fmt::Debug for RedisSignalBus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisSignalBus")
            .field("health", &self.health.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}
impl Drop for RedisSignalBus {
    fn drop(&mut self) {
        self.listener.abort();
    }
}
impl RedisSignalBus {
    /// Connects and confirms subscription before returning. The URL supplies TLS,
    /// credentials and database configuration. Pub/Sub ignores Redis database
    /// numbers: scope deployments with distinct namespaces and Redis ACLs.
    /// # Errors
    /// Rejects invalid bounds/names and returns sanitized connection errors.
    pub async fn connect(
        url: &str,
        namespace: &str,
        tenant: &TenantContext,
        queue: &str,
        options: SignalOptions,
    ) -> BlueprintResult<Self> {
        Self::connect_scoped(
            url,
            namespace,
            tenant,
            queue,
            options,
            &crate::tenant_isolation::RedisKeyScope::from_env()?,
        )
        .await
    }

    /// Subscribes with an explicitly injected platform channel scope.
    /// # Errors
    /// Rejects invalid input and returns sanitized connection/subscription failures.
    pub async fn connect_scoped(
        url: &str,
        namespace: &str,
        tenant: &TenantContext,
        queue: &str,
        options: SignalOptions,
        scope: &crate::tenant_isolation::RedisKeyScope,
    ) -> BlueprintResult<Self> {
        let channel = scope.qualify(&channel_name(namespace, tenant, queue, &options)?);
        let client = redis::Client::open(url).map_err(|_| storage_error())?;
        let timeout = options.operation_timeout;
        let setup = async {
            let config = ConnectionManagerConfig::new()
                .set_connection_timeout(Some(timeout))
                .set_response_timeout(Some(timeout))
                .set_number_of_retries(2);
            let connection = ConnectionManager::new_with_config(client.clone(), config)
                .await
                .map_err(|error| redis_error(&error))?;
            verify_subscription(&client, &channel).await?;
            let mut subscription = client
                .get_async_pubsub()
                .await
                .map_err(|error| redis_error(&error))?;
            subscription
                .subscribe(&channel)
                .await
                .map_err(|error| redis_error(&error))?;
            Ok::<_, BlueprintError>((connection, subscription))
        };
        let (connection, subscription) = tokio::time::timeout(timeout, setup)
            .await
            .map_err(|_| storage_error())??;
        let (sender, receiver) = mpsc::channel(options.capacity);
        let health = Arc::new(AtomicU8::new(0));
        let status = health.clone();
        let listener = tokio::spawn(async move {
            let mut stream = subscription.into_on_message();
            while let Some(message) = stream.next().await {
                let Some(signal) = decode_signal(message.get_payload_bytes()) else {
                    status.store(2, Ordering::Release);
                    return;
                };
                if sender.try_send(signal).is_err() {
                    status.store(3, Ordering::Release);
                    return;
                }
            }
            status.store(1, Ordering::Release);
        });
        Ok(Self {
            publisher: RedisSignalPublisher {
                connection,
                channel,
                timeout,
            },
            receiver,
            health,
            listener,
        })
    }
    #[must_use]
    pub fn publisher(&self) -> RedisSignalPublisher {
        self.publisher.clone()
    }
    fn check_health(&self) -> BlueprintResult<()> {
        match self.health.load(Ordering::Acquire) {
            0 => Ok(()),
            2 => Err(signal_error(
                "worker_signal_invalid_message",
                "Invalid distributed signal",
            )),
            3 => Err(signal_error(
                "worker_signal_overflow",
                "Distributed signal buffer overflow",
            )),
            _ => Err(signal_error(
                "worker_signal_disconnected",
                "Distributed signal subscription disconnected",
            )),
        }
    }
}
impl AsyncSignalBus for RedisSignalBus {
    async fn broadcast(&self, signal: DistributedSignal) -> BlueprintResult<usize> {
        self.check_health()?;
        self.publisher.broadcast(signal).await
    }
    async fn next_signal(&mut self, wait: Duration) -> BlueprintResult<Option<DistributedSignal>> {
        if wait > Duration::from_hours(24) {
            return Err(signal_error(
                "worker_signal_invalid_input",
                "Signal wait exceeds one day",
            ));
        }
        self.check_health()?;
        let received = tokio::time::timeout(wait, self.receiver.recv()).await;
        self.check_health()?;
        match received {
            Ok(Some(signal)) => Ok(Some(signal)),
            Err(_) => Ok(None),
            Ok(None) => Err(signal_error(
                "worker_signal_disconnected",
                "Distributed signal subscription disconnected",
            )),
        }
    }
    fn drain_signals(&mut self) -> BlueprintResult<Vec<DistributedSignal>> {
        self.check_health()?;
        let mut signals = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(signal) => signals.push(signal),
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.check_health()?;
                    return Err(signal_error(
                        "worker_signal_disconnected",
                        "Distributed signal subscription disconnected",
                    ));
                }
            }
        }
        self.check_health()?;
        Ok(signals)
    }
}
fn decode_signal(bytes: &[u8]) -> Option<DistributedSignal> {
    match bytes {
        b"pause" => Some(DistributedSignal::Pause),
        b"resume" => Some(DistributedSignal::Resume),
        b"drain" => Some(DistributedSignal::Drain),
        _ => None,
    }
}
fn channel_name(
    namespace: &str,
    tenant: &TenantContext,
    queue: &str,
    options: &SignalOptions,
) -> BlueprintResult<String> {
    if [namespace, queue].iter().any(|name| {
        name.trim().is_empty() || name.len() > 256 || name.chars().any(char::is_control)
    }) || options.capacity == 0
        || options.capacity > 65_536
        || options.operation_timeout.is_zero()
        || options.operation_timeout > Duration::from_secs(60)
    {
        return Err(signal_error(
            "worker_signal_invalid_input",
            "Invalid distributed signal configuration",
        ));
    }
    let encode = |name: &str| {
        name.bytes().fold(String::new(), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        })
    };
    Ok(format!(
        "fl_worker_signal:{{{}_{}}}:{}",
        encode(namespace),
        encode(tenant.id()),
        encode(queue)
    ))
}
fn signal_error(code: &str, message: &str) -> BlueprintError {
    BlueprintError::new(code, message)
}
fn redis_error(error: &redis::RedisError) -> BlueprintError {
    crate::tenant_isolation::isolation_error(error).unwrap_or_else(storage_error)
}

// redis 0.32's PubSubSink::subscribe discards a RESP ServerError payload.
// A short-lived command connection validates the real SUBSCRIBE acknowledgment
// with the same credentials before the long-lived PubSub client is accepted.
// This publishes no data and cannot change another worker's control state.
async fn verify_subscription(client: &redis::Client, channel: &str) -> BlueprintResult<()> {
    let mut connection = client
        .get_multiplexed_async_connection()
        .await
        .map_err(|error| redis_error(&error))?;
    let (kind, actual, count): (String, String, i64) = redis::cmd("SUBSCRIBE")
        .arg(channel)
        .query_async(&mut connection)
        .await
        .map_err(|error| redis_error(&error))?;
    if kind != "subscribe" || actual != channel || count != 1 {
        return Err(storage_error());
    }
    Ok(())
}
fn storage_error() -> BlueprintError {
    signal_error(
        "worker_signal_storage_error",
        "Distributed signal storage operation failed",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_worker_database::control::{WorkerControl, WorkerState, forward_signals};
    #[test]
    fn signal_channels_are_bounded_and_fence_tenant_namespace_and_queue() {
        let a = TenantContext::new("tenant-a").unwrap();
        let b = TenantContext::new("tenant-b").unwrap();
        let options = SignalOptions::default();
        let channel = channel_name("a:b", &a, "c", &options).unwrap();
        assert_ne!(channel, channel_name("a", &a, "b:c", &options).unwrap());
        assert_ne!(channel, channel_name("a:b", &b, "c", &options).unwrap());
        for options in [
            SignalOptions {
                capacity: 0,
                ..options.clone()
            },
            SignalOptions {
                capacity: 65_537,
                ..options.clone()
            },
            SignalOptions {
                operation_timeout: Duration::ZERO,
                ..options.clone()
            },
        ] {
            assert!(channel_name("ns", &a, "jobs", &options).is_err());
        }
        assert!(channel_name("ns", &a, "bad\nname", &SignalOptions::default()).is_err());
        assert_eq!(decode_signal(b"pause"), Some(DistributedSignal::Pause));
        assert!(decode_signal(b"restart").is_none());
    }
    #[tokio::test]
    async fn invalid_signal_configuration_fails_before_network_access() {
        assert_eq!(
            RedisSignalBus::connect(
                "redis://127.0.0.1:1",
                "ns",
                &TenantContext::new("tenant").unwrap(),
                "queue",
                SignalOptions {
                    capacity: 0,
                    ..SignalOptions::default()
                }
            )
            .await
            .unwrap_err()
            .code,
            "worker_signal_invalid_input"
        );
    }
    async fn bus(
        url: &str,
        namespace: &str,
        tenant: &str,
        queue: &str,
        capacity: usize,
    ) -> RedisSignalBus {
        RedisSignalBus::connect(
            url,
            namespace,
            &TenantContext::new(tenant).unwrap(),
            queue,
            SignalOptions {
                capacity,
                ..SignalOptions::default()
            },
        )
        .await
        .unwrap()
    }
    async fn wait_state(control: &WorkerControl, expected: WorkerState) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while control.state() != expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    #[ignore = "requires isolated Redis in FORKLAUNCH_TEST_REDIS_URL"]
    async fn redis_worker_signals_broadcast_order_isolation_controls_and_overflow() {
        let url = std::env::var("FORKLAUNCH_TEST_REDIS_URL").unwrap();
        let namespace = format!("signals-{}", uuid::Uuid::new_v4());
        let mut a = bus(&url, &namespace, "a", "jobs", 16).await;
        let mut b = bus(&url, &namespace, "a", "jobs", 16).await;
        let mut foreign_tenant = bus(&url, &namespace, "b", "jobs", 16).await;
        let mut foreign_queue = bus(&url, &namespace, "a", "other", 16).await;
        let mut foreign_namespace = bus(&url, &format!("{namespace}-other"), "a", "jobs", 16).await;
        let admin = RedisSignalPublisher::connect(
            &url,
            &namespace,
            &TenantContext::new("a").unwrap(),
            "jobs",
            &SignalOptions::default(),
        )
        .await
        .unwrap();
        assert!(!format!("{admin:?}").contains("redis://"));
        for signal in [
            DistributedSignal::Pause,
            DistributedSignal::Resume,
            DistributedSignal::Drain,
        ] {
            assert_eq!(admin.broadcast(signal).await.unwrap(), 2);
            assert_eq!(
                a.next_signal(Duration::from_secs(2)).await.unwrap(),
                Some(signal)
            );
            assert_eq!(
                b.next_signal(Duration::from_secs(2)).await.unwrap(),
                Some(signal)
            );
        }
        for foreign in [
            &mut foreign_tenant,
            &mut foreign_queue,
            &mut foreign_namespace,
        ] {
            assert!(
                foreign
                    .next_signal(Duration::from_millis(10))
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        assert!(a.drain_signals().unwrap().is_empty());
        let mut controlled = bus(&url, &namespace, "a", "controlled", 16).await;
        let publisher = controlled.publisher();
        let control = WorkerControl::new();
        let owned = control.clone();
        let task = tokio::spawn(async move {
            forward_signals(&mut controlled, &owned, std::future::pending()).await
        });
        publisher.broadcast(DistributedSignal::Pause).await.unwrap();
        wait_state(&control, WorkerState::Paused).await;
        publisher
            .broadcast(DistributedSignal::Resume)
            .await
            .unwrap();
        wait_state(&control, WorkerState::Running).await;
        publisher.broadcast(DistributedSignal::Drain).await.unwrap();
        task.await.unwrap().unwrap();
        assert_eq!(control.state(), WorkerState::Draining);
        assert!(!control.resume());
        assert_signal_faults(&url, &namespace).await;
        assert_cross_process_signals(&url, &namespace).await;
        drop(a);
        drop(b);
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if admin.broadcast(DistributedSignal::Pause).await.unwrap() == 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    struct ChildGuard(Option<std::process::Child>);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if let Some(child) = self.0.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    async fn assert_cross_process_signals(url: &str, namespace: &str) {
        use std::io::BufRead as _;
        let parent = bus(url, namespace, "a", "child", 16).await;
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "signals::tests::redis_worker_signal_child_process",
                "--nocapture",
            ])
            .env("FORKLAUNCH_SIGNAL_CHILD_NAMESPACE", namespace)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut child = ChildGuard(Some(child));
        let (ready, received) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut output = String::new();
            for line in std::io::BufReader::new(stdout).lines() {
                let line = line.unwrap();
                if line.ends_with("SIGNAL_CHILD_READY") {
                    let _ = ready.send(());
                }
                output.push_str(&line);
                output.push('\n');
            }
            output
        });
        tokio::task::spawn_blocking(move || received.recv_timeout(Duration::from_secs(10)))
            .await
            .unwrap()
            .expect("child failed to subscribe");
        for signal in [
            DistributedSignal::Pause,
            DistributedSignal::Resume,
            DistributedSignal::Drain,
        ] {
            assert_eq!(parent.broadcast(signal).await.unwrap(), 2);
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            while child.0.as_mut().unwrap().try_wait().unwrap().is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("child did not apply signals");
        let result = child.0.take().unwrap().wait_with_output().unwrap();
        let stdout = reader.join().unwrap();
        assert!(
            result.status.success(),
            "child failed: {stdout} {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(stdout.contains("SIGNAL_CHILD_DRAINED"));
    }
    #[tokio::test]
    #[ignore = "subprocess fixture for the isolated Redis signal test"]
    async fn redis_worker_signal_child_process() {
        use std::io::Write as _;
        let Ok(namespace) = std::env::var("FORKLAUNCH_SIGNAL_CHILD_NAMESPACE") else {
            return;
        };
        let url = std::env::var("FORKLAUNCH_TEST_REDIS_URL").unwrap();
        let mut child = bus(&url, &namespace, "a", "child", 16).await;
        let control = WorkerControl::new();
        println!("SIGNAL_CHILD_READY");
        std::io::stdout().flush().unwrap();
        for expected in [
            DistributedSignal::Pause,
            DistributedSignal::Resume,
            DistributedSignal::Drain,
        ] {
            let actual = child
                .next_signal(Duration::from_secs(5))
                .await
                .unwrap()
                .expect("missing signal");
            assert_eq!(actual, expected);
            assert!(control.apply_signal(actual));
        }
        assert_eq!(control.state(), WorkerState::Draining);
        println!("SIGNAL_CHILD_DRAINED");
    }

    async fn assert_signal_faults(url: &str, namespace: &str) {
        let mut overflow = bus(url, namespace, "a", "overflow", 1).await;
        let publisher = overflow.publisher();
        for _ in 0..4 {
            publisher.broadcast(DistributedSignal::Pause).await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            while overflow.health.load(Ordering::Acquire) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            overflow.drain_signals().unwrap_err().code,
            "worker_signal_overflow"
        );
        let mut malformed = bus(url, namespace, "a", "malformed", 16).await;
        let control = WorkerControl::new();
        let mut connection = malformed.publisher.connection.clone();
        let _: usize = redis::cmd("PUBLISH")
            .arg(&malformed.publisher.channel)
            .arg("invalid")
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            forward_signals(&mut malformed, &control, std::future::pending())
                .await
                .unwrap_err()
                .code,
            "worker_signal_invalid_message"
        );
        assert_eq!(control.state(), WorkerState::Draining);
        assert_network_disconnect(url, namespace).await;
    }
    async fn assert_network_disconnect(url: &str, namespace: &str) {
        let client = redis::Client::open(url).unwrap();
        let redis::ConnectionAddr::Tcp(host, port) = client.get_connection_info().addr() else {
            panic!("isolated fixture requires a TCP Redis endpoint");
        };
        let upstream = format!("{host}:{port}");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_url = format!("redis://{}/", listener.local_addr().unwrap());
        let (shutdown, mut stopped) = tokio::sync::watch::channel(false);
        let proxy = tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    _ = stopped.changed() => break,
                    accepted = listener.accept() => accepted,
                };
                let (mut incoming, _) = accepted.unwrap();
                let upstream = upstream.clone();
                let mut stopped = stopped.clone();
                tokio::spawn(async move {
                    let mut outgoing = tokio::net::TcpStream::connect(upstream).await.unwrap();
                    if *stopped.borrow() {
                        return;
                    }
                    tokio::select! {
                        _ = stopped.changed() => (),
                        _ = tokio::io::copy_bidirectional(&mut incoming, &mut outgoing) => (),
                    }
                });
            }
        });
        let mut disconnected = bus(&proxy_url, namespace, "a", "network-disconnect", 16).await;
        let control = WorkerControl::new();
        shutdown.send(true).unwrap();
        let error = tokio::time::timeout(
            Duration::from_secs(3),
            forward_signals(&mut disconnected, &control, std::future::pending()),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(error.code, "worker_signal_disconnected");
        assert_eq!(control.state(), WorkerState::Draining);
        proxy.await.unwrap();
    }
}
