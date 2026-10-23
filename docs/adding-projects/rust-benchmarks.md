# Rust runtime benchmarks

Run `framework-rs/parity/rust_benchmarks.mjs` from any working directory. It builds the
feature-gated `forklaunch-testing` example, starts a loopback service, generates
and compiles a real TypeScript SDK, checks every response, and writes a JSON
report. The default build is optimized (`--release`); `--debug` supports quick
functional smoke checks. Benchmark dependencies are disabled by default in the
library.

```sh
TSC_BIN=/path/to/typescript/bin/tsc \
FORKLAUNCH_BENCH_ITERATIONS=1000 \
FORKLAUNCH_BENCH_WARMUP=100 \
FORKLAUNCH_BENCH_OUTPUT=/tmp/rust-benchmarks.json \
CARGO_INCREMENTAL=0 node framework-rs/parity/rust_benchmarks.mjs
```

Use TypeScript 5.9.3, matching parity CI. Both sample counts must be positive
integers at most one million. Compilation, connection setup and warmup samples
are excluded from timing. Every measured request runs sequentially at concurrency
one. Latency values are nanoseconds; each record includes count, warmup count,
minimum, median, nearest-rank p95, maximum and total measured time. Reports also
record revision/dirty state, profile, payload size, compiler/Node versions and
host CPU/OS/memory. No credentials or endpoint URLs are included in the report.

| Measurement | Timed work |
| --- | --- |
| `json_parse_validate` | Deserialize and validate a typed JSON DTO with serde and garde |
| `http_get_json` | Reused reqwest client, real contract-driven Axum route, response body and typed JSON decode |
| `generated_typescript_sdk_http` | Generated native-service SDK through Node fetch, response decoding and SDK envelope |
| `redis_set_get` | Real synchronous Redis adapter, one SET and one GET on a reused connection |
| `s3_put_get` | Official AWS S3 adapter, one PUT and one GET with full body transfer |
| `websocket_json_roundtrip` | Reused typed native WebSocket, JSON message encoding, echo and decoding |

The DTO has one 1024-byte string; storage uses the same serialized JSON bytes.
Assertions execute after each sample's timer stops. Redis/S3 statistics represent
**pairs** of operations, not individual reads or writes. HTTP and SDK rows use
different client runtimes, so their difference is not an isolated SDK overhead
measurement. This suite does not measure concurrent saturation, TLS overhead,
authentication, encryption, batch APIs, streaming, cold starts or production
cloud latency. Debug smoke output establishes that the harness works; it is not
an optimized performance baseline. Compare repeated release runs on a controlled
host with matching versions, configuration and sample sizes.

## Redis and S3

Storage rows explicitly report `skipped` unless configured. Add `--require-infra`
to fail unless all six rows execute. Use a dedicated test Redis database and S3
bucket. The harness writes a process/time-qualified key, removes only that key
on success or an operation error, and gives Redis data a five-minute TTL. A hard
process kill can leave the S3 key; its prefix is `forklaunch-benchmark/`.

```sh
TSC_BIN=/path/to/typescript/bin/tsc \
FORKLAUNCH_BENCH_REDIS_URL=redis://127.0.0.1:6379/ \
FORKLAUNCH_BENCH_S3=1 \
S3_URL=http://127.0.0.1:9000 \
S3_REGION=us-east-1 \
S3_BUCKET=forklaunch-bench \
S3_ACCESS_KEY_ID=local-test-access \
S3_SECRET_ACCESS_KEY=local-test-secret \
FORKLAUNCH_BENCH_OUTPUT=/tmp/rust-benchmarks.json \
CARGO_INCREMENTAL=0 node framework-rs/parity/rust_benchmarks.mjs --require-infra
```

The ordinary S3 environment/default credential chain is supported. Set
`FORKLAUNCH_BENCH_CREATE_BUCKET=1` only when the harness should initialize its
configured local test bucket; otherwise provision the bucket beforehand. The
runner does not create or remove Docker containers. It removes its own temporary
SDK source/build files and terminates its own server on failure.

Parity CI executes all six rows with 20 samples after five warmups against
isolated Redis/MinIO and uploads `rust-benchmark-smoke.json`. It enforces successful
execution and response correctness, with no timing thresholds on shared runners.
