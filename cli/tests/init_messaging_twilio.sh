#!/bin/bash
# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/init-messaging-twilio" ]; then
    rm -rf output/init-messaging-twilio
fi

mkdir -p output/init-messaging-twilio
cd output/init-messaging-twilio

# Messaging is retired as a module; email and SMS go through the managed gateway.
if RUST_BACKTRACE=1 $FL init application messaging-twilio-node -p messaging-twilio-node -o src/modules -d postgresql -f prettier -l oxlint -v zod -F express -r node -t vitest -m messaging-twilio -D "Test library" -A "Rohin Bhargava" -L 'AGPL-3.0' 2> rejection.log; then
    echo 'Expected the retired messaging-twilio module to be rejected'
    exit 1
fi
grep -q 'managed gateway' rejection.log
test ! -e messaging-twilio-node/src/modules/messaging
