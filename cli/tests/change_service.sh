if [ -d "output/change-service" ]; then
    rm -rf output/change-service
fi

mkdir -p output/change-service
cd output/change-service

RUST_BACKTRACE=1 cargo run --release init application change-service-test-node-application -p . -o src/modules -d postgresql -f prettier -l eslint -v zod -F express -r bun -t vitest -m billing-base -m iam-base -D "Test service" -A "Rohin Bhargava" -L 'AGPL-3.0'
RUST_BACKTRACE=1 cargo run --release init service svc -d postgresql -p . -D "Test service"
RUST_BACKTRACE=1 cargo run --release change service -p svc -N newsvc -d mongodb -D "Test service 2" -i redis -c
RUST_BACKTRACE=1 cargo run --release change service -p newsvc -i s3 -c

cd src/modules

bun install
bun run build