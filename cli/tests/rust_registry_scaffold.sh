#!/usr/bin/env bash
# A generated app with the default framework source, built against the
# framework crates as packaged for release: mixed build hooks and the Docker
# image included. Slow; full runs only.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
export FORKLAUNCH_CLI="${FORKLAUNCH_CLI:-${FORKLAUNCH_TEST_CLI:-$REPO_ROOT/cli/target/debug/forklaunch}}"
PACKAGES="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/forklaunch-framework-packages"
cd "$REPO_ROOT"
python3 scripts/rust_release.py prepare --scope framework --allow-dirty --output "$PACKAGES"
exec python3 "$SCRIPT_DIR/fixtures/rust_registry_scaffold.py" --default-source --mixed --docker --release "$PACKAGES" "$@"
