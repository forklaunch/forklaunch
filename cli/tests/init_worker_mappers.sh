if [ -d "output/init-worker-mappers" ]; then
    rm -rf output/init-worker-mappers
fi

mkdir -p output/init-worker-mappers
cd output/init-worker-mappers

RUST_BACKTRACE=1 cargo run --release init application worker-mapper-test-app -p worker-mapper-test-app -o src/modules -d postgresql -f biome -l oxlint -v zod -F express -r node -t vitest -D "Test application for worker mapper feature" -A "Forklaunch Team" -L 'MIT' -m
RUST_BACKTRACE=1 cargo run --release init worker email-worker -t bullmq -p worker-mapper-test-app/src/modules -D "Email worker without mappers"
RUST_BACKTRACE=1 cargo run --release init worker notification-worker --mappers -t bullmq -p worker-mapper-test-app/src/modules -D "Notification worker with mappers"

cd worker-mapper-test-app/src/modules

pnpm install
pnpm build
