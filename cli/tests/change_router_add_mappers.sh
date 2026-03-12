if [ -d "output/change-router-add-mappers" ]; then
    rm -rf output/change-router-add-mappers
fi

mkdir -p output/change-router-add-mappers
cd output/change-router-add-mappers

RUST_BACKTRACE=1 cargo run --release init application add-mappers-test-app -p add-mappers-test-app -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r node -t vitest -D "Test application for change router add-mappers" -A "Forklaunch Team" -L 'MIT' -m
RUST_BACKTRACE=1 cargo run --release init service product-service -d postgresql -p add-mappers-test-app/src/modules -D "Product service without mappers initially"

cd add-mappers-test-app/src/modules/product-service

RUST_BACKTRACE=1 cargo run --release change router --add-mappers

cd ..

pnpm install
pnpm build
