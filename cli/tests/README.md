# CLI end-to-end tests

Shell scripts in this directory drive the CLI under test (`FORKLAUNCH_CLI`, or
a `cargo run` of `cli/` when unset) and prove that scaffolding and the
code-editing commands produce projects that install, build and lint, for
TypeScript, Rust and mixed applications. Framework and module behaviour is
tested by `cargo test` in framework-rs and blueprint-rs and by the TypeScript
packages' own suites, not here. Helpers the scripts need live in `fixtures/`.
CI runs them from `.github/workflows/e2e_tests.yml`.

## Golden apps

Most scripts used to scaffold their own copy of the same application and then
install or compile the same dependency graph again. Golden apps are those
canonical applications scaffolded once with the CLI under test, built once,
and shared: as a CI artifact, or built locally on demand.

| App            | Bundle | What it is                                                                                         | Built with                                   |
|----------------|--------|----------------------------------------------------------------------------------------------------|----------------------------------------------|
| `ts-node`      | `ts`   | TypeScript, node/express/zod/prettier, `billing-base` + `iam-base`, modules in `src/modules`         | `pnpm install && pnpm build`                 |
| `ts-bun`       | `ts`   | The same on bun with biome                                                                          | `bun install && bun run build`               |
| `rust-service` | `rust` | TypeScript shell + in-memory Rust service `accounts`                                                 | `cargo build --workspace`                    |
| `rust-full`    | `rust` | Rust app with every component: in-memory, PostgreSQL and mapper/redis/s3 services, router, apalis and consumer-only database workers, redis and consumer-only redis workers, consumer-only kafka worker, libraries (one SDK consumer), iam-base/billing-base/billing-stripe/cac-base/ecommerce-stripe modules, agent | `cargo build --workspace` |
| `mixed`        | `rust` | TypeScript-first mixed workspace (`modules/`): TS service `legacy-api` + Rust service `native-api`    | `cargo build -p native-api`                  |

The definitions live in `fixtures/golden.py` (`APPS`). Cargo unifies features
across the packages it builds, so a dependency is only warm for a script whose
app resolves it with the same features: `rust-service` and `mixed` match their
consumers exactly, `rust-full` warms the (large) part of the graph whose
features do not vary.

### Interface for CI

* **Build:** `bash cli/tests/golden_apps.sh build [ts|rust|APP ...]` (default:
  everything). Needs `FORKLAUNCH_CLI`, Rust stable, pnpm, bun, Node.
* **Package:** `bash cli/tests/golden_apps.sh package --out DIR [ts] [rust]` writes
  `DIR/golden-apps-ts.tar.zst` and `DIR/golden-apps-rust.tar.zst` (`.tar.gz`
  without zstd). Upload each as an artifact named `golden-apps-ts` /
  `golden-apps-rust`. The rust bundle carries the shared Cargo target
  (incremental dirs excluded).
* **Consume:** download, then `bash cli/tests/golden_apps.sh unpack DIR/golden-apps-*.tar.*`
  with `FORKLAUNCH_GOLDEN_APPS_DIR` set. Jobs that only need TypeScript take the
  ts bundle alone.
* **Environment the tests read:**
  * `FORKLAUNCH_GOLDEN_APPS_DIR`: where the apps live. Use the same absolute
    path in the build job and every consumer job (for example
    `$RUNNER_TEMP/golden-apps`). The path is part of what Cargo and sccache
    key on. When it is set, a missing app, or apps scaffolded by a CLI other
    than `FORKLAUNCH_CLI`, is an error. When it is unset, apps are built under
    `cli/tests/output/golden` and rebuilt whenever the CLI binary or a
    definition changes.
  * `CARGO_TARGET_DIR`: converted Rust scripts default it to
    `$FORKLAUNCH_GOLDEN_APPS_DIR/target`. Point it there for whole jobs
    (instead of `$RUNNER_TEMP/forklaunch-generated-target`), so unconverted
    scripts with the same dependency feature sets start warm too. Keep
    `CARGO_INCREMENTAL=0` and the same Rust toolchain as the build job.
    `CARGO_PROFILE_DEV_DEBUG`, `RUSTFLAGS` and the like change every
    fingerprint, so set them identically in the build job and its consumers
    (they are recorded in `golden.json`). `CARGO_PROFILE_DEV_DEBUG=line-tables-only`
    keeps the rust bundle at a fraction of its full-debuginfo size.
  * `FORKLAUNCH_CLI`: must be the binary the artifact was built with (its
    SHA-256 is in `golden.json`). `FORKLAUNCH_GOLDEN_ALLOW_CLI_MISMATCH=1`
    overrides the check.
* **Shard entrypoints:** `bash cli/tests/shards.sh run SHARD...` (`list`,
  `check`). TypeScript shards: `ts-golden`, `ts-change`, `ts-delete-sync`,
  `ts-init`, `ts-env-release`. Rust and mixed shards: `rust-commands`,
  `rust-sdk`, `rust-mixed`, `rust-modules`, and the slow `rust-images` (full
  CI runs only). The CI layout is `.github/ci/e2e-suites.json`. Each shard
  runs every script, prints per-script timings and fails if any script failed
  (`FORKLAUNCH_SHARD_FAIL_FAST=1` stops at the first).

### Writing a consumer

```python
from fixtures import golden
app = golden.app('ts-node', root / 'app')                    # built copy: node_modules, dist
app = golden.app('rust-service', root / 'app', built=False)  # the pristine scaffold
env.setdefault('CARGO_TARGET_DIR', golden.target())
```

```bash
source "$SCRIPT_DIR/fixtures/golden.sh"
golden_app ts-bun my-app                     # built copy
golden_app --scaffold rust-service "$RUN/app"  # pristine scaffold
```

Copies keep symlinks and modification times, so pnpm links and Cargo
fingerprints survive. Built copies have their `pnpm-lock.yaml` / `bun.lock`
removed: a fresh scaffold has none, and a copied one would be outdated once the
test adds a component, which pnpm rejects under `CI=true` (frozen lockfile).
The next install resolves as on a fresh scaffold and reuses `node_modules`.
Because `node_modules` is present, pnpm runs the CLI starts can install and
write a lockfile mid-test, so built copies also get `frozenLockfile: false`
(`pnpm-workspace.yaml`) and `frozen-lockfile=false` (`.npmrc`), scoped to the
copy; nothing is set job-wide. Use the pristine scaffold when the script's own first
build is under test (build.rs output, lockfile creation). Use the built copy when
the script only needs a working, installed app. Never modify a golden app in
place.
