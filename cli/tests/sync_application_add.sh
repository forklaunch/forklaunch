if [ -d "output/sync-application-add" ]; then
    rm -rf output/sync-application-add
fi

mkdir -p output/sync-application-add
cd output/sync-application-add

RUST_BACKTRACE=1 cargo run --release init application sync-test-node-application \
    -p sync-test-node-application \
    -o src/modules \
    -d postgresql \
    -f prettier \
    -l eslint \
    -v zod \
    -F express \
    -r node \
    -t vitest \
    -D "Test Sync Application" \
    -A "Mushroom Research" \
    -L 'AGPL-3.0' \
    -m

# Init dummy application
RUST_BACKTRACE=1 cargo run --release init application sync-test-dummy-application \
    -p sync-test-dummy-application \
    -o src/modules \
    -d postgresql \
    -f prettier \
    -l eslint \
    -v zod \
    -F express \
    -r node \
    -t vitest \
    -D "Test Sync Application Dummy" \
    -A "Mushroom Research" \
    -L 'AGPL-3.0' \
    -m

RUST_BACKTRACE=1 cargo run --release init library lib-dummy \
    -p sync-test-dummy-application/src/modules \
    -D "Test library"

RUST_BACKTRACE=1 cargo run --release init service svc-dummy \
    -d postgresql \
    -p sync-test-dummy-application/src/modules \
    -D "Test service"

RUST_BACKTRACE=1 cargo run --release init worker wrk-dummy \
    -t database \
    -d postgresql \
    -p sync-test-dummy-application/src/modules \
    -D "Test worker"

RUST_BACKTRACE=1 cargo run --release init router rtr-dummy \
    -p sync-test-dummy-application/src/modules/svc-dummy

RUST_BACKTRACE=1 cargo run --release init router rtr-dummy-two \
    -p sync-test-dummy-application/src/modules/svc-dummy

# Define copy operations with proper key-value syntax
# Copy operations to simulate manual project additions
declare -A copy_operations=(
    ["sync-test-dummy-application/src/modules/lib-dummy"]="sync-test-node-application/src/modules/"
    ["sync-test-dummy-application/src/modules/svc-dummy"]="sync-test-node-application/src/modules/"
    ["sync-test-dummy-application/src/modules/wrk-dummy"]="sync-test-node-application/src/modules/"
)

# Copy operations to add projects
echo "[INFO] Copying projects from dummy to test application"
for source_path in "${!copy_operations[@]}"; do
    destination_path="${copy_operations[$source_path]}"
    
    if [ ! -e "$source_path" ]; then
        echo "[ERROR] Source '$source_path' does not exist"
        exit 1
    fi
    
    mkdir -p "$destination_path"
    echo "  Copying $source_path to $destination_path"
    cp -rp "$source_path" "$destination_path"
done

# Fix all references to use the correct application name
echo "[INFO] Fixing application name references in all copied files"
find sync-test-node-application/src/modules/lib-dummy sync-test-node-application/src/modules/svc-dummy sync-test-node-application/src/modules/wrk-dummy -type f \
  -exec sed -i.bak 's/@sync-test-dummy-application\//@sync-test-node-application\//g' {} \;
find sync-test-node-application/src/modules -name "*.bak" -type f -delete

# Run sync to add the new projects
echo "[INFO] Running sync all to add new projects"
PROMPTS_JSON='{"svc-dummy": {"category": "service", "database": "postgresql", "infrastructure": "none", "description": "Dummy service"}, "lib-dummy": {"category": "library", "description": "Dummy library"}, "wrk-dummy": {"category": "worker", "type": "database", "database": "postgresql", "description": "Dummy worker"}}'

RUST_BACKTRACE=1 cargo run --release sync all -p sync-test-node-application -c -P "$PROMPTS_JSON"

# Verify build
cd sync-test-node-application/src/modules
pnpm install
pnpm build

echo "[SUCCESS] Sync add test completed"