# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/dryrun" ]; then
    rm -rf output/dryrun
fi

mkdir -p output/dryrun
cd output/dryrun

RUST_BACKTRACE=1 $FL init application dryrun-test-node-application -p dryrun-test-node-application -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L 'AGPL-3.0' -n

if [ "$(ls -A)" ]; then
     echo "Error: Directory not empty" >&2
     exit 1
fi

RUST_BACKTRACE=1 $FL init application dryrun-test-node-application -p dryrun-test-node-application -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r node -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L 'AGPL-3.0'

cd dryrun-test-node-application

RUST_BACKTRACE=1 $FL init library library-test -D "Test service" -p . -n
RUST_BACKTRACE=1 $FL init service service-test -d postgresql -D "Test service" -p . -n
RUST_BACKTRACE=1 $FL init worker worker-test -t database -d postgresql -D "Test worker" -p . -n

if [ -d "library-test" ]; then
    echo "Error: library-test directory exists" >&2
    exit 1
fi

if [ -d "service-test" ]; then
    echo "Error: service-test directory exists" >&2
    exit 1
fi

if [ -d "worker-test" ]; then
    echo "Error: worker-test directory exists" >&2
    exit 1
fi


RUST_BACKTRACE=1 $FL init service service-test -d postgresql -D "Test service" -p .

cd src/modules/service-test

RUST_BACKTRACE=1 $FL init router router-test -n

if [ -d "router-test" ]; then
    echo "Error: router-test directory exists" >&2
    exit 1
fi
