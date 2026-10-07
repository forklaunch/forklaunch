# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-worker-workspace" ]; then
    rm -rf output/init-worker-workspace
fi

mkdir -p output/init-worker-workspace
cd output/init-worker-workspace

RUST_BACKTRACE=1 $FL init application workspace-test-app -p workspace-test-app -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -D "Workspace integrity test application" -A "Forklaunch Team" -L 'MIT'

WORKSPACE_YAML="workspace-test-app/src/modules/pnpm-workspace.yaml"

# A fresh scaffold must seed real allowBuilds values, not pnpm placeholders
if grep -q "set this to true or false" "$WORKSPACE_YAML"; then
    echo "[ERROR] pnpm-workspace.yaml contains literal placeholder values"
    exit 1
fi
if ! grep -q "esbuild: true" "$WORKSPACE_YAML"; then
    echo "[ERROR] pnpm-workspace.yaml missing allowBuilds entry for esbuild"
    exit 1
fi

RUST_BACKTRACE=1 $FL init worker queue-worker -t bullmq -p workspace-test-app/src/modules -D "BullMQ worker"

# Re-emitting the workspace file must preserve allowBuilds
if ! grep -q "esbuild: true" "$WORKSPACE_YAML"; then
    echo "[ERROR] init worker clobbered allowBuilds in pnpm-workspace.yaml"
    exit 1
fi

# Simulate a workspace poisoned by pnpm placeholders plus consumer-added
# config; the next init must heal the placeholders and keep unknown keys
cat > "$WORKSPACE_YAML" << 'EOF'
packages:
- core
- monitoring
- client-sdk
- queue-worker
allowBuilds:
  esbuild: set this to true or false
  protobufjs: set this to true or false
blockExoticSubdeps: false
EOF

RUST_BACKTRACE=1 $FL init service checkout -d postgresql -p workspace-test-app/src/modules -D "Checkout service"

if grep -q "set this to true or false" "$WORKSPACE_YAML"; then
    echo "[ERROR] init service did not heal placeholder allowBuilds values"
    exit 1
fi
if ! grep -q "blockExoticSubdeps: false" "$WORKSPACE_YAML"; then
    echo "[ERROR] init service clobbered unmodeled workspace configuration"
    exit 1
fi

cd workspace-test-app/src/modules

# A fresh scaffold must install and typecheck with zero manual patches
CI=true pnpm install

for project in core queue-worker checkout; do
    (cd "$project" && pnpm exec tsc --noEmit) || {
        echo "[ERROR] tsc --noEmit failed for $project"
        exit 1
    }
done

pnpm build
