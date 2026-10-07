# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-billing-stripe" ]; then
    rm -rf output/init-billing-stripe
fi

mkdir -p output/init-billing-stripe
cd output/init-billing-stripe

RUST_BACKTRACE=1 $FL init application billing-stripe-node -p billing-stripe-node -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m billing-stripe -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0'

cd billing-stripe-node/src/modules

pnpm install
pnpm build
pnpm database:setup

docker compose -p billing-stripe-node down

cd ../../..

RUST_BACKTRACE=1 $FL init application billing-stripe-bun -p billing-stripe-bun -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -m billing-stripe -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0'

cd billing-stripe-bun/src/modules

bun install
bun run build
bun database:setup

docker compose -p billing-stripe-bun down