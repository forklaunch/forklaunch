# CLI end-to-end tests

Shell scripts in this directory drive the CLI under test (`FORKLAUNCH_CLI`, or
a `cargo run` of `cli/` when unset) and prove that scaffolding and the
code-editing commands produce projects that install, build and lint, for
TypeScript, Rust and mixed applications. Framework and module behaviour is
tested by `cargo test` in framework-rs and blueprint-rs and by the TypeScript
packages' own suites, not here. Helpers the scripts need live in `fixtures/`.
CI runs them from `.github/workflows/e2e_tests.yml`.

## Running them

```bash
bash cli/tests/shards.sh list                  # shards and their scripts
bash cli/tests/shards.sh run rust-commands     # run shards, with per-script timings
bash cli/tests/init_service.sh                 # or one script
```

CI's layout (which shards form which job, and which run on pull requests) is
`.github/ci/e2e-suites.json`. The `rust-images` shard (Docker images of
generated apps, and a build against the framework crates as packaged for
release) runs on full runs only.

Every script scaffolds the applications it tests. To keep the Rust jobs from
compiling the same dependency graph cold, CI builds it once per run with
`fixtures/rust_dependency_target.sh build`, packs the Cargo target and points
every Rust job's `CARGO_TARGET_DIR` at the unpacked copy. Keep
`CARGO_INCREMENTAL=0`, the toolchain and `CARGO_PROFILE_DEV_DEBUG` identical
between that job and its consumers: they are part of every fingerprint.

The TypeScript/Rust parity suites are not here: they live in
`framework-rs/parity` and run nightly from `.github/workflows/rust_parity.yml`.
