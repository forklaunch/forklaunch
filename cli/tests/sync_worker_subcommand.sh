#!/usr/bin/env bash

# E2E test for sync worker subcommand
# Tests syncing a specific worker to all artifacts

set -e

if [ -d "output/sync-worker-subcommand" ]; then
    rm -rf output/sync-worker-subcommand
fi

mkdir -p output/sync-worker-subcommand
cd output/sync-worker-subcommand

echo "[TEST] sync worker subcommand"

# Create test application
RUST_BACKTRACE=1 cargo run --release init application test-app \
    -p test-app \
    -o src/modules \
    -d postgresql \
    -f prettier \
    -l eslint \
    -v zod \
    -F express \
    -r node \
    -t vitest \
    -D "Test application" \
    -A "Test Author" \
    -L 'MIT' \
    -m

cd test-app

# Initialize first worker
RUST_BACKTRACE=1 cargo run --release init worker email-sender \
    -t database \
    -d postgresql \
    -p src/modules \
    -D "Email sender worker"

# Manually create a second worker directory
mkdir -p src/modules/video-processor
cp -r src/modules/email-sender/* src/modules/video-processor/
sed -i.bak 's/"email-sender"/"video-processor"/g' src/modules/video-processor/package.json
rm -f src/modules/video-processor/package.json.bak

# Test 1: Worker directory exists but not in manifest
if [ -d "src/modules/video-processor" ]; then
    echo "[PASS] Worker directory created"
else
    echo "[FAIL] Worker directory not found"
    exit 1
fi

if ! grep -q "video-processor" .forklaunch/manifest.toml; then
    echo "[PASS] Worker not yet in manifest (expected)"
else
    echo "[FAIL] Worker should not be in manifest yet"
    exit 1
fi

# Test 2: Sync specific worker with prompts
PROMPTS_JSON='{"video-processor": {"type": "database", "database": "postgresql", "description": "Video processing worker"}}'

if RUST_BACKTRACE=1 cargo run --release sync worker video-processor -p . -P "$PROMPTS_JSON" > /dev/null 2>&1; then
    echo "[PASS] Sync worker command executed"
else
    echo "[FAIL] Sync worker command failed"
    exit 1
fi

# Test 3: Verify worker added to manifest
if grep -q "video-processor" .forklaunch/manifest.toml; then
    echo "[PASS] Worker added to manifest"
else
    echo "[FAIL] Worker NOT in manifest"
    cat .forklaunch/manifest.toml
    exit 1
fi

# Test 4: Verify worker added to docker-compose
if grep -q "video-processor" docker-compose.yaml; then
    echo "[PASS] Worker added to docker-compose"
else
    echo "[FAIL] Worker NOT in docker-compose"
    cat docker-compose.yaml
    exit 1
fi

# Test 5: Verify worker added to runtime files
if grep -q "video-processor" src/modules/pnpm-workspace.yaml; then
    echo "[PASS] Worker added to pnpm-workspace"
else
    echo "[FAIL] Worker NOT in pnpm-workspace"
    cat pnpm-workspace.yaml
    exit 1
fi

# Test 6: Idempotency check
if RUST_BACKTRACE=1 cargo run --release sync worker video-processor -p . -P "$PROMPTS_JSON" > /dev/null 2>&1; then
    echo "[PASS] Sync is idempotent"
else
    echo "[FAIL] Sync should be idempotent"
    exit 1
fi

# Test 7: Non-existent worker handling
if ! RUST_BACKTRACE=1 cargo run --release sync worker nonexistent -p . 2>&1 | grep -q "not found"; then
    echo "[FAIL] Should fail for non-existent worker"
    exit 1
else
    echo "[PASS] Fails gracefully for non-existent worker"
fi

echo "[SUCCESS] All sync worker tests passed"
