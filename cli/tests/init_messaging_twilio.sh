# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-messaging-twilio" ]; then
    rm -rf output/init-messaging-twilio
fi

mkdir -p output/init-messaging-twilio
cd output/init-messaging-twilio

RUST_BACKTRACE=1 $FL init application messaging-twilio-node -p messaging-twilio-node -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m messaging-twilio -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0'

cd messaging-twilio-node/src/modules

pnpm install
pnpm build
pnpm database:setup

docker compose -p messaging-twilio-node down

cd ../../..

RUST_BACKTRACE=1 $FL init application messaging-twilio-bun -p messaging-twilio-bun -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -m messaging-twilio -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0'

cd messaging-twilio-bun/src/modules

bun install
bun run build
bun database:setup

docker compose -p messaging-twilio-bun down
