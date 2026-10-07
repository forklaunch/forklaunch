# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

echo "Testing environment sync command..."

if [ -d "output/environment-sync" ]; then
    rm -rf output/environment-sync
fi

mkdir -p output/environment-sync
cd output/environment-sync

echo "Creating test application..."
RUST_BACKTRACE=1 $FL init application env-sync-app \
    -p env-sync-app \
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
    -D "Test application for environment sync" \
    -A "Test Author" \
    -L "MIT"

cd env-sync-app

# Test dry run first
echo "Testing environment sync --dry-run..."
RUST_BACKTRACE=1 $FL environment sync --dry-run

# Test actual sync
echo "Testing environment sync (creating missing variables)..."
RUST_BACKTRACE=1 $FL environment sync

echo "Checking created .env files..."
if [ -f ".env.local" ]; then
    echo "✅ Root .env.local created:"
    cat .env.local
else
    echo "❌ Root .env.local not found"
fi

for module in src/modules/*/; do
    if [ -d "$module" ]; then
        module_name=$(basename "$module")
        echo "Checking module: $module_name"
        if [ -f "$module/.env.local" ]; then
            echo "✅ $module_name/.env.local exists:"
            cat "$module/.env.local"
        elif [ -f "$module/.env" ]; then
            echo "✅ $module_name/.env exists:"
            cat "$module/.env"
        else
            echo "ℹ️  No .env file in $module_name"
        fi
    fi
done

echo "Testing environment validate after sync (expecting no missing variables)..."
RUST_BACKTRACE=1 $FL environment validate

echo "Environment sync test completed!"
