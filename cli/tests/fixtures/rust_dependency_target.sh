#!/usr/bin/env bash
# Build the dependency graph of generated Rust applications once, into the
# Cargo target directory FORKLAUNCH_RUST_TARGET, so the Rust E2E jobs start
# from it instead of each compiling the same crates cold.
#
# It scaffolds one throwaway Rust application with every component kind,
# checks its docker-compose.yaml and builds its workspace; only the target
# directory is kept. The tests still
# scaffold fresh applications, which is what they test: Cargo reuses the
# registry crates (same versions, features and profile) and rebuilds each
# application's own crates. Consumers must use the same absolute
# FORKLAUNCH_RUST_TARGET, toolchain, CARGO_INCREMENTAL=0 and
# CARGO_PROFILE_DEV_DEBUG, which are part of every fingerprint.
#
#   FORKLAUNCH_CLI=... FORKLAUNCH_RUST_TARGET=... bash rust_dependency_target.sh build
#   FORKLAUNCH_RUST_TARGET=... bash rust_dependency_target.sh pack OUT.tar.zst
#   FORKLAUNCH_RUST_TARGET=... bash rust_dependency_target.sh unpack IN.tar.zst
set -euo pipefail
target="${FORKLAUNCH_RUST_TARGET:?set FORKLAUNCH_RUST_TARGET to an absolute path}"
case "${1:-}" in
  build)
    cli="${FORKLAUNCH_CLI:?set FORKLAUNCH_CLI}"
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    app="$work/deps-app"
    export CARGO_TARGET_DIR="$target" CARGO_INCREMENTAL=0 FORKLAUNCH_SKIP_FORMAT=1
    run() { echo "+ forklaunch $*"; "$cli" "$@"; }
    run init application deps-app --path "$app" --language rust -D 'Dependency target' -A ForkLaunch -L MIT
    run init service api --path "$app" --language rust -D API
    run init router users --path "$app/src/api" --language rust
    run init service records --path "$app" --language rust -d postgresql -D Records
    run init service notes --path "$app" --language rust --mappers --infrastructure redis s3 -D Notes
    run init worker jobs --path "$app" --language rust -t database -d postgresql --runtime apalis -D Jobs
    run init worker queue --path "$app" --language rust --consumer-only -t database -d postgresql -D Queue
    run init worker tasks --path "$app" --language rust -t redis -D Tasks
    run init worker inbox --path "$app" --language rust --consumer-only -t redis -D Inbox
    run init worker events --path "$app" --language rust --consumer-only -t kafka -D Events
    run init library shared --path "$app" --language rust -D Shared
    run init library clients --path "$app" --language rust --sdk-services api -D Clients
    run init module iam --path "$app" --language rust -m iam-base -d postgresql
    run init module billing --path "$app" --language rust -m billing-base
    run init module payments --path "$app" --language rust -m billing-stripe
    run init module cac --path "$app" --language rust -m cac-base
    run init module ecommerce --path "$app" --language rust -m ecommerce-stripe
    # Every worker type (database/apalis, redis, kafka, consumer-only) and
    # module is scaffolded here, so check the generated Compose file too.
    docker compose -f "$app/docker-compose.yaml" config --quiet
    echo "PASS: docker compose config for every Rust component kind"
    (cd "$app" && cargo build --workspace)
    du -sh "$target"
    ;;
  pack)
    out="${2:?pack OUT.tar.zst}"
    tar -C "$target" --zstd -cf "$out" .
    ls -la "$out"
    ;;
  unpack)
    in="${2:?unpack IN.tar.zst}"
    mkdir -p "$target"
    tar -C "$target" --zstd -xf "$in"
    du -sh "$target"
    ;;
  *)
    echo "usage: rust_dependency_target.sh build | pack OUT | unpack IN" >&2
    exit 2
    ;;
esac
