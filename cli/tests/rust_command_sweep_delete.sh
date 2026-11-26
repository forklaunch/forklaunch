#!/usr/bin/env bash
# The delete half of the Rust command sweep (fixtures/rust_command_sweep.py).
set -euo pipefail
exec bash "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/rust_command_sweep.sh" --group delete "$@"
