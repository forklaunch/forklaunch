#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# The CLI default stays the checkout's build; Cargo output defaults to the
# target the golden apps compiled their dependencies into.
export FORKLAUNCH_CLI="${FORKLAUNCH_CLI:-${FORKLAUNCH_TEST_CLI:-${CARGO_TARGET_DIR:-$REPO_ROOT/cli/target}/debug/forklaunch}}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(python3 "$SCRIPT_DIR/fixtures/golden.py" target)}"
export CARGO_INCREMENTAL=0
export FORKLAUNCH_SKIP_FORMAT=1
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
if [ "${FORKLAUNCH_WORKER_RUNTIME_LOCKED:-0}" != 1 ]; then
    exec python3 "$SCRIPT_DIR/fixtures/worker_runtime_lock.py" "$SCRIPT_DIR/rust_http_runtime.sh"
fi
export FORKLAUNCH_CLI="${FORKLAUNCH_CLI:-${FORKLAUNCH_TEST_CLI:-$CARGO_TARGET_DIR/debug/forklaunch}}"
python3 "$SCRIPT_DIR/rust_http_runtime.py" "$REPO_ROOT"
