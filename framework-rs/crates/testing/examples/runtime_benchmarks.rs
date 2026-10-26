//! Sequential local runtime benchmarks; driven by `framework-rs/parity/rust_benchmarks.mjs`.
use anyhow::{Context, Result, ensure};
use axum::{Json, extract::ws::WebSocketUpgrade, response::Response};
use forklaunch_axum::{ForkLaunchRouter, websocket};
use forklaunch_core::ServiceInfo;
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{
    BodyContract, ContractRegistry, FieldContract, RouteContract, SchemaRef,
};
use forklaunch_core::typescript::{TypeScriptHttpSdkOptions, generate_typescript_http_sdk_source};
use forklaunch_infrastructure_redis::{RedisTtlCacheConfig, TtlCache, client::RedisTtlCache};
use forklaunch_infrastructure_s3::{
    AsyncObjectStore, ObjectMetadata, aws::environment::S3Environment,
};
use forklaunch_ws::{
    codec::PayloadCodec,
    transport::{self, SocketEvent, SocketOptions, SocketSchemas},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    hint::black_box,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, garde::Validate,
)]
struct Payload {
    #[garde(length(min = 1, max = 4096))]
    id: String,
}
fn schemas() -> SocketSchemas<Payload, Payload> {
    SocketSchemas::new(PayloadCodec::serde(), PayloadCodec::serde())
}
async fn upgrade(ws: WebSocketUpgrade) -> Response {
    websocket::upgrade(
        ws,
        schemas(),
        SocketOptions::default(),
        |mut socket| async move {
            while let Ok(Some(SocketEvent::Message(payload))) = socket.recv().await {
                if socket.send(&payload).await.is_err() {
                    break;
                }
            }
        },
    )
    .expect("valid benchmark socket configuration")
}
struct Samples {
    name: &'static str,
    values: Vec<u64>,
    warmup: usize,
}
impl Samples {
    fn new(name: &'static str, warmup: usize) -> Self {
        Self {
            name,
            values: Vec::new(),
            warmup,
        }
    }
    fn record(&mut self, index: usize, elapsed: Duration) {
        if index >= self.warmup {
            self.values
                .push(u64::try_from(elapsed.as_nanos()).expect("bounded sample duration"));
        }
    }
    fn report(mut self) {
        self.values.sort_unstable();
        let count = self.values.len();
        let total: u64 = self.values.iter().sum();
        println!(
            "{}",
            json!({"benchmark":self.name,"samples":count,"warmup":self.warmup,"total_ns":total,"min_ns":self.values[0],"p50_ns":self.values[(count-1)/2],"p95_ns":self.values[(count*95).div_ceil(100)-1],"max_ns":self.values[count-1]})
        );
    }
}
fn setting(name: &str, default: usize) -> Result<usize> {
    let value = std::env::var(name).map_or(Ok(default), |value| value.parse::<usize>())?;
    ensure!(
        value > 0 && value <= 1_000_000,
        "{name} must be between 1 and 1000000"
    );
    Ok(value)
}
fn redis_benchmark(payload: &[u8], iterations: usize, warmup: usize, key: &str) -> Result<()> {
    let Ok(url) = std::env::var("FORKLAUNCH_BENCH_REDIS_URL") else {
        println!(
            "{}",
            json!({"benchmark":"redis_set_get","skipped":"FORKLAUNCH_BENCH_REDIS_URL not set"})
        );
        return Ok(());
    };
    let mut cache =
        RedisTtlCache::connect(RedisTtlCacheConfig::new(url, Duration::from_secs(300)))?;
    let result = (|| -> Result<()> {
        let mut samples = Samples::new("redis_set_get", warmup);
        for index in 0..iterations + warmup {
            let start = Instant::now();
            cache.set(key, payload.to_vec(), None)?;
            let value = cache.get(key)?;
            samples.record(index, start.elapsed());
            ensure!(
                value.as_deref() == Some(payload),
                "Redis round trip mismatch"
            );
        }
        samples.report();
        Ok(())
    })();
    let cleanup = cache.delete(key);
    result?;
    cleanup?;
    Ok(())
}
async fn s3_benchmark(payload: &[u8], iterations: usize, warmup: usize, key: &str) -> Result<()> {
    if std::env::var("FORKLAUNCH_BENCH_S3").as_deref() != Ok("1") {
        println!(
            "{}",
            json!({"benchmark":"s3_put_get","skipped":"FORKLAUNCH_BENCH_S3 not enabled"})
        );
        return Ok(());
    }
    let store = S3Environment::from_env()?.into_store().await?;
    // Only an explicitly requested local fixture may create its configured bucket.
    if std::env::var("FORKLAUNCH_BENCH_CREATE_BUCKET").as_deref() == Ok("1") {
        store.init_bucket(true).await?;
    }
    let result = async {
        let mut samples = Samples::new("s3_put_get", warmup);
        for index in 0..iterations + warmup {
            let start = Instant::now();
            store
                .put(key, payload.to_vec(), ObjectMetadata::new())
                .await?;
            let value = store.get(key).await?;
            samples.record(index, start.elapsed());
            ensure!(
                value.as_ref().map(|object| object.body.as_slice()) == Some(payload),
                "S3 round trip mismatch"
            );
        }
        samples.report();
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let cleanup = store.delete(key).await;
    result?;
    cleanup?;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    let iterations = setting("FORKLAUNCH_BENCH_ITERATIONS", 1000)?;
    let warmup = setting("FORKLAUNCH_BENCH_WARMUP", 100)?;
    let output = std::env::args()
        .nth(1)
        .context("SDK output path required")?;
    let payload = Payload {
        id: "x".repeat(1024),
    };
    let bytes = serde_json::to_vec(&payload)?;
    let mut samples = Samples::new("json_parse_validate", warmup);
    for index in 0..iterations + warmup {
        let start = Instant::now();
        let decoded = forklaunch_validator::parse_and_validate::<Payload>(black_box(&bytes))
            .map_err(|errors| anyhow::anyhow!("{errors:?}"))?;
        samples.record(index, start.elapsed());
        ensure!(decoded == payload, "validation round trip mismatch");
        black_box(decoded);
    }
    samples.report();
    let key = format!(
        "forklaunch-benchmark/{}/{}/record",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    redis_benchmark(&bytes, iterations, warmup, &key)?;
    s3_benchmark(&bytes, iterations, warmup, &key).await?;
    let body = BodyContract {
        schema: SchemaRef::named("Payload")
            .with_json_schema(serde_json::to_string(&schemars::schema_for!(Payload))?),
        ..BodyContract::json("Payload")
    };
    let contract = RouteContract::get("Read Presence", "Benchmark payload", "/presence/:index")
        .with_param(FieldContract::required("index", "String"))
        .with_response(200, Some(body));
    let mut registry = ContractRegistry::new();
    registry.register(contract.clone());
    std::fs::write(
        output,
        generate_typescript_http_sdk_source(
            &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
            &TypeScriptHttpSdkOptions::new("presence"),
        )?,
    )?;
    let reply = payload.clone();
    let router = ForkLaunchRouter::new(ServiceInfo::new("benchmark", "0.1.0"))
        .get(contract, move || {
            let reply = reply.clone();
            async move { Json(reply) }
        })
        .into_router()
        .route("/socket", axum::routing::get(upgrade));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let url = format!("http://{address}/presence/one");
    let mut samples = Samples::new("http_get_json", warmup);
    for index in 0..iterations + warmup {
        let start = Instant::now();
        let response = client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json::<Payload>()
            .await?;
        samples.record(index, start.elapsed());
        ensure!(response == payload, "HTTP round trip mismatch");
    }
    samples.report();
    let (mut socket, _) = transport::connect_with_response(
        format!("ws://{address}/socket"),
        schemas(),
        SocketOptions::default(),
    )
    .await?;
    let mut samples = Samples::new("websocket_json_roundtrip", warmup);
    for index in 0..iterations + warmup {
        let start = Instant::now();
        socket.send(&payload).await?;
        let response = tokio::time::timeout(Duration::from_secs(10), socket.recv()).await??;
        samples.record(index, start.elapsed());
        ensure!(
            response == Some(SocketEvent::Message(payload.clone())),
            "WebSocket round trip mismatch"
        );
    }
    samples.report();
    socket.close(1000, None).await?;
    println!(
        "{}",
        json!({"ready":format!("http://{address}"),"payload_bytes":bytes.len(),"profile":if cfg!(debug_assertions){"debug"}else{"release"}})
    );
    tokio::task::spawn_blocking(|| std::io::stdin().read_line(&mut String::new())).await??;
    server.abort();
    let _ = server.await;
    Ok(())
}
