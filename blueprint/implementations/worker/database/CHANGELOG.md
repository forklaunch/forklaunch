# @forklaunch/implementation-worker-database

## 1.0.35

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

## 1.0.34

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

- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.31

## 1.0.33

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

- Updated dependencies [927378c]
  - @forklaunch/interfaces-worker@1.0.30

## 1.0.28

### Patch Changes

- package bump
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.26

## 1.0.27

### Patch Changes

- internal package upgrade
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.25

## 1.0.26

### Patch Changes

- Internal package updates
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.24

## 1.0.25

### Patch Changes

- chore: update internal packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.23

## 1.0.24

### Patch Changes

- chore: bump internal package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.22

## 1.0.23

### Patch Changes

- package version bumps
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.21

## 1.0.22

### Patch Changes

- update internal packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.20

## 1.0.21

### Patch Changes

- update packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.19

## 1.0.20

### Patch Changes

- update packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.18

## 1.0.19

### Patch Changes

- update package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.17

## 1.0.18

### Patch Changes

- update packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.16

## 1.0.17

### Patch Changes

- bump packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.15

## 1.0.16

### Patch Changes

- update packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.14

## 1.0.15

### Patch Changes

- upgrade packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.13

## 1.0.14

### Patch Changes

- package bumps
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.12

## 1.0.13

### Patch Changes

- update packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.11

## 1.0.12

### Patch Changes

- update fl packages

## 1.0.11

### Patch Changes

- package version increase
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.10

## 1.0.10

### Patch Changes

- shorter brand for compliance entities

## 1.0.9

### Patch Changes

- update packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.9

## 1.0.8

### Patch Changes

- update internal packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.8

## 1.0.7

### Patch Changes

- upgrade packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.7

## 1.0.6

### Patch Changes

- package updates
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.6

## 1.0.5

### Patch Changes

- update packages
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.5

## 1.0.4

### Patch Changes

- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.4

## 1.0.3

### Patch Changes

- Worker decryption and encryption update
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.3

## 1.0.2

### Patch Changes

- Package updates
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.2

## 1.0.1

### Patch Changes

- update internal package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.1

## 1.0.0

### Major Changes

- Compliance framework installed

### Patch Changes

- Updated dependencies
  - @forklaunch/interfaces-worker@1.0.0

## 0.9.0

### Minor Changes

- MikroOrm v7 upgrade

### Patch Changes

- Updated dependencies
  - @forklaunch/interfaces-worker@0.8.0

## 0.8.23

### Patch Changes

- package bumps
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.23

## 0.8.22

### Patch Changes

- fix mikro-orm
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.22

## 0.8.21

### Patch Changes

- internal bump
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.21

## 0.8.20

### Patch Changes

- revert mikroorm version
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.20

## 0.8.19

### Patch Changes

- Package bumps
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.19

## 0.8.18

### Patch Changes

- Package version bumps
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.18

## 0.8.17

### Patch Changes

- package bump
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.17

## 0.8.16

### Patch Changes

- small nits
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.16

## 0.8.15

### Patch Changes

- Update internal package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.15

## 0.8.14

### Patch Changes

- bump package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.14

## 0.8.13

### Patch Changes

- package bump
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.13

## 0.8.12

### Patch Changes

- minor typing updates
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.12

## 0.8.11

### Patch Changes

- update package dependency versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.11

## 0.8.10

### Patch Changes

- Update package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.10

## 0.8.9

### Patch Changes

- dependency upgrade and minor metadata fixes
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.9

## 0.8.8

### Patch Changes

- Mapper syntax fixed and package bump
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.8

## 0.8.7

### Patch Changes

- Update package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.7

## 0.8.6

### Patch Changes

- update framework package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.6

## 0.8.5

### Patch Changes

- update package dependency versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.5

## 0.8.4

### Patch Changes

- Update internal packages
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.4

## 0.8.3

### Patch Changes

- update dependency versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.3

## 0.8.2

### Patch Changes

- update internal package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.2

## 0.8.1

### Patch Changes

- Update internal packages
- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.1

## 0.8.0

### Minor Changes

- Update service implementations and add tests

### Patch Changes

- Updated dependencies
  - @forklaunch/interfaces-worker@0.7.0

## 0.7.3

### Patch Changes

- export internal transitive dependencies
- Updated dependencies
  - @forklaunch/interfaces-worker@0.6.3

## 0.7.2

### Patch Changes

- package upgrade
- Updated dependencies
  - @forklaunch/interfaces-worker@0.6.2

## 0.7.1

### Patch Changes

- Internal package dependency bump
- Updated dependencies
  - @forklaunch/interfaces-worker@0.6.1

## 0.7.0

### Minor Changes

- Major universal sdk refactor, better handler ergonomics and update internal package versions

### Patch Changes

- Updated dependencies
  - @forklaunch/interfaces-worker@0.6.0

## 0.6.4

### Patch Changes

- Use more appropriate auth apis for use with auth around the app
- Updated dependencies
  - @forklaunch/interfaces-worker@0.5.4

## 0.6.3

### Patch Changes

- Bump internal package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.5.3

## 0.6.2

### Patch Changes

- update internal packages
- Updated dependencies
  - @forklaunch/interfaces-worker@0.5.2

## 0.6.1

### Patch Changes

- Much cleaner schema syntax for registrations
- Updated dependencies
  - @forklaunch/interfaces-worker@0.5.1

## 0.6.0

### Minor Changes

- new mapper syntax, and cleaner logic

### Patch Changes

- Updated dependencies
  - @forklaunch/interfaces-worker@0.5.0

## 0.5.8

### Patch Changes

- internal package upgrades
- Updated dependencies
  - @forklaunch/interfaces-worker@0.4.7

## 0.5.7

### Patch Changes

- internal package bump
- Updated dependencies
  - @forklaunch/interfaces-worker@0.4.6

## 0.5.6

### Patch Changes

- upgrade internal packages
- Updated dependencies
  - @forklaunch/interfaces-worker@0.4.5

## 0.5.5

### Patch Changes

- upgrade internal packages
- Updated dependencies
  - @forklaunch/interfaces-worker@0.4.4

## 0.5.4

### Patch Changes

- internal package updates
- Updated dependencies
  - @forklaunch/interfaces-worker@0.4.3

## 0.5.3

### Patch Changes

- update internal versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.4.2

## 0.5.2

### Patch Changes

- bump internal package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.4.1

## 0.5.1

### Patch Changes

- update package versions

## 0.5.0

### Minor Changes

- pin forklaunch package to patch versions

### Patch Changes

- Updated dependencies
  - @forklaunch/interfaces-worker@0.4.0

## 0.4.1

### Patch Changes

- Fix: find args for database worker

## 0.4.0

### Minor Changes

- change implementation structure, and update internal dependencies, SdkClient breaking changes

### Patch Changes

- Updated dependencies
  - @forklaunch/interfaces-worker@0.3.0

## 0.3.8

### Patch Changes

- update internal dependencies

## 0.3.7

### Patch Changes

- update package dependencies
- Updated dependencies
  - @forklaunch/interfaces-worker@0.2.5

## 0.3.6

### Patch Changes

- remove enums from implementations for erasable syntax
- Updated dependencies
  - @forklaunch/interfaces-worker@0.2.4

## 0.3.5

### Patch Changes

- bump internal package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.2.3

## 0.3.4

### Patch Changes

- fix import paths from schemas to other domain layers

## 0.3.3

### Patch Changes

- move domain files into domain folder

## 0.3.2

### Patch Changes

- update dependencies and introduce stripe billing
- Updated dependencies
  - @forklaunch/interfaces-worker@0.2.2

## 0.3.1

### Patch Changes

- export mjs and bump packages
- Updated dependencies
  - @forklaunch/interfaces-worker@0.2.1

## 0.3.0

### Minor Changes

- create mjs and cjs artifacts for blueprint items

### Patch Changes

- Updated dependencies
  - @forklaunch/interfaces-worker@0.2.0

## 0.2.3

### Patch Changes

- package version bump
- Updated dependencies
  - @forklaunch/interfaces-worker@0.1.11

## 0.2.2

### Patch Changes

- update package version
- Updated dependencies
  - @forklaunch/interfaces-worker@0.1.10

## 0.2.1

### Patch Changes

- increase package dependencies version
- Updated dependencies
  - @forklaunch/interfaces-worker@0.1.9

## 0.2.0

### Minor Changes

- consume async mappers now

## 0.1.9

### Patch Changes

- Get env var movement
- Updated dependencies
  - @forklaunch/interfaces-worker@0.1.8

## 0.1.8

### Patch Changes

- upgrade package dependency versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.1.7

## 0.1.7

### Patch Changes

- increase package versions
- Updated dependencies
  - @forklaunch/interfaces-worker@0.1.6

## 0.1.6

### Patch Changes

- Upgrade dependencies
- Updated dependencies
  - @forklaunch/interfaces-worker@0.1.5

## 0.1.5

### Patch Changes

- Options export name change

## 0.1.4

### Patch Changes

- Fix bug where files not uploading to npm
- Updated dependencies
  - @forklaunch/interfaces-worker@0.1.4

## 0.1.3

### Patch Changes

- Replace link dependencies, hotfix
- Updated dependencies
  - @forklaunch/interfaces-worker@0.1.3

## 0.1.2

### Patch Changes

- Package bump and minor syntax changes
- Updated dependencies
  - @forklaunch/interfaces-worker@0.1.2

## 0.1.1

### Patch Changes

- Minor bugfixes and package version increases
- Updated dependencies
  - @forklaunch/interfaces-worker@0.1.1

## 0.1.9

### Patch Changes

- minor build bug -- script was not being used
- Updated dependencies
  - @forklaunch/interfaces-billing@0.1.9

## 0.1.8

### Patch Changes

- bump core dependency version and change packaging scripts
- Updated dependencies
  - @forklaunch/interfaces-billing@0.1.8

## 0.1.7

### Patch Changes

- Keyword argument schema validation for better ergonomics with options
- Updated dependencies
  - @forklaunch/interfaces-billing@0.1.7

## 0.1.6

### Patch Changes

- Change package exports
- Updated dependencies
  - @forklaunch/interfaces-billing@0.1.6

## 0.1.5

### Patch Changes

- Path based exports
- Updated dependencies
  - @forklaunch/interfaces-billing@0.1.5
