# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -e

if [ -d "output/existing-docker-compose" ]; then
    rm -rf output/existing-docker-compose
fi

mkdir -p output/existing-docker-compose
cd output/existing-docker-compose

# Create an existing docker-compose.yaml under infra/ BEFORE initializing ForkLaunch
echo "Creating existing docker-compose.yaml with basic services under infra/..."
mkdir -p infra
cat > infra/docker-compose.yaml << 'EOF'
version: '3.8'

services:
  existing-service:
    image: nginx:latest
    ports:
      - "8080:80"
    networks:
      - test-app-network

  redis:
    image: redis:latest
    ports:
      - "6379:6379"
    networks:
      - test-app-network

networks:
  test-app-network:
    name: test-app-network
    driver: bridge

volumes:
  existing-data:
    driver: local
EOF

if ! grep -q "existing-service:" infra/docker-compose.yaml; then
    echo "ERROR: existing-service not found in infra/docker-compose.yaml"
    exit 1
fi

if ! grep -q "redis:" infra/docker-compose.yaml; then
    echo "ERROR: redis service not found in infra/docker-compose.yaml"
    exit 1
fi

RUST_BACKTRACE=1 $FL init application test-app \
    --path "." \
    -o "src/modules" \
    -d "postgresql" \
    -v "zod" \
    -f "prettier" \
    -l "eslint" \
    -F "express" \
    -r "node" \
    -t "vitest" \
    -m "billing-base" \
    -m "iam-base" \
    -D "Test application with existing docker-compose" \
    -A "Test Author" \
    -L "MIT"

if [ $? -ne 0 ]; then
    echo "ERROR: Failed to initialize application"
    exit 1
fi

echo "Application initialized successfully"

if ! grep -q "existing-service:" infra/docker-compose.yaml; then
    echo "ERROR: existing-service was removed during ForkLaunch init"
    exit 1
fi

if ! grep -q "redis:" infra/docker-compose.yaml; then
    echo "ERROR: redis service was removed during ForkLaunch init"
    exit 1
fi

RUST_BACKTRACE=1 $FL init service test-service \
    -d "postgresql" \
    -p "src/modules" \
    -D "Test service for existing docker-compose"

if [ $? -ne 0 ]; then
    echo "ERROR: Failed to add service"
    exit 1
fi

if ! grep -q "test-service:" infra/docker-compose.yaml; then
    echo "ERROR: test-service was not added to infra/docker-compose.yaml"
    exit 1
fi

if ! grep -q "container_name: test-app-test-service-node" infra/docker-compose.yaml; then
    echo "ERROR: test-service container_name not found"
    exit 1
fi

if ! grep -q "hostname: test-service" infra/docker-compose.yaml; then
    echo "ERROR: test-service hostname not found"
    exit 1
fi

if ! grep -q "image: test-app-test-service-node:latest" infra/docker-compose.yaml; then
    echo "ERROR: test-service image not found"
    exit 1
fi

if ! grep -q "depends_on:" infra/docker-compose.yaml; then
    echo "ERROR: test-service depends_on not found"
    exit 1
fi

if ! grep -q "8081:8081" infra/docker-compose.yaml; then
    echo "ERROR: test-service port mapping not found"
    exit 1
fi

if ! grep -q "test-app-network" infra/docker-compose.yaml; then
    echo "ERROR: test-service network configuration not found"
    exit 1
fi

if ! grep -q "existing-service:" infra/docker-compose.yaml; then
    echo "ERROR: existing-service was removed after adding new service"
    exit 1
fi

if ! grep -q "redis:" infra/docker-compose.yaml; then
    echo "ERROR: redis service was removed after adding new service"
    exit 1
fi

cd src/modules
pnpm install
if [ $? -ne 0 ]; then
    echo "ERROR: Failed to install dependencies"
    exit 1
fi

pnpm build
if [ $? -ne 0 ]; then
    echo "ERROR: Failed to build application"
    exit 1
fi

cd ../..
if ! docker compose -f infra/docker-compose.yaml config > /dev/null 2>&1; then
    echo "ERROR: infra/docker-compose.yaml has invalid syntax"
    exit 1
fi