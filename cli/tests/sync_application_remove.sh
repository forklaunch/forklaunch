#!/usr/bin/env bash

# Integration test for sync command - orphan removal flow
# Tests that sync correctly removes orphaned projects from artifacts

set -e

if [ -d "output/sync-application-remove" ]; then
    rm -rf output/sync-application-remove
fi

mkdir -p output/sync-application-remove
cd output/sync-application-remove

echo "[TEST] Sync Orphan Removal Flow"

# Test with Node runtime
echo "[INFO] Creating Node application"
RUST_BACKTRACE=1 cargo run --release init application sync-test-node-application \
    -p sync-test-node-application \
    -o src/modules \
    -d postgresql \
    -f prettier \
    -l eslint \
    -v zod \
    -F express \
    -r node \
    -t vitest \
    -D "Test Sync Application" \
    -A "Test Author" \
    -L 'MIT' \
    -m

cd sync-test-node-application

RUST_BACKTRACE=1 cargo run --release init service svc-test \
    -d postgresql \
    -p src/modules \
    -D "Test service"

RUST_BACKTRACE=1 cargo run --release init worker wrk-test \
    -t database \
    -d postgresql \
    -p src/modules \
    -D "Test worker"

RUST_BACKTRACE=1 cargo run --release init library lib-test \
    -p src/modules \
    -D "Test library"

# Remove directories to create orphans
echo "[INFO] Removing project directories to create orphans"
rm -rf src/modules/svc-test src/modules/wrk-test src/modules/lib-test

# Sync should detect and remove orphans
echo "[INFO] Running sync all to clean up orphans"
RUST_BACKTRACE=1 cargo run --release sync all -p . -c

# Verify build still works
cd src/modules
pnpm install
pnpm build

cd ../../../..

# Test with Bun runtime
echo "[INFO] Creating Bun application"
RUST_BACKTRACE=1 cargo run --release init application sync-test-bun-application \
    -p sync-test-bun-application \
    -o src/modules \
    -d postgresql \
    -f biome \
    -l oxlint \
    -v zod \
    -F express \
    -r bun \
    -t vitest \
    -D "Test Sync Application" \
    -A "Test Author" \
    -L 'MIT' \
    -m

cd sync-test-bun-application

RUST_BACKTRACE=1 cargo run --release init service svc-test \
    -d postgresql \
    -p src/modules \
    -D "Test service"

RUST_BACKTRACE=1 cargo run --release init worker wrk-test \
    -t database \
    -d postgresql \
    -p src/modules \
    -D "Test worker"

RUST_BACKTRACE=1 cargo run --release init library lib-test \
    -p src/modules \
    -D "Test library"

# Remove directories to create orphans
echo "[INFO] Removing project directories to create orphans"
rm -rf src/modules/svc-test src/modules/wrk-test src/modules/lib-test

# Sync should detect and remove orphans
echo "[INFO] Running sync all to clean up orphans"
RUST_BACKTRACE=1 cargo run --release sync all -p . -c

# Verify build still works
cd src/modules
bun install
bun run build

echo "[SUCCESS] Orphan removal test completed"
