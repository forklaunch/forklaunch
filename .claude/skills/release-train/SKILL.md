---
name: release-train
description: "Release forklaunch-js packages: framework, then blueprint, then CLI version constants. Default path is the TypeScript Release workflow (ts_release.yml: Version Packages PR, dry run, publish); manual fallback is pnpm up:packages, changesets, pnpm publish:packages, check_blueprint_deps --fix. Use when publishing @forklaunch/* to npm or bumping what the CLI generates."
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

## CI release (default): `.github/workflows/ts_release.yml`

Steps 1 and 2 run in GitHub Actions. Every version reaches npm from `main`
after a reviewed PR. Run **TypeScript Release** from the Actions tab, or with
`gh workflow run ts_release.yml -f workspace=... -f stage=... -f publish=...`.

| Input | Values | Meaning |
| --- | --- | --- |
| `workspace` | `framework` (default), `blueprint` | Which pnpm workspace |
| `stage` | `release` (default), `version` | `version` opens the Version Packages PR. `release` builds, tests and publishes what `main` has that npm lacks |
| `publish` | `false` (default), `true` | `release` only. When false, the run stops at `pnpm publish --dry-run`. When true it publishes, and only from `main` in this repo, never from a fork |
| `prepare_blueprint` | `true` (default) | After a framework publish, wait for npm, then open the blueprint Version Packages PR |

The order:

1. Merge the feature PRs with their `.changeset/*.md` files into `main`.
   Writing changesets stays a human step (see step 1 below).
2. `workspace=framework stage=version`: the run does install
   `--frozen-lockfile`, `pnpm cache delete '*'`, `pnpm up:packages`, build and
   tests, `changeset version`, `pnpm install`, then build. It opens
   `chore(release): framework versions`. The PR body lists the versions it
   will publish. The run fails rather than open a PR if a new version would be
   lower than npm's `latest`.
3. Review and merge that PR.
4. `workspace=framework stage=release publish=false`: this is the dry run.
   It builds, tests, plans and runs `pnpm publish --dry-run` for every
   pending package. The plan is in the job summary and the
   `ts-release-plan-*` artifact.
5. `workspace=framework stage=release publish=true`: the run verifies again,
   then publishes. Each pending package's own `publish:package` script runs
   with `--no-git-checks --provenance`, and E409 gets retried. The run then
   polls `npm view` until every version has shown up twice in a row. After
   that it waits for npm again and opens `chore(release): blueprint versions`.
   That PR comes from `pnpm up:packages
   '--config.minimum-release-age-exclude=@forklaunch/*'`, the per-package
   tests and `changeset version`. The run fails if any blueprint reference to
   a framework package is not on the version just published. It also fails if
   `@mikro-orm/*` versions differ between the workspaces.
6. Review and merge the blueprint PR. Run `workspace=blueprint stage=release`,
   first with `publish=false`, then with `publish=true`.
7. Step 3 (CLI constants) stays manual, a day later (see Traps).

The workflow has guards. One concurrency group (`ts-release`) covers both
workspaces, so releases never interleave. GitHub keeps one queued run and
cancels older queued ones. The publish job checks that the commit is on
`main`. The version job fails if the commands touched files outside the
workspace. The workflow never edits manifests itself. Private packages are
skipped even when they have a `publish:package` script, which fixes the
`ERR_PNPM_RECURSIVE_RUN_FIRST_FAIL` trap.

The repository needs this setup:

- **`NPM_TOKEN` secret (required).** An npm granular or automation token
  with publish rights on `@forklaunch/*`. An automation token avoids the 2FA
  prompt. Only the `publish` job reads it.
- **`npm-publish` environment.** It is created on first use. Add required
  reviewers and restrict it to `main` to put an approval in front of npm.
- **Settings > Actions > General > "Allow GitHub Actions to create and
  approve pull requests"**, so the workflow can open the Version Packages PRs.
- **`RELEASE_PR_TOKEN` secret (optional).** A fine-grained PAT with Contents
  and Pull requests write access. Without it the PRs come from
  `GITHUB_TOKEN`, and GitHub does not run `pull_request` CI on them. To get
  checks, close and reopen the PR, or push to it.

The helper scripts live in `.github/workflows/scripts/ts_release/`. They also
work locally and only read from npm:

```bash
node .github/workflows/scripts/ts_release/ts_release.mjs plan framework        # what npm lacks
node .github/workflows/scripts/ts_release/ts_release.mjs wait framework        # poll npm view
node .github/workflows/scripts/ts_release/ts_release.mjs check-pickup framework blueprint
node .github/workflows/scripts/ts_release/ts_release.mjs check-mikro-orm framework blueprint
node .github/workflows/scripts/ts_release/ts_release.mjs publish framework --dry-run
```

If CI is unavailable, or a run fails halfway, use the manual steps below.
They are the same commands. To finish a half-published run, just re-run
`stage=release publish=true`. Versions that npm already has are skipped.

## Manual fallback

### 0. Release from one branch

Publish from one branch that holds everything going out: `main`, or a release
branch merged from `main` plus the PRs in the release. npm publishes cannot be
undone. A version published from an unmerged branch is the version everyone
installs.

Check npm auth first:

```bash
npm whoami            # must print the publishing account
```

### 1. Framework

```bash
cd framework
pnpm install --frozen-lockfile
pnpm cache delete '*'                            # fresh registry metadata (see Traps)
pnpm up:packages --config.fetch-full-metadata=true   # latest *mature* versions; full metadata carries publish times (see Traps)
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

### 2. Blueprint

```bash
cd blueprint
pnpm install --frozen-lockfile
pnpm cache delete '*'                             # see "Traps"
# Two passes. The blueprint has no window of its own, and in pnpm 11.1 setting
# minimum-release-age-exclude switches the window off for every package, so
# third-party packages go first with the window on and @forklaunch/* after.
# Stripe stays on its current major unless the release is about Stripe.
pnpm -r update --latest '!uWebSockets.js' '!stripe' '!@forklaunch/*' \
  --config.minimum-release-age=1440 --config.fetch-full-metadata=true
pnpm -r update --latest '@forklaunch/*' --config.minimum-release-age=1440 \
  '--config.minimum-release-age-exclude=@forklaunch/*' --config.fetch-full-metadata=true
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

- **pnpm 11 `minimumReleaseAge` (about a day), and generated apps enforce it too.**
  - Never turn the window off (`--config.minimum-release-age=0`). Every third-party version it lets in ends up in the CLI's constants, and apps generated by the CLI run pnpm and bun with the same one-day window. They then fail to install (`ERR_PNPM_NO_MATURE_MATCHING_VERSION`, bun's "blocked by minimum-release-age"). The CLI e2e tests fail on every PR until those versions are a day old.
  - Exempt only our own scope in the blueprint: `'--config.minimum-release-age-exclude=@forklaunch/*'`.
  - The window is skipped for packages whose cached metadata lacks a `time` field, which is how the framework and blueprint once resolved different `@mikro-orm` versions. Clear the cache first (`pnpm cache delete '*'`) in both workspaces, so the window applies the same way in each.
  - Our own freshly published packages are also inside the window for generated apps. **Cut the CLI release, and expect the CLI e2e jobs to pass, only a day after publishing.** A PR that bumps the CLI constants shows red e2e until then; re-run it the next day.
- **pnpm's metadata cache.** It can miss a version published minutes ago, or lack publish times. `pnpm cache delete '*'` is not enough: pnpm refills the cache with abbreviated metadata that has no `time`, warns "missing the time field; skipping the minimumReleaseAge check", and installs versions hours old (2026-10-20: jose 13h, fastmcp 3h). Pass `--config.fetch-full-metadata=true` to every `update`, and treat any such warning as a failed run.
- **Check ages after every update.** Diff the manifests and `npm view <pkg> time` each changed version; anything under 24h means restore and rerun. The lockfile can still pick up young indirect versions in the `@forklaunch/*` pass; a young `@opentelemetry/api` there gave vitest two copies and every test failed with "Cannot read properties of undefined (reading 'config')". Restore the lockfile from git and `pnpm install --no-frozen-lockfile --config.optimistic-repeat-install=false` with the window on, so locked versions are reused.
- **Stale vitest cache.** The same "reading 'config'" error can outlive the fix, tied to a test's path (an identical copy of the file passes). Move `node_modules/.vitest-cache` and each package's `node_modules/.vite` aside and rerun.
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
