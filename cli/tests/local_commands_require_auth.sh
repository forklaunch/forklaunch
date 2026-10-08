#!/usr/bin/env bash
set -euo pipefail
: "${FORKLAUNCH_CLI:?Set the absolute path to the newly built CLI}"
case "$FORKLAUNCH_CLI" in /*) ;; *) echo 'CLI path must be absolute' >&2; exit 1;; esac
AUTH_PROOF_ROOT="$(mktemp -d)"
trap 'rm -rf "$AUTH_PROOF_ROOT"' EXIT
mkdir -p "$AUTH_PROOF_ROOT/home" "$AUTH_PROOF_ROOT/project"
cd "$AUTH_PROOF_ROOT/project"
expect_login_failure() {
  if [ "$state" = expired ]; then
    mkdir -p "$AUTH_PROOF_ROOT/home/.forklaunch"
    # Expired, deliberately invalid credential: no real account is accessed.
    printf 'access_token = "expired-test"\nrefresh_token = ""\nexpires_at = 1\n' > "$AUTH_PROOF_ROOT/home/.forklaunch/token"
  fi
  if env -i PATH="$PATH" HOME="$AUTH_PROOF_ROOT/home" XDG_CONFIG_HOME="$AUTH_PROOF_ROOT/home/config" XDG_DATA_HOME="$AUTH_PROOF_ROOT/home/data" FORKLAUNCH_HMAC_SECRET=regression-test-not-a-credential "$FORKLAUNCH_CLI" "$@" > "$AUTH_PROOF_ROOT/command.log" 2>&1; then
    echo "FAIL: unauthenticated command succeeded: $*" >&2; exit 1
  fi
  if ! grep -Eiq 'login|expired|authenticate' "$AUTH_PROOF_ROOT/command.log"; then
    cat "$AUTH_PROOF_ROOT/command.log" >&2; exit 1
  fi
  [ -z "$(ls -A .)" ]
}
for state in missing expired; do
  for command in init add; do
    for kind in application service worker library module router; do
      expect_login_failure "$command" "$kind" auth-proof
    done
  done
  expect_login_failure release create --version 1.0.0 --prepare-only --application-id 11111111-1111-4111-8111-111111111111 --local --skip-sync --skip-package-build --yes

done
echo 'PASS: scaffold commands and local release preparation require user login; no files created.'
