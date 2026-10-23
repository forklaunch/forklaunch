#!/usr/bin/env bash
# Golden applications: scaffold the canonical apps once with the CLI under
# test, build them, and package or unpack them as CI artifacts. See the
# "Golden apps" section of cli/tests/README.md and fixtures/golden.py.
#
#   bash cli/tests/golden_apps.sh build [APP|BUNDLE ...]   # default: everything
#   bash cli/tests/golden_apps.sh package --out DIR [ts] [rust]
#   bash cli/tests/golden_apps.sh unpack ARCHIVE...
#   bash cli/tests/golden_apps.sh list | path APP | copy APP DEST | target
#
# Reads FORKLAUNCH_CLI (the CLI binary) and FORKLAUNCH_GOLDEN_APPS_DIR (where
# the apps live; default cli/tests/output/golden).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export RUSTUP_TOOLCHAIN=stable
exec python3 "$SCRIPT_DIR/fixtures/golden.py" "$@"
