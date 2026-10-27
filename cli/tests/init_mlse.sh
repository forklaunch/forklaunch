# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-mlse" ]; then
    rm -rf output/init-mlse
fi

mkdir -p output/init-mlse
cd output/init-mlse

RUST_BACKTRACE=1 $FL init application mlse-node -p mlse-node -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m mlse-base -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0'

# corpus maintenance scripts and the evaluation set ship with the module
for f in scripts/refresh-corpus.ts scripts/load-mesh.ts scripts/run-eval.ts scripts/enforce-retention.ts eval/gold-set.example.json; do
    test -f "mlse-node/src/modules/mlse/$f" || { echo "missing mlse/$f"; exit 1; }
done
for s in corpus:refresh mesh:load eval:run retention:enforce; do
    grep -q "\"$s\"" mlse-node/src/modules/mlse/package.json || { echo "missing script $s"; exit 1; }
done

cd mlse-node/src/modules

pnpm install
pnpm build
pnpm database:setup

docker compose -p mlse-node down

cd ../../..

RUST_BACKTRACE=1 $FL init application mlse-bun -p mlse-bun -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r bun -t vitest -m mlse-base -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0'

cd mlse-bun/src/modules

bun install
bun run build
bun database:setup

docker compose -p mlse-bun down
