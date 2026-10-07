# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-module" ]; then
    rm -rf output/init-module
fi

mkdir -p output/init-module
cd output/init-module

# Test for specific path for modules using -f flag (src path)
RUST_BACKTRACE=1 $FL init application module-test-src-path -p module-test-src-path -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -D "Test service" -A "ForkLaunch" -L 'AGPL-3.0' -m
RUST_BACKTRACE=1 $FL init module -m iam-base -d postgresql -p module-test-src-path
RUST_BACKTRACE=1 $FL init module -m billing-base -d postgresql -p module-test-src-path

cd module-test-src-path/src/modules

pnpm install
pnpm build

cd ../../..

# Test for specific path for modules using -f flag (no src path)
RUST_BACKTRACE=1 $FL init application module-test-no-src-path -p module-test-no-src-path -o modules -d postgresql -f biome -l oxlint -v zod -F express -r node -t vitest -D "Test service" -A "ForkLaunch" -L "MIT" -m
RUST_BACKTRACE=1 $FL init module -m iam-base -d postgresql -p module-test-no-src-path
RUST_BACKTRACE=1 $FL init module -m billing-base -d postgresql -p module-test-no-src-path

cd module-test-no-src-path/modules

pnpm install
pnpm build

cd ../..