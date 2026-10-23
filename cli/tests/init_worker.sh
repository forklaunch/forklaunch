# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e
# Golden apps: the canonical application this script starts from is scaffolded,
# installed and built once (fixtures/golden.py); see "Golden apps" in README.md.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/fixtures/golden.sh"

if [ -d "output/init-worker" ]; then
    rm -rf output/init-worker
fi

mkdir -p output/init-worker
cd output/init-worker

golden_app ts-node worker-test-node-application
RUST_BACKTRACE=1 $FL init worker wrk-test -t database -d postgresql -p worker-test-node-application/src/modules -D "Test worker"
RUST_BACKTRACE=1 $FL init worker wrk-test-redis -t redis -p worker-test-node-application/src/modules -D "Test worker"
RUST_BACKTRACE=1 $FL init worker wrk-test-bullmq -t bullmq -p worker-test-node-application/src/modules -D "Test worker"
RUST_BACKTRACE=1 $FL init worker wrk-test-kafka -t kafka -p worker-test-node-application/src/modules -D "Test worker"

cd worker-test-node-application

RUST_BACKTRACE=1 $FL delete worker wrk-test -c
RUST_BACKTRACE=1 $FL init worker wrk-test -t database -d postgresql -D "Test worker" -p .

cd src/modules

pnpm install
pnpm build

cd ../../..

golden_app ts-bun worker-test-bun-application
RUST_BACKTRACE=1 $FL init worker wrk-test -t database -d postgresql -p worker-test-bun-application/src/modules -D "Test worker"
RUST_BACKTRACE=1 $FL init worker wrk-test-redis -t redis -p worker-test-bun-application/src/modules -D "Test worker"
RUST_BACKTRACE=1 $FL init worker wrk-test-bullmq -t bullmq -p worker-test-bun-application/src/modules -D "Test worker"
RUST_BACKTRACE=1 $FL init worker wrk-test-kafka -t kafka -p worker-test-bun-application/src/modules -D "Test worker"

cd worker-test-bun-application

RUST_BACKTRACE=1 $FL delete worker wrk-test -c
RUST_BACKTRACE=1 $FL init worker wrk-test -t database -d postgresql -D "Test worker" -p .

cd src/modules

bun install
bun run build
