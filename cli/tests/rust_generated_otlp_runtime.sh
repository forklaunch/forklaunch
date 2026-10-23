#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
export FORKLAUNCH_SKIP_FORMAT=1
export CARGO_INCREMENTAL=0
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
# Cargo output defaults to the target the golden apps compiled into.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(python3 "$SCRIPT_DIR/fixtures/golden.py" target)}"
if [ "${FORKLAUNCH_WORKER_RUNTIME_LOCKED:-}" != 1 ]; then
    exec python3 "$SCRIPT_DIR/fixtures/worker_runtime_lock.py" "${BASH_SOURCE[0]}" "$@"
fi
FORKLAUNCH_CLI="${FORKLAUNCH_CLI:-${FORKLAUNCH_TEST_CLI:-}}"
run_cli() {
    if [ -n "$FORKLAUNCH_CLI" ]; then "$FORKLAUNCH_CLI" "$@";
    else cargo run --manifest-path "$REPO_ROOT/cli/Cargo.toml" --release -- "$@"; fi
}
mkdir -p "$SCRIPT_DIR/output/rust-generated-otlp-runtime"
RUN_ROOT="$(mktemp -d "$SCRIPT_DIR/output/rust-generated-otlp-runtime/run.XXXXXX")"
cleanup() {
    if [ "${FORKLAUNCH_KEEP_FIXTURE:-0}" != 1 ]; then rm -rf "$RUN_ROOT"; fi
}
trap cleanup EXIT
# TypeScript shell plus Rust service accounts: the golden 'rust-service' scaffold.
source "$SCRIPT_DIR/fixtures/golden.sh"
golden_app --scaffold rust-service "$RUN_ROOT/app"
run_cli init worker starter --path "$RUN_ROOT/app" --language rust --consumer-only --type database --database postgresql -D Starter
cp -R "$RUN_ROOT/app" "$RUN_ROOT/relocated-app"
cd "$RUN_ROOT/relocated-app"
cargo build --workspace
# Invalid live telemetry settings must not break offline artifact generation.
OTEL_EXPORTER_OTLP_PROTOCOL=invalid OTEL_EXPORTER_OTLP_ENDPOINT=invalid run_cli openapi export --path .
BINARY="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"]+"/debug/accounts")')"
node "$SCRIPT_DIR/rust_generated_otlp_runtime.mjs" "$BINARY"
