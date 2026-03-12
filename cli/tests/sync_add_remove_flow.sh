#!/usr/bin/env bash

# Integration test for sync command - complete add/remove flow
# Tests that sync correctly detects and synchronizes changes

set -e

if [ -d "output/sync-add-remove-flow" ]; then
    rm -rf output/sync-add-remove-flow
fi

mkdir -p output/sync-add-remove-flow
cd output/sync-add-remove-flow

echo "[TEST] Sync Complete Add/Remove Flow"

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

RUST_BACKTRACE=1 cargo run --release init service users \
    -p src/modules \
    -d postgresql \
    -D "Users service"

echo "[INFO] Test 1: Sync with no changes"
if RUST_BACKTRACE=1 cargo run --release sync all -p . -c; then
    echo "[PASS] Sync with no changes"
else
    echo "[FAIL] Sync failed with no changes"
    exit 1
fi

echo "[INFO] Test 2: Creating new service manually"
mkdir -p src/modules/products
cp -r src/modules/users/* src/modules/products/
sed -i.bak 's/"users"/"products"/g' src/modules/products/package.json
sed -i.bak 's/"description": "Users service"/"description": "Products service"/g' src/modules/products/package.json
rm -f src/modules/products/package.json.bak

echo "[INFO] Test 3: Syncing new service"
PROMPTS_JSON='{"products": {"category": "service", "database": "postgresql", "infrastructure": "none", "description": "Products service"}}'

if RUST_BACKTRACE=1 cargo run --release sync all -p . -c -P "$PROMPTS_JSON"; then
    echo "[PASS] Sync added new service"
else
    echo "[FAIL] Sync failed to add service"
    exit 1
fi

if grep -q "products" .forklaunch/manifest.toml; then
    echo "[PASS] Service in manifest"
else
    echo "[FAIL] Service NOT in manifest"
    exit 1
fi

if grep -q "products" docker-compose.yaml; then
    echo "[PASS] Service in docker-compose"
else
    echo "[FAIL] Service NOT in docker-compose"
    exit 1
fi

echo "[INFO] Test 6: Removing service directory"
rm -rf src/modules/products

echo "[INFO] Test 7: Syncing removal"
if RUST_BACKTRACE=1 cargo run --release sync all -p . -c; then
    echo "[PASS] Sync removed service"
else
    echo "[FAIL] Sync failed to remove"
    exit 1
fi

if ! grep -q "products" .forklaunch/manifest.toml; then
    echo "[PASS] Service removed from manifest"
else
    echo "[FAIL] Service still in manifest"
    exit 1
fi

echo "[SUCCESS] Complete sync workflow validated"
