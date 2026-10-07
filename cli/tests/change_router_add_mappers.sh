# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/change-router-add-mappers" ]; then
    rm -rf output/change-router-add-mappers
fi

mkdir -p output/change-router-add-mappers
cd output/change-router-add-mappers

RUST_BACKTRACE=1 $FL init application add-mappers-test-app -p add-mappers-test-app -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r node -t vitest -D "Test application for change router add-mappers" -A "Forklaunch Team" -L 'MIT' -m
RUST_BACKTRACE=1 $FL init service product-service -d postgresql -p add-mappers-test-app/src/modules -D "Product service without mappers initially"

cd add-mappers-test-app/src/modules/product-service

RUST_BACKTRACE=1 $FL change router --add-mappers

cd ..

pnpm install
pnpm build
