# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e
# Golden apps: the canonical application this script starts from is scaffolded,
# installed and built once (fixtures/golden.py); see "Golden apps" in README.md.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/fixtures/golden.sh"

if [ -d "output/init-library" ]; then
    rm -rf output/init-library
fi

mkdir -p output/init-library
cd output/init-library

golden_app ts-node library-test-node-application
RUST_BACKTRACE=1 $FL init library lib-test -p library-test-node-application -D "Test library"

cd library-test-node-application

RUST_BACKTRACE=1 $FL delete library lib-test -c
RUST_BACKTRACE=1 $FL init library lib-test -D "Test service" -p .

cd src/modules

pnpm install
pnpm build

cd ../../..

golden_app ts-bun library-test-bun-application
RUST_BACKTRACE=1 $FL init library lib-test -p library-test-bun-application -D "Test library"

cd library-test-bun-application

RUST_BACKTRACE=1 $FL delete library lib-test -c
RUST_BACKTRACE=1 $FL init library lib-test -D "Test library" -p .

cd src/modules

bun install
bun run build

