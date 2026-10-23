# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e
# Golden apps: the canonical application this script starts from is scaffolded,
# installed and built once (fixtures/golden.py); see "Golden apps" in README.md.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/fixtures/golden.sh"

# docker is needed for this
if [ -d "output/init-compile" ]; then
    rm -rf output/init-compile
fi

mkdir -p output/init-compile
cd output/init-compile

golden_app ts-node compile-test-node-application
RUST_BACKTRACE=1 $FL init worker worker-test-cache -t redis -p compile-test-node-application -D "Test worker"
RUST_BACKTRACE=1 $FL init router router-test -p compile-test-node-application/src/modules/billing

cd compile-test-node-application/src/modules

pnpm install
pnpm build
pnpm database:setup

docker compose -p compile-test-node-application down

cd ../../..

golden_app ts-bun compile-test-bun-application
RUST_BACKTRACE=1 $FL init worker worker-test -t database -d postgresql -p compile-test-bun-application -D "Test worker"
RUST_BACKTRACE=1 $FL init router router-test -p compile-test-bun-application/src/modules/billing

cd compile-test-bun-application/src/modules

bun install
bun run build
bun database:setup

docker compose -p compile-test-bun-application down