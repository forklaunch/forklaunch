# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e
# Golden apps: the canonical application this script starts from is scaffolded,
# installed and built once (fixtures/golden.py); see "Golden apps" in README.md.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/fixtures/golden.sh"

if [ -d "output/depcheck" ]; then
    rm -rf output/depcheck
fi
mkdir -p output/depcheck
cd output/depcheck

golden_app --scaffold ts-node depcheck-test-node-application
RUST_BACKTRACE=1 $FL init library library-test -p depcheck-test-node-application -D "Test library"
RUST_BACKTRACE=1 $FL init service service-test -d postgresql -p depcheck-test-node-application -D "Test service"
RUST_BACKTRACE=1 $FL depcheck -p depcheck-test-node-application

golden_app --scaffold ts-bun depcheck-test-bun-application
RUST_BACKTRACE=1 $FL init library library-test -p depcheck-test-bun-application -D "Test library"
RUST_BACKTRACE=1 $FL init service service-test -d postgresql -p depcheck-test-bun-application -D "Test service"
RUST_BACKTRACE=1 $FL depcheck -p depcheck-test-bun-application
