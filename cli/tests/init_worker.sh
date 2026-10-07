# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-worker" ]; then
    rm -rf output/init-worker
fi

mkdir -p output/init-worker
cd output/init-worker

RUST_BACKTRACE=1 $FL init application worker-test-node-application -p worker-test-node-application -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m billing-base -m iam-base -D "Test worker" -A "Rohin Bhargava" -L 'AGPL-3.0'
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

RUST_BACKTRACE=1 $FL init application worker-test-bun-application -p worker-test-bun-application -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -m billing-base -m iam-base -D "Test worker" -A "Rohin Bhargava" -L "MIT"
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
