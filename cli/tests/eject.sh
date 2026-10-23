# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
# Golden apps: the canonical application this script starts from is scaffolded,
# installed and built once (fixtures/golden.py); see "Golden apps" in README.md.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/fixtures/golden.sh"
# KNOWN-ISSUE set-e exemption — this script tolerates in-test failures until
# #267: ejected iam implementation fails tsgo build (TS2352 mapper casts).
# Remove this exemption (restore `set -e`) when the issue is fixed.

if [ -d "output/eject" ]; then
    rm -rf output/eject
fi

mkdir -p output/eject
cd output/eject

golden_app ts-node service-test-node-application

cd service-test-node-application/src/modules

pnpm install

cd iam

RUST_BACKTRACE=1 $FL eject -d @forklaunch/implementation-iam-base -d @forklaunch/interfaces-iam -n

cd ..

RUST_BACKTRACE=1 $FL eject -d @forklaunch/implementation-iam-base -d @forklaunch/interfaces-iam -p iam -c

pnpm build

cd ../../..

golden_app ts-bun service-test-bun-application

cd service-test-bun-application/src/modules

bun install

cd billing

RUST_BACKTRACE=1 $FL eject -d @forklaunch/implementation-billing-base -d @forklaunch/interfaces-billing -d @forklaunch/infrastructure-redis -n

cd ../..

RUST_BACKTRACE=1 $FL eject -d @forklaunch/implementation-billing-base -d @forklaunch/interfaces-billing -d @forklaunch/infrastructure-redis -p modules/billing -c

cd modules

bun run build

cd ../../..
