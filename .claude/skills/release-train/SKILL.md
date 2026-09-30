---
name: release-train
description: "Release forklaunch-js packages: framework, then blueprint, then CLI version constants (pnpm up:packages, changesets, pnpm publish:packages, check_blueprint_deps --fix). Use when publishing @forklaunch/* to npm or bumping what the CLI generates."
user-invokable: true
---

# Release train: framework, then blueprint, then CLI

A release publishes the framework (`framework/`, e.g. `@forklaunch/core`) and
then the blueprint's implementation and interface packages (`blueprint/`, which
depend on the framework). Last, it points the CLI's generated code at the new
versions. Each step consumes what the previous one published, so the order
is fixed.

The whole ceremony is pnpm and changesets commands. **Never edit a
`package.json`, a lockfile or a version constant by hand.** If a run produces
the wrong versions, restore the files from git and run the command again with
the right flags. A hand edit is how a range quietly goes down.

## 0. Release from one branch

Publish from one branch that holds everything going out: `main`, or a release
branch merged from `main` plus the PRs in the release. npm publishes cannot be
undone. A version published from an unmerged branch is the version everyone
installs.

Check npm auth first:

```bash
npm whoami            # must print the publishing account
```

## 1. Framework

```bash
cd framework
pnpm install --frozen-lockfile
pnpm up:packages --config.minimum-release-age=0   # every dependency to latest
pnpm build && npx vitest run                     # compare failures with main
```

Write changesets for what changed. Pick the bump from the change, not from
the dependencies:

```bash
npx changeset status                 # which packages will bump, and how
# add .changeset/<name>.md files:
# ---
# '@forklaunch/core': major|minor|patch
# ---
# One paragraph a user can act on.
npx changeset version                # bumps versions, writes CHANGELOGs
pnpm install && pnpm build
git add -A && git commit -m "chore(release): framework ..."
pnpm publish:packages --no-git-checks
```

Then confirm npm really has every version. `npm view` lags 1–3 minutes, so
poll, and don't trust one read:

```bash
for d in common core express hyper-express internal testing validator ws infrastructure/redis infrastructure/S3; do
  n=$(node -p "require('./$d/package.json').name"); v=$(node -p "require('./$d/package.json').version")
  npm view "$n@$v" version || echo "missing $n@$v"
done
```

## 2. Blueprint

```bash
cd blueprint
pnpm install --frozen-lockfile
pnpm cache delete "@forklaunch/*"                 # see "Traps"
pnpm up:packages --config.minimum-release-age=0
grep -h '"@forklaunch/core"' */package.json implementations/*/*/package.json | sort | uniq -c   # all on the new version?
pnpm build
```

Run the blueprint's tests per package, not from the blueprint root. The root
config loads a setup file that is not in git:

```bash
for p in billing-stripe iam-better-auth ecommerce-stripe implementations/billing/stripe; do
  (cd $p && npx vitest run --passWithNoTests)
done
```

Then do the same changesets steps as the framework (`changeset status`,
`changeset version`, `pnpm install`, `pnpm build`, commit,
`pnpm publish:packages --no-git-checks`, poll npm).

## 3. CLI version constants

The CLI writes framework and blueprint versions into generated apps. They come
from `cli/src/core/package_json/package_json_constants.rs`, which a script
rewrites from the manifests:

```bash
cd .github/workflows/scripts/check_blueprint_deps
cargo run -- --fix        # rewrite the constants from framework/ and blueprint/
cargo run                 # check mode, as CI runs it: "All package versions are correct"
cd ../../../../cli && cargo test -q --bin forklaunch
```

Prove it end to end the way CI does. The e2e scripts generate apps with this
CLI and build them against what you just published:

```bash
cd cli && cargo build --release
FORKLAUNCH_CLI=$PWD/target/release/forklaunch bash tests/change_service.sh
FORKLAUNCH_CLI=$PWD/target/release/forklaunch bash tests/init_billing_stripe.sh
```

The CLI release itself is a tag, `cli-vX.Y.Z` (`cli_release.yml` reads the
version from the tag; `Cargo.toml` stays `0.0.0`).

## Traps

- **pnpm 11 `minimumReleaseAge`.**
  - pnpm refuses versions published in roughly the last day, and silently resolves to an older one. Your own packages are always minutes old.
  - It also applies unevenly: it is skipped for packages whose cached metadata lacks a `time` field. So the framework and the blueprint can resolve **different** third-party versions in the same hour. That is how two copies of `@mikro-orm/core` end up installed, and the blueprint fails with `EntitySchemaWithMeta … is not assignable to EntityName<any>`.
  - Run `up:packages` with `--config.minimum-release-age=0` in **both** workspaces, so both resolve the true latest.
- **pnpm's metadata cache.** It can miss a version published minutes ago. `pnpm cache delete "@forklaunch/*"` before the blueprint's `up:packages` fixes it.
- **`update` never lowers a specifier.** If an `up:packages` run wrote the wrong version, restore the manifests and lockfile (`git checkout -- '*package.json' pnpm-lock.yaml`), `pnpm install --frozen-lockfile`, and run again with the right flags.
- **Quote globs in flags.** zsh expands `@forklaunch/*` before pnpm sees it.
- **Publish command.** It is `pnpm publish:packages --no-git-checks`, never with `--` in front of the flag: the `--` is forwarded literally to `pnpm publish`. npm answers E409 "previously staged version" while it catches up. Re-running until every package reports "skipping" is the check.
- **Private packages with a `publish:package` script.**
  - Some blueprint app modules, such as `@forklaunch/blueprint-ecommerce-stripe`, are `private` but still have a publish script. A changeset that bumps one makes `pnpm publish:packages` fail on it (`ERR_PNPM_RECURSIVE_RUN_FIRST_FAIL`).
  - The real packages still publish; poll npm to confirm.
- **MikroORM must match.** The framework and the blueprint must resolve the same `@mikro-orm/*` version, because core pins it exactly. Check with `grep -h '"@mikro-orm/core"'` across both.
- **The `--latest` flag rewrites aliases.** An alias like `"x": "npm:y@^5"` becomes the latest `y`. The repo is TypeScript 7 throughout. The compliant-fields codemod uses TypeScript 7's own API (`typescript/unstable/sync`), so no TypeScript 5 alias is needed.
- **Pre-existing test noise.**
  - On `main` today, six framework test files are type-only and vitest reports "no test suite found" for them.
  - Suites using test containers (Postgres, Redis) time out when the machine runs many containers.
  - Compare against a clean `main` checkout before calling a failure new.
