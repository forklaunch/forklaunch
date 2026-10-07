# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/depcheck" ]; then
    rm -rf output/depcheck
fi
mkdir -p output/depcheck
cd output/depcheck

RUST_BACKTRACE=1 $FL init application depcheck-test-node-application -p depcheck-test-node-application -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L 'AGPL-3.0'
RUST_BACKTRACE=1 $FL init library library-test -p depcheck-test-node-application -D "Test library"
RUST_BACKTRACE=1 $FL init service service-test -d postgresql -p depcheck-test-node-application -D "Test service"
RUST_BACKTRACE=1 $FL depcheck -p depcheck-test-node-application

RUST_BACKTRACE=1 $FL init application depcheck-test-bun-application -p depcheck-test-bun-application -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L "MIT"
RUST_BACKTRACE=1 $FL init library library-test -p depcheck-test-bun-application -D "Test library"
RUST_BACKTRACE=1 $FL init service service-test -d postgresql -p depcheck-test-bun-application -D "Test service"
RUST_BACKTRACE=1 $FL depcheck -p depcheck-test-bun-application
