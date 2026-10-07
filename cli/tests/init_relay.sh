# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-relay" ]; then
    rm -rf output/init-relay
fi

mkdir -p output/init-relay
cd output/init-relay

# The relay module injects the managed-apps OAuth session-ingest endpoint into
# an existing better-auth iam service, so scaffold that first, then add relay
# with `init module -m relay` and prove the injected TypeScript typechecks.
RUST_BACKTRACE=1 $FL init application relay-node -p relay-node -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m iam-better-auth -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0'

RUST_BACKTRACE=1 $FL init module -m relay -p relay-node

cd relay-node/src/modules

pnpm install
pnpm build
pnpm database:setup

docker compose -p relay-node down

cd ../../..

RUST_BACKTRACE=1 $FL init application relay-bun -p relay-bun -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -m iam-better-auth -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0'

RUST_BACKTRACE=1 $FL init module -m relay -p relay-bun

cd relay-bun/src/modules

bun install
bun run build
bun database:setup

docker compose -p relay-bun down
