# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e
# Golden apps: the canonical application this script starts from is scaffolded,
# installed and built once (fixtures/golden.py); see "Golden apps" in README.md.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/fixtures/golden.sh"

if [ -d "output/init-service" ]; then
    rm -rf output/init-service
fi

mkdir -p output/init-service
cd output/init-service

golden_app ts-node service-test-node-application
RUST_BACKTRACE=1 $FL init service svc-test -d postgresql -p service-test-node-application/src/modules -D "Test service"
RUST_BACKTRACE=1 $FL init service svc-test-mongodb -d mongodb -p service-test-node-application/src/modules -D "Test service"

cd service-test-node-application/src/modules
RUST_BACKTRACE=1 $FL delete service svc-test -c

RUST_BACKTRACE=1 $FL init service svc-test -d postgresql -D "Test service" -p .

pnpm install
pnpm build

cd ../../..

golden_app ts-bun service-test-bun-application
RUST_BACKTRACE=1 $FL init service svc-test -d postgresql -p service-test-bun-application/src/modules -D "Test service"
RUST_BACKTRACE=1 $FL init service svc-test-mongodb -d mongodb -p service-test-bun-application/src/modules -D "Test service"

cd service-test-bun-application/src/modules
RUST_BACKTRACE=1 $FL delete service svc-test -c

RUST_BACKTRACE=1 $FL init service svc-test -d postgresql -D "Test service" -p .

bun install
bun run build
