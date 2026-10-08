#!/usr/bin/env bash
# Run the built CLI in a container with --network none for the strongest proof.
set -euo pipefail
: "${FORKLAUNCH_CLI:?Set the absolute path to the reviewed built CLI}"
case "$FORKLAUNCH_CLI" in /*) ;; *) echo 'CLI path must be absolute' >&2; exit 1;; esac
LOCAL_PROOF_ROOT="$(mktemp -d)"
trap 'rm -rf "$LOCAL_PROOF_ROOT"' EXIT
mkdir -p "$LOCAL_PROOF_ROOT/home" "$LOCAL_PROOF_ROOT/project"
cd "$LOCAL_PROOF_ROOT/project"
env -i PATH="$PATH" HOME="$LOCAL_PROOF_ROOT/home" XDG_CONFIG_HOME="$LOCAL_PROOF_ROOT/home/config" XDG_DATA_HOME="$LOCAL_PROOF_ROOT/home/data" FORKLAUNCH_SKIP_FORMAT=1 "$FORKLAUNCH_CLI" init application offline-proof -p . -o src/modules -d postgresql -v zod -f biome -l oxlint -F express -r node -t vitest -m iam-better-auth -D 'Offline local template proof' -A 'ForkLaunch' -L MIT > "$LOCAL_PROOF_ROOT/init.log" 2>&1
[ -f .forklaunch/manifest.toml ]
[ -f src/modules/iam/package.json ]
env -i PATH="$PATH" HOME="$LOCAL_PROOF_ROOT/home" FORKLAUNCH_SKIP_FORMAT=1 "$FORKLAUNCH_CLI" init service records -p src/modules -d postgresql -D 'Local records API' > "$LOCAL_PROOF_ROOT/service.log" 2>&1
[ -f src/modules/records/package.json ]
# No fake token, keyring fixture, account selection, or credential is supplied.
[ ! -e "$LOCAL_PROOF_ROOT/home/.forklaunch/token" ]
[ ! -e "$LOCAL_PROOF_ROOT/home/.forklaunch/accounts" ]
echo 'PASS: local application and service initialize with empty HOME and no credentials.'
