# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/change-service" ]; then
    rm -rf output/change-service
fi

mkdir -p output/change-service
cd output/change-service

RUST_BACKTRACE=1 $FL init application change-service-test-node-application -p . -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r bun -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L 'AGPL-3.0'
RUST_BACKTRACE=1 $FL init service svc -d postgresql -p . -D "Test service"
RUST_BACKTRACE=1 $FL change service -p svc -N newsvc -d mongodb -D "Test service 2" -i redis -c
RUST_BACKTRACE=1 $FL change service -p newsvc -i redis -i s3 -c

cd src/modules

bun install
bun run build