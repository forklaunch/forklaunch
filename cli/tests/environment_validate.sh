# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

echo "Testing environment validate command..."

if [ -d "output/environment-validate" ]; then
    rm -rf output/environment-validate
fi

mkdir -p output/environment-validate
cd output/environment-validate

echo "Creating test application..."
RUST_BACKTRACE=1 $FL init application env-test-app \
    -p env-test-app \
    -o src/modules \
    -d postgresql \
    -f prettier \
    -l eslint \
    -v zod \
    -F express \
    -r node \
    -t vitest \
    -m billing-base \
    -m iam-base \
    -D "Test application for environment validation" \
    -A "Test Author" \
    -L "MIT"

echo "Adding custom service..."
RUST_BACKTRACE=1 $FL init service custom-svc \
    -d postgresql \
    -p env-test-app/src/modules \
    -D "Custom service for env testing"

cd env-test-app

echo "Testing environment validate (expecting missing variables)..."
# validate exits nonzero when variables are missing — that is the expected
# outcome of this invocation, not a test failure.
RUST_BACKTRACE=1 $FL environment validate \
    || echo "[EXPECTED] validate reported missing variables"

# Create some .env files with partial variables
echo "Creating partial .env files..."
mkdir -p src/modules/billing
echo "HOST=localhost" > src/modules/billing/.env.local
echo "PORT=3001" >> src/modules/billing/.env.local

mkdir -p src/modules/iam
echo "HOST=localhost" > src/modules/iam/.env.local
echo "HMAC_SECRET_KEY=test-secret" >> src/modules/iam/.env.local

echo "Testing environment validate after adding some variables..."
# Still nonzero by design: only partial variables were added above.
RUST_BACKTRACE=1 $FL environment validate \
    || echo "[EXPECTED] validate reported remaining missing variables"

echo "Environment validate test completed!"
