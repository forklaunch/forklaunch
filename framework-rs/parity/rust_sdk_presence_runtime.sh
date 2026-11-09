#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/cli/target}"
export CARGO_INCREMENTAL=0
export FORKLAUNCH_SKIP_FORMAT=1
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
if [ "${FORKLAUNCH_WORKER_RUNTIME_LOCKED:-0}" != 1 ]; then
    exec python3 "$SCRIPT_DIR/fixtures/worker_runtime_lock.py" "$SCRIPT_DIR/rust_sdk_presence_runtime.sh"
fi
export FORKLAUNCH_CLI="${FORKLAUNCH_CLI:-${FORKLAUNCH_TEST_CLI:-$CARGO_TARGET_DIR/debug/forklaunch}}"
python3 "$SCRIPT_DIR/rust_sdk_presence_runtime.py"
