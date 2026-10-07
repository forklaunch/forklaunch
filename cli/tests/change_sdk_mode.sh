# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/change-sdk-mode" ]; then
    rm -rf output/change-sdk-mode
fi

mkdir -p output/change-sdk-mode
cd output/change-sdk-mode

RUST_BACKTRACE=1 $FL init application service-test-node-application -p service-test-node-application -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L 'AGPL-3.0'

cd service-test-node-application/src/modules

pnpm install

pnpm build

RUST_BACKTRACE=1 $FL sdk mode -t generated

pnpm build

RUST_BACKTRACE=1 $FL sdk mode -t live

pnpm build

cd ../../..

RUST_BACKTRACE=1 $FL init application service-test-bun-application -p service-test-bun-application -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L "MIT"

cd service-test-bun-application/src/modules

bun install

bun run build

RUST_BACKTRACE=1 $FL sdk mode -t generated

bun run build

RUST_BACKTRACE=1 $FL sdk mode -t live

bun run build

cd ../../..
