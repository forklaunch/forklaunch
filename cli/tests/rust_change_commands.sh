#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
if [ -z "${FORKLAUNCH_CLI:-}" ]; then
    cargo build --manifest-path "$REPO_ROOT/cli/Cargo.toml" --release
    export FORKLAUNCH_CLI="$REPO_ROOT/cli/target/release/forklaunch"
fi
exec python3 "$SCRIPT_DIR/fixtures/rust_change_preservation.py"
