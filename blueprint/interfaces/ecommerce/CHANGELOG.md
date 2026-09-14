# @forklaunch/interfaces-ecommerce

## 1.0.8

### Patch Changes

- Pin `@mikro-orm/*` to an exact 7.1.15 so only one copy of `@mikro-orm/core`
  resolves in a consumer's tree.

  `interfaces-*` and `implementation-*-base` pinned `@mikro-orm/core` at exactly
  `7.1.14`, while `@forklaunch/core`, `internal` and `testing` ranged on
  `^7.1.14`. When MikroORM published 7.1.15 the carets took it and these exact
  pins did not, so every generated app installed **two copies of
  `@mikro-orm/core`** and stopped compiling:

      error TS2741: Property '#private' is missing in type
        'PostgreSqlEntityManager<PostgreSqlDriver>' but required in 'EntityManager'
      error TS2345: Argument of type 'EntitySchemaWithMeta<"Plan", ...>' is not
        assignable to parameter of type 'EntityName<any>'

  `EntityManager` and `EntitySchema` carry a `#private` field, which TypeScript
  treats as a per-class brand, so the same class from two copies is structurally
  incompatible. Both errors are duplicated-package reports rather than real type
  errors -- `EntityName` and `EntitySchema` are byte-identical between 7.1.14 and
  7.1.15, and nothing in the source needed to change.

  Every `@mikro-orm/*` spec across framework and blueprint is now the same exact
  version, so a future MikroORM patch cannot split the tree by moving one half of
  it. Released together with `@forklaunch/core` 1.5.18, `internal` 1.2.28 and
  `testing` 1.2.31, which carry the same pin.

## 1.0.7

### Patch Changes

- Refresh dependencies to their latest published versions and drop the pnpm
  overrides block.

  Consumes the newly published framework packages (@forklaunch/core 1.5.17,
  validator 1.2.26, common 1.2.25, express and hyper-express 1.2.42, internal
  1.2.27, universal-sdk 1.2.26, infrastructure-redis and -s3 1.4.12, testing
  1.2.30, ws 1.2.40, bunrun 1.2.23) along with MikroORM 7.1.14, stripe 22.6.0,
  zod 4.5.4, jose 6.2.10, uuid 14.0.2 and vitest 4.1.11.

  The `overrides` block is gone. It had pinned @mikro-orm/* to 7.1.13 to keep a
  single copy resolving workspace-wide, and pinned @forklaunch/core to a floor
  that silently held it back -- the override replaces the requested range, so core
  stayed on 1.5.16 no matter what the manifests asked for. Every package now
  declares the versions it actually wants and resolution agrees without help:
  one copy each of @mikro-orm/core, @forklaunch/core, validator and common.

  Three source changes were required by the upgrades:

  - MikroORM 7.1.14 made a MikroORM instance's entity list `readonly`, so the
    local `clearDatabase` helpers no longer accepted the orm they are handed.
    They now type that parameter as `TestSetupResult['orm']`, matching both the
    value's real origin and the adjacent `redis` field, instead of a bare
    `MikroORM` whose type argument defaulted to a mutable array. Six test-utils
    files across billing, iam, messaging and sample-worker.
  - stripe 22.6.0 moved its pinned API version literal, so the two billing-stripe
    scripts now request '2026-08-26.dahlia'.
  - `@forklaunch/blueprint-core` had to be rebuilt from clean. Its gitignored
    `lib/` still held declarations emitted against an older core, in which
    `.compliance('none')` produced a `'~c': true` marker rather than a
    `ComplianceLevel`. That stale output alone accounted for 13 of the 17
    compile errors this upgrade first surfaced, none of which were real.

## 1.0.6

### Patch Changes

- 927378c: Declare uWebSockets.js directly and align first-party dependency versions.

  `@forklaunch/hyper-express` and `@forklaunch/hyper-express-fork` now take
  uWebSockets.js as a peer rather than a dependency. pnpm 11 refuses a
  git-resolved package when it arrives as a subdependency and permits it as a
  direct one, and uWebSockets.js is only ever installed from its GitHub tarball —
  so every package depending on either one has to declare it itself. That covers
  packages depending on the fork directly, not only on the wrapper.

  The first-party `@forklaunch/*` ranges move to the currently published versions
  in the same pass. They had to: taking the new hyper-express on its own left two
  copies of `@forklaunch/validator` in the tree, and TypeScript rejected the
  result outright rather than picking one —

      error TS2883: The inferred type of 'BaseUserServiceSchemas' cannot be named
      without a reference to 'AnySchemaValidator' from
      '.pnpm/@forklaunch+validator@1.2.25/...'. This is likely not portable.

  The mechanism is worth remembering, because a version range that already
  tolerates the newer release is exactly the case where nothing forces a
  re-resolution: the lockfile keeps the old version for the packages that declare
  it, while a newly added dependency resolves the same range to the new one, and
  the two copies coexist until something type-checks across them.

## 1.0.1

### Patch Changes

- package bump
