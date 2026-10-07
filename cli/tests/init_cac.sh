# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-cac" ]; then
    rm -rf output/init-cac
fi

mkdir -p output/init-cac
cd output/init-cac

RUST_BACKTRACE=1 $FL init application cac-node -p cac-node -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m cac-base -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0'

cd cac-node/src/modules

pnpm install
pnpm build
pnpm database:setup

docker compose -p cac-node down

cd ../../..

RUST_BACKTRACE=1 $FL init application cac-bun -p cac-bun -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -m cac-base -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0'

cd cac-bun/src/modules

bun install
bun run build
bun database:setup

docker compose -p cac-bun down
