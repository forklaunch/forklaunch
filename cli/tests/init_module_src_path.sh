if [ -d "output/init-module" ]; then
    rm -rf output/init-module
fi

mkdir -p output/init-module
cd output/init-module

# Test for specific path for modules using -f flag (src path)
RUST_BACKTRACE=1 cargo run --release init application module-test-src-path -p module-test-src-path -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -D "Test service" -A "ForkLaunch" -L 'AGPL-3.0' -m
RUST_BACKTRACE=1 cargo run --release init module -m iam-base -d postgresql -p module-test-src-path
RUST_BACKTRACE=1 cargo run --release init module -m billing-base -d postgresql -p module-test-src-path

cd module-test-src-path/src/modules

pnpm install
pnpm build

cd ../../..

# Test for specific path for modules using -f flag (no src path)
RUST_BACKTRACE=1 cargo run --release init application module-test-no-src-path -p module-test-no-src-path -o modules -d postgresql -f biome -l oxlint -v zod -F express -r node -t vitest -D "Test service" -A "ForkLaunch" -L "MIT" -m
RUST_BACKTRACE=1 cargo run --release init module -m iam-base -d postgresql -p module-test-no-src-path
RUST_BACKTRACE=1 cargo run --release init module -m billing-base -d postgresql -p module-test-no-src-path

cd module-test-no-src-path/modules

pnpm install
pnpm build

cd ../..