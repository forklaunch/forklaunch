# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/change-worker" ]; then
    rm -rf output/change-worker
fi

mkdir -p output/change-worker
cd output/change-worker

RUST_BACKTRACE=1 $FL init application change-worker-test-node-application -p . -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r bun -t vitest -m billing-base -m iam-base -D "Test worker" -A "Rohin Bhargava" -L 'AGPL-3.0'
RUST_BACKTRACE=1 $FL init worker workr -t database -d postgresql -p . -D "Test worker"

cd src/modules

RUST_BACKTRACE=1 $FL change worker -p workr -N newworkr -t bullmq -D "Test worker 2" -c

bun install
bun run build