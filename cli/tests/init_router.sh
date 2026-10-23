# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e
# Golden apps: the canonical application this script starts from is scaffolded,
# installed and built once (fixtures/golden.py); see "Golden apps" in README.md.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/fixtures/golden.sh"

if [ -d "output/init-router" ]; then
    rm -rf output/init-router
fi

mkdir -p output/init-router
cd output/init-router

golden_app ts-node router-test-node-application
RUST_BACKTRACE=1 $FL init router rtr-test -p router-test-node-application/src/modules/billing

cd router-test-node-application/src/modules/billing

RUST_BACKTRACE=1 $FL init router rtr-test-two 

pnpm install
pnpm build

cd ../../../..

RUST_BACKTRACE=1 $FL init application router-test-bun-application -p router-test-bun-application -o modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L "MIT"
RUST_BACKTRACE=1 $FL init router rtr-test -p router-test-bun-application/modules/iam

cd router-test-bun-application/modules/iam

RUST_BACKTRACE=1 $FL init router rtr-test-two 

bun install
bun run build
