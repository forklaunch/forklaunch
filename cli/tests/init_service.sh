# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-service" ]; then
    rm -rf output/init-service
fi

mkdir -p output/init-service
cd output/init-service

RUST_BACKTRACE=1 $FL init application service-test-node-application -p service-test-node-application -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L 'AGPL-3.0'
RUST_BACKTRACE=1 $FL init service svc-test -d postgresql -p service-test-node-application/src/modules -D "Test service"
RUST_BACKTRACE=1 $FL init service svc-test-mongodb -d mongodb -p service-test-node-application/src/modules -D "Test service"

cd service-test-node-application/src/modules
RUST_BACKTRACE=1 $FL delete service svc-test -c

RUST_BACKTRACE=1 $FL init service svc-test -d postgresql -D "Test service" -p .

pnpm install
pnpm build

cd ../../..

RUST_BACKTRACE=1 $FL init application service-test-bun-application -p service-test-bun-application -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L "MIT"
RUST_BACKTRACE=1 $FL init service svc-test -d postgresql -p service-test-bun-application/src/modules -D "Test service"
RUST_BACKTRACE=1 $FL init service svc-test-mongodb -d mongodb -p service-test-bun-application/src/modules -D "Test service"

cd service-test-bun-application/src/modules
RUST_BACKTRACE=1 $FL delete service svc-test -c

RUST_BACKTRACE=1 $FL init service svc-test -d postgresql -D "Test service" -p .

bun install
bun run build
