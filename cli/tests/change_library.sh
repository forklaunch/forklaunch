# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/change-library" ]; then
    rm -rf output/change-library
fi

mkdir -p output/change-library
cd output/change-library

RUST_BACKTRACE=1 $FL init application change-library-test-node-application -p . -o src/modules -d postgresql -f prettier -l eslint -v zod -F hyper-express -r bun -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L 'AGPL-3.0'
RUST_BACKTRACE=1 $FL init library lbry -p . -D "Test library"

cd src/modules

RUST_BACKTRACE=1 $FL change library -p lbry -N newlbry -D "Test library 2" -c

bun install
bun run build