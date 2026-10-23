#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
export FORKLAUNCH_SKIP_FORMAT=1
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
FORKLAUNCH_CLI="${FORKLAUNCH_CLI:-${FORKLAUNCH_TEST_CLI:-}}"
run_cli() {
    if [ -n "$FORKLAUNCH_CLI" ]; then
        "$FORKLAUNCH_CLI" "$@"
    else
        cargo run --manifest-path "$REPO_ROOT/cli/Cargo.toml" --release -- "$@"
    fi
}

cleanup() {
    if [ -n "${RUN_ROOT:-}" ] && [ "${FORKLAUNCH_KEEP_FIXTURE:-0}" != 1 ]; then rm -rf "$RUN_ROOT"; fi
}
trap cleanup EXIT
# Allow an already generated fixture for local retries; CI generates its own.
if [ -n "${FORKLAUNCH_RUST_CONTAINER_APP:-}" ]; then
    APP_ROOT="$(cd "$FORKLAUNCH_RUST_CONTAINER_APP" && pwd)"
else
    mkdir -p "$SCRIPT_DIR/output/rust-container-runtime"
    RUN_ROOT="$(mktemp -d "$SCRIPT_DIR/output/rust-container-runtime/run.XXXXXX")"
    # TypeScript shell plus Rust service accounts: the golden 'rust-service' scaffold.
    source "$SCRIPT_DIR/fixtures/golden.sh"
    golden_app --scaffold rust-service "$RUN_ROOT/app"
    cp -R "$RUN_ROOT/app" "$RUN_ROOT/relocated-app"
    APP_ROOT="$RUN_ROOT/relocated-app"
fi

NATIVE_SERVICE="${FORKLAUNCH_RUST_CONTAINER_SERVICE:-accounts}"
cargo generate-lockfile --manifest-path "$APP_ROOT/Cargo.toml"
node "$SCRIPT_DIR/rust_registry_container.mjs" "$APP_ROOT" "$NATIVE_SERVICE"
echo 'PASS relocated Rust shared image, health/OpenAPI/MCP, non-root process and SIGTERM shutdown'
