#!/usr/bin/env bash
# Verifies `forklaunch release create --yes` does not fail with
# "IO error: not a terminal" when stdin is not a TTY (e.g. deployment workers).
#
# Regression test for: workers invoking the CLI with --yes hit the interactive
# mode-selection Select::interact() prompt and fail with "not a terminal".
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_DIR="$(dirname "$SCRIPT_DIR")"

cd "$CLI_DIR"
cargo build --quiet 2>&1 | tail -3
BIN="$CLI_DIR/target/debug/forklaunch"

# Pick an ephemeral port and start a minimal HTTP mock for platform-management.
MOCK_PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); p=s.getsockname()[1]; s.close(); print(p)')
MOCK_LOG="$(mktemp)"
python3 - "$MOCK_PORT" >"$MOCK_LOG" 2>&1 <<'PY' &
import http.server, json, socketserver, sys
PORT = int(sys.argv[1])
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        # 404 on release-version existence check => "release does not yet exist"
        self.send_response(404); self.end_headers()
    def do_POST(self):
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps({
            "uploadUrl": "http://127.0.0.1:1/upload",
            "releaseId": "rel_test",
        }).encode())
    def log_message(self, *a, **k): pass
socketserver.TCPServer(("127.0.0.1", PORT), H).serve_forever()
PY
MOCK_PID=$!

TMPDIR=$(mktemp -d)
cleanup() {
    kill "$MOCK_PID" 2>/dev/null || true
    rm -rf "$TMPDIR" "$MOCK_LOG"
}
trap cleanup EXIT

# Give the mock server a moment to bind.
for _ in 1 2 3 4 5; do
    if python3 -c "import socket,sys; s=socket.socket(); s.settimeout(0.2); sys.exit(0 if s.connect_ex(('127.0.0.1', $MOCK_PORT))==0 else 1)"; then
        break
    fi
    sleep 0.2
done

cd "$TMPDIR"
mkdir -p .forklaunch src/modules

cat > .forklaunch/manifest.toml <<'TOML'
id = "00000000-0000-0000-0000-000000000000"
cli_version = "0.0.0"
app_name = "non-tty-test"
modules_path = "src/modules"
app_description = "Non-TTY test"
linter = "eslint"
formatter = "prettier"
validator = "zod"
http_framework = "express"
runtime = "node"
test_framework = "vitest"
author = "Test"
license = "AGPL-3.0"
platform_application_id = "00000000-0000-0000-0000-000000000001"
projects = []
project_peer_topology = {}
TOML

# Invoke `release create --yes` with stdin redirected from /dev/null.
# Workers authenticate with HMAC; setting FORKLAUNCH_HMAC_SECRET matches that path.
# `--skip-sync` avoids running sync_all_projects (which needs a full project).
# The command is expected to fail later (no real pnpm project), but it must NOT
# fail with "IO error: not a terminal" — that would mean the Select prompt
# still runs despite --yes.
set +e
OUTPUT=$(
    FORKLAUNCH_HMAC_SECRET="test-secret" \
    FORKLAUNCH_HMAC_KEY_ID="test-key" \
    FORKLAUNCH_PLATFORM_MANAGEMENT_API_URL="http://127.0.0.1:$MOCK_PORT" \
    "$BIN" release create --version test-version --yes --skip-sync </dev/null 2>&1
)
STATUS=$?
set -e

echo "=== CLI exit: $STATUS ==="
echo "$OUTPUT"
echo "=== end output ==="

if echo "$OUTPUT" | grep -qi "not a terminal"; then
    echo "FAIL: CLI failed with 'not a terminal' — --yes did not suppress the mode-selection prompt"
    exit 1
fi

if echo "$OUTPUT" | grep -qi "IO error"; then
    echo "FAIL: CLI hit an interactive IO error — --yes did not suppress an interactive prompt"
    exit 1
fi

echo "OK: --yes successfully bypasses the mode-selection prompt in non-TTY mode"
