# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-service-mappers" ]; then
    rm -rf output/init-service-mappers
fi

mkdir -p output/init-service-mappers
cd output/init-service-mappers

RUST_BACKTRACE=1 $FL init application mapper-test-app -p mapper-test-app -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r node -t vitest -D "Test application for mapper feature" -A "Forklaunch Team" -L 'MIT' -m
RUST_BACKTRACE=1 $FL init service user-service -d postgresql -p mapper-test-app/src/modules -D "User service without mappers"
RUST_BACKTRACE=1 $FL init service product-service --mappers -d postgresql -p mapper-test-app/src/modules -D "Product service with mappers"

cd mapper-test-app/src/modules

pnpm install
pnpm build
