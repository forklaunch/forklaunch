# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/delete-router" ]; then
    rm -rf output/delete-router
fi

mkdir -p output/delete-router
cd output/delete-router

RUST_BACKTRACE=1 $FL init application router-test-node-application -p router-test-node-application -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L 'AGPL-3.0'
RUST_BACKTRACE=1 $FL init router rtr-test -p router-test-node-application/src/modules/billing
RUST_BACKTRACE=1 $FL delete router rtr-test -p router-test-node-application/src/modules/billing -c

cd router-test-node-application/src/modules

pnpm install
pnpm build

cd ../../..

RUST_BACKTRACE=1 $FL init application router-test-bun-application -p router-test-bun-application -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L "MIT"
RUST_BACKTRACE=1 $FL init router rtr-test -p router-test-bun-application/src/modules/iam
RUST_BACKTRACE=1 $FL delete router rtr-test -p router-test-bun-application/src/modules/iam -c

cd router-test-bun-application/src/modules

bun install
bun run build

cd ../../..
