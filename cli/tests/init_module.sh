if [ -d "output/init-module" ]; then
    rm -rf output/init-module
fi

mkdir -p output/init-module
cd output/init-module

RUST_BACKTRACE=1 cargo run --release init application service-test-node-application -p service-test-node-application -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -D "Test service" -A "Rohin Bhargava" -L 'AGPL-3.0' -m
RUST_BACKTRACE=1 cargo run --release init module -m iam-base -d postgresql -p service-test-node-application
RUST_BACKTRACE=1 cargo run --release init module -m billing-base -d postgresql -p service-test-node-application

cd service-test-node-application/src/modules

pnpm install
pnpm build

cd ../../..

RUST_BACKTRACE=1 cargo run --release init application service-test-bun-application -p service-test-bun-application -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -D "Test service" -A "Rohin Bhargava" -L "MIT" -m
RUST_BACKTRACE=1 cargo run --release init module -m iam-base -d postgresql -p service-test-bun-application
RUST_BACKTRACE=1 cargo run --release init module -m billing-base -d postgresql -p service-test-bun-application

cd service-test-bun-application/src/modules

bun install
bun run build

cd ../../..

