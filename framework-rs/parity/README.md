# TypeScript/Rust parity suites

Scripts here compare the Rust framework and blueprint crates with their
TypeScript counterparts (validation, wire formats, SDKs, billing, IAM and the
rest), with the fixtures they need under `fixtures/`. They run nightly and on
demand from `.github/workflows/rust_parity.yml`, not on pull requests; the
pull-request end-to-end suite in `cli/tests` only checks scaffolding.

Moved here from `cli/tests`; the layout is unchanged, so a script that ran as
`node cli/tests/x.mjs` now runs as `node framework-rs/parity/x.mjs` from the
repository root. Which steps run in which nightly shard is data in
`.github/workflows/scripts/parity_shards.py`.
