#!/bin/bash
set -e

if [ -d "output/migration-test" ]; then
    rm -rf output/migration-test
fi

mkdir -p output/migration-test
cd output/migration-test

echo "Initializing Application..."
RUST_BACKTRACE=1 cargo run --release -- init application migration-app \
  -p . \
  -o src/modules \
  -d postgresql \
  -f biome \
  -l eslint \
  -v zod \
  -F express \
  -r bun \
  -t vitest \
  -D "Migration App" \
  -A "Test Author" \
  -L "MIT" \
  -m

echo "Initializing Service..."
RUST_BACKTRACE=1 cargo run --release -- init service my-service \
  -p . \
  -d postgresql \
  -D "Original Service"

echo "Migrating Service to Worker..."
RUST_BACKTRACE=1 cargo run --release -- change service \
  --path src/modules/my-service \
  --to worker \
  --type bullmq \
  -c

if [ ! -f "src/modules/my-service/worker.ts" ]; then
  echo "Error: worker.ts missing after conversion"
  exit 1
fi

echo "Migrating Worker to Service..."
RUST_BACKTRACE=1 cargo run --release -- change worker \
  --path src/modules/my-service \
  --to service \
  -c

if ! grep -q "^// " "src/modules/my-service/worker.ts"; then
  echo "Error: worker.ts not commented out after reversion"
  exit 1
fi

echo "Verifying Build..."
cd src/modules/my-service
bun install
bun run build

echo "Test Passed!"
