# @forklaunch/core

## 1.6.4

### Patch Changes

- `ResolvedRelation` passes `any` through instead of treating it as an entity. A shape
  entity that declares `fp.enum()` without naming the enum infers the field as `any`, and
  `keyof any` contains every symbol, so the previous check saw an entity there and produced
  an index-signature type the app's real enum could not satisfy.

## 1.6.3

### Patch Changes

- **`ResolvedEntity` now resolves relation targets, so module entity constraints keep working on mikro-orm 7.2.**

  mikro-orm 7.2 declares its `defineEntity` property builders invariant (`in out`). The
  builder record an inferred entity carries in its `IndexHints` slot therefore no longer
  unifies between two definitions of "the same" entity: a module's minimal `Permission`
  (`id`, `slug`) and an application's real one (`id` generated `onCreate`, timestamps,
  `.unique()` on `slug`). `ResolvedEntity` already dropped that slot on the entity being
  compared, but not on the entities behind its relations, so any constraint that crossed a
  `Collection<Permission>` or a `manyToOne(Organization)` failed to compile
  (`Type 'RoleMapperTypes' does not satisfy the constraint 'RoleEntities'` in the IAM
  module).

  `ResolvedEntity<T>` now maps `Collection<E>` to `Collection<ResolvedEntity<E>>`,
  `Reference<E>` to `Reference<ResolvedEntity<E>>`, and a bare related entity to
  `ResolvedEntity<E>`; scalars, dates, enums and `null`/`undefined` pass through. The new
  `ResolvedRelation<V>` helper is exported for the per-field case. Type tests cover a
  to-many, a nullable to-one, and the negative case.

## 1.6.2

### Patch Changes

- Refresh dependencies to their latest published versions.

  `@mikro-orm/*` moves from 7.1.15 to 7.2.1 in every package that pins it,
  as one step: the framework, the blueprint and the CLI's scaffold constants
  all agree on a single MikroORM version, so a freshly generated app resolves
  exactly one copy (the duplicate-package type errors from mixed pins are the
  reason it is pinned exactly). `@aws-sdk/client-s3` 3.1131 → 3.1136 in
  infrastructure-s3. The rest is devDependency movement; `@types/node` 26.6
  added `Socket.server`, which the Bun socket shim in express now declares.

  Packages with only devDependency changes release too, so the whole
  framework carries one MikroORM version on npm.

- Updated dependencies
  - @forklaunch/validator@1.2.28
  - @forklaunch/common@1.2.27

## 1.6.1

### Patch Changes

- **`wrapEmWithTenantContext(em, '')` now binds the empty tenant, and never leaks a tenant into the caller.**

  Global rows (a billing plan, a trial, a template) are encrypted under the
  empty tenant. Two defects together made reading or writing them depend on
  what had run earlier on the same worker:

  - `''` was treated like `undefined` ("do not wrap"), so a caller asking for
    the no-tenant key got an entity manager bound to nothing.
  - The wrapper called `setEncryptionTenantId`, whose `enterWith` mutates the
    calling async resource. After one org-scoped EM was created on a request,
    a later "no-tenant" EM silently read and wrote under that org's key.

  The visible symptom was intermittent `Failed to decrypt encrypted column
value` on rows that were encrypted correctly. Now `''` returns a Proxy that
  runs every EM call inside `withEncryptionContext('', …)`, only `undefined`
  skips wrapping, the tenant filter is set only for a real tenant id, and the
  wrapper no longer touches the caller's context. Tests cover all four cases.

## 1.6.0

### Minor Changes

- **Encryption key ring and rotation sweep.**

  `FieldEncryptor` now holds a ring: the current key plus any number of
  previous keys (`new FieldEncryptor(key, { previousKeys })`, or
  `FieldEncryptor.fromEnv()` reading `ENCRYPTION_KEY` and
  `LEGACY_ENCRYPTION_KEYS`). Writes use the current key; reads try the current
  key and then each previous key, so `ENCRYPTION_KEY` can change without a
  downtime window. Single-key behaviour and the on-disk `v2:` format are
  unchanged.

  - `open()` reports which key opened a value (by fingerprint) and whether a
    rewrite would change it; `needsRotation()`, `rotate()`, `keyIds`,
    `withPreviousKeys()`, `withFormat()`.
  - New `v3:{keyId}:{iv}:{tag}:{data}` envelope, opt-in via
    `ENCRYPTION_FORMAT=v3`: reads resolve the key directly, a missing key fails
    by name, and `countValuesByKeyId()` answers "can this key be dropped?"
    without decrypting. Every reader (`EncryptedType`, the redis cache, the S3
    store) accepts all three envelopes.
  - `reencryptEncryptedColumns()` is the rotation sweep for migrations: walks
    every entity with `pii`/`phi`/`pci` fields, rewrites what is still under a
    previous key with the same tenant it was written with, tries every known
    organization as a fallback tenant, and reports per table.

  See `docs/compliance/key-rotation.md` for the three-step rotation.

## 1.5.20

### Patch Changes

- **Security:** authorization failures no longer log the credential.

  `parseRequestAuth` wrote the raw `Authorization` header value into the
  "JWT Verification Failed" and "Authorization Failed" log lines. For bearer
  auth that is a token that stays valid until it expires; for basic auth it is
  a password. Affected: every release up to and including 1.5.19. If your
  application logs are shipped outside your own infrastructure, rotate any
  token that failed authorization while on an affected version.

  The log line now carries what the credential was for, not the credential:

  - `reason`: `jwt_expired`, `jwt_bad_signature`, `jwt_malformed`,
    `jwks_no_key`, `jwks_unavailable`, `jwt_claim_<name>`. Previously an
    expired token and a forged one produced identical lines.
  - `claimed`: the decoded, **unverified** `sub`, `organizationId`, `iss`,
    `exp` and `kid`. Labelled `claimed` because on a failed verification they
    are whatever the caller wrote. `email` is deliberately omitted.
  - `tokenFingerprint`: the first 8 hex characters of SHA-256 of the header
    value, to correlate retries and match a token you hold against a line.
    Not reversible.
  - `hasToken`.

  Both lines move from `error` to `warn`: an expired token is normal traffic.

  The JWKS verification path now surfaces the last `jose` error instead of
  swallowing it, so `reason` is populated for `jwksPublicKeyUrl` consumers.
  Status codes and response bodies are unchanged.

## 1.5.19

### Patch Changes

- Re-emit the intra-framework dependency ranges so `core` and `internal` track
  the rest of the set.

  Both declare `@forklaunch/common` and `@forklaunch/validator` as `workspace:^`,
  which is frozen into a concrete range at publish time. They were published one
  wave ahead of `common` and `validator`, so they went out pinned to the previous
  pair. A consumer then resolved two copies of each — and duplicated packages are
  the whole class of bug this release exists to remove.

  No source changes; this republishes them against the current set.

## 1.5.18

### Patch Changes

- Pin `@mikro-orm/*` to an exact version instead of a caret range.

  These three packages ranged on `^7.1.14` while `@forklaunch/interfaces-*` and
  `@forklaunch/implementation-*-base` pinned `7.1.14` exactly. When MikroORM
  published 7.1.15 the carets took it and the exact pins did not, so every
  consumer resolved **two copies of `@mikro-orm/core`**.

  That is not a harmless duplication. `EntityManager` and `EntitySchema` carry a
  `#private` field, which TypeScript treats as a per-class brand, so the same
  class coming from two copies is structurally incompatible and every generated
  app stops compiling:

      error TS2741: Property '#private' is missing in type
        'PostgreSqlEntityManager<PostgreSqlDriver>' but required in 'EntityManager'
      error TS2883: The inferred type of 'ci' cannot be named without a reference
        to 'Connection' from '.bun/@mikro-orm+core@7.1.14/node_modules/@mikro-orm/core'

  7.1.15 itself is not a breaking change — `EntityName` and `EntitySchema` are
  byte-identical to 7.1.14. Only the duplication broke.

  An exact pin here matches what the rest of the family already does, so a future
  MikroORM patch cannot split the tree again by moving one half of it.

## 1.5.17

### Patch Changes

- Refresh dependencies to their latest published versions.

  Runtime dependency changes, which is why these five packages release rather
  than the whole workspace: `zod` 4.4.3 → 4.5.4 (core, validator), `fastmcp`
  4.16.10 → 4.17.1 (core, express), `qs` 6.15.3 → 6.16.0 and
  `@scalar/express-api-reference` 0.10.16 → 0.10.17 (express, hyper-express),
  `multer` 2.2.0 → 2.3.0 (express), and `@aws-sdk/client-s3` 3.1120.0 → 3.1121.0
  (infrastructure-s3). All are patch or minor upstream releases with no API
  change on our side; the build and test suites pass unmodified.

  The remaining packages only saw devDependency movement (`jest` 30.4.2 → 30.5.0,
  `tsx` 4.23.12 → 4.23.13), which no consumer installs, so they are not released.

  `jest` 30.5.0 pulls in `@parcel/watcher` as a new transitive dependency, and
  pnpm requires an explicit build decision for it. It is set to `false` in
  `pnpm-workspace.yaml`: it arrives only through `jest-haste-map`, so it is
  dev-only and never reaches a published package, and the platform prebuilt
  binary is already resolved, so the native build script has nothing to add.
  Without that entry `pnpm install` fails outright — pnpm writes a literal
  `set this to true or false` placeholder into the file, which is not valid
  configuration.

- Updated dependencies
  - @forklaunch/validator@1.2.26

## 1.5.16

### Patch Changes

- Update internal package versions
- Updated dependencies
  - @forklaunch/common@1.2.24
  - @forklaunch/validator@1.2.25

## 1.5.9

### Patch Changes

- restrict wildcard subpath exports to the types condition (no runtime targets are emitted for deep files)

## 1.5.8

### Patch Changes

- add wildcard subpath exports so per-file declaration output is addressable by consumers

## 1.5.7

### Patch Changes

- publish internal @forklaunch dependencies as caret ranges (workspace:^) instead of exact pins

## 1.5.6

### Patch Changes

- accept structural ORM types in setupRls, ComplianceDataService, and RetentionService (MikroORM v7 init returns a readonly entities array that a bare MikroORM parameter rejects)

## 1.5.5

### Patch Changes

- TypeScript 7 build pipeline: tsgo declaration emit replaces tsup --dts
- BatchLogRecordProcessor options-object API (@opentelemetry/sdk-logs 0.221)

## 1.5.4

### Patch Changes

- update packages
- Updated dependencies
  - @forklaunch/validator@1.2.20
  - @forklaunch/common@1.2.20

## 1.5.3

### Patch Changes

- 92c06f9: dep upgrades
- Updated dependencies [92c06f9]
  - @forklaunch/validator@1.2.19
  - @forklaunch/common@1.2.19

## 1.5.2

### Patch Changes

- update dependency versions
- Updated dependencies
  - @forklaunch/validator@1.2.18
  - @forklaunch/common@1.2.18

## 1.5.1

### Patch Changes

- Update internal versions and allow ZodType early release
- Updated dependencies
  - @forklaunch/validator@1.2.17
  - @forklaunch/common@1.2.17

## 1.5.0

### Minor Changes

- Export wrapEmWithTenantContext for tenant based filtering

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@1.2.16
  - @forklaunch/common@1.2.16

## 1.4.1

### Patch Changes

- chore: update internal package versions
- Updated dependencies
  - @forklaunch/validator@1.2.15
  - @forklaunch/common@1.2.15

## 1.4.0

### Minor Changes

- Encryption and decryption now take tenant id as a first party compliance input

## 1.3.17

### Patch Changes

- update enum logic
- Updated dependencies
  - @forklaunch/validator@1.2.14
  - @forklaunch/common@1.2.14

## 1.3.16

### Patch Changes

- Update packages and enum constraint fix
- Updated dependencies
  - @forklaunch/validator@1.2.13
  - @forklaunch/common@1.2.13

## 1.3.15

### Patch Changes

- sync changes across packages
- Updated dependencies
  - @forklaunch/validator@1.2.12
  - @forklaunch/common@1.2.12

## 1.3.14

### Patch Changes

- Handle enums more robustly for ORM

## 1.3.13

### Patch Changes

- no more private fields

## 1.3.12

### Patch Changes

- Turn private into public members

## 1.3.11

### Patch Changes

- exhaustively cover serde

## 1.3.10

### Patch Changes

- Align package vers
- Updated dependencies
  - @forklaunch/validator@1.2.11
  - @forklaunch/common@1.2.11

## 1.3.9

### Patch Changes

- Only validate on real path, not openapi path

## 1.3.8

### Patch Changes

- fix nested app and router
- Updated dependencies
  - @forklaunch/validator@1.2.10
  - @forklaunch/common@1.2.10

## 1.3.7

### Patch Changes

- Perf improvement
- Updated dependencies
  - @forklaunch/validator@1.2.9
  - @forklaunch/common@1.2.9

## 1.3.6

### Patch Changes

- bump package versions
- Updated dependencies
  - @forklaunch/validator@1.2.8
  - @forklaunch/common@1.2.8

## 1.3.5

### Patch Changes

- Access auth respected now

## 1.3.4

### Patch Changes

- export consolidated retention logic
- Updated dependencies
  - @forklaunch/validator@1.2.7
  - @forklaunch/common@1.2.7

## 1.3.3

### Patch Changes

- accept custom relation and relation in tenant filtering

## 1.3.2

### Patch Changes

- Encryptor required on redis and s3
- Updated dependencies
  - @forklaunch/validator@1.2.6
  - @forklaunch/common@1.2.6

## 1.3.1

### Patch Changes

- Make private fields respect interfaces
- Updated dependencies
  - @forklaunch/validator@1.2.5
  - @forklaunch/common@1.2.5

## 1.3.0

### Minor Changes

- Custom Entity type for encrypted fields

## 1.2.9

### Patch Changes

- up packages
- Updated dependencies
  - @forklaunch/validator@1.2.4
  - @forklaunch/common@1.2.4

## 1.2.8

### Patch Changes

- encrypted level for pii too

## 1.2.7

### Patch Changes

- decryption with tenant id = null

## 1.2.6

### Patch Changes

- tenant id detail

## 1.2.5

### Patch Changes

- update packages
- Updated dependencies
  - @forklaunch/validator@1.2.3
  - @forklaunch/common@1.2.3

## 1.2.4

### Patch Changes

- update tenant filter

## 1.2.3

### Patch Changes

- tenant and rls configuration
- Updated dependencies
  - @forklaunch/validator@1.2.2
  - @forklaunch/common@1.2.2

## 1.2.2

### Patch Changes

- fix compliance entity
- Updated dependencies
  - @forklaunch/validator@1.2.1
  - @forklaunch/common@1.2.1

## 1.2.1

### Patch Changes

- ConfigInjector undefined vs defined

## 1.2.0

### Minor Changes

- Validator 25% performance uptick and cleaner Config Injector syntax

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@1.2.0
  - @forklaunch/common@1.2.0

## 1.1.8

### Patch Changes

- Simplify property chain for easier consumption
- Updated dependencies
  - @forklaunch/validator@1.1.8
  - @forklaunch/common@1.1.8

## 1.1.7

### Patch Changes

- More relations covered for compliance entity
- Updated dependencies
  - @forklaunch/validator@1.1.7
  - @forklaunch/common@1.1.7

## 1.1.6

### Patch Changes

- Restore MaybeOpt
- Updated dependencies
  - @forklaunch/validator@1.1.6
  - @forklaunch/common@1.1.6

## 1.1.5

### Patch Changes

- cross boundary inference fix compliance entities
- Updated dependencies
  - @forklaunch/validator@1.1.5
  - @forklaunch/common@1.1.5

## 1.1.4

### Patch Changes

- improve performance of entity branding
- Updated dependencies
  - @forklaunch/validator@1.1.4
  - @forklaunch/common@1.1.4

## 1.1.3

### Patch Changes

- Package versions and simplified compliance entity typing
- Updated dependencies
  - @forklaunch/validator@1.1.3
  - @forklaunch/common@1.1.3

## 1.1.2

### Patch Changes

- move FieldEncryptor into persistence (previously not exported)
- Updated dependencies
  - @forklaunch/validator@1.1.2
  - @forklaunch/common@1.1.2

## 1.1.1

### Patch Changes

- add compliance utilities
- Updated dependencies
  - @forklaunch/validator@1.1.1
  - @forklaunch/common@1.1.1

## 1.1.0

### Minor Changes

- retention policy update

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@1.1.0
  - @forklaunch/common@1.1.0

## 1.0.13

### Patch Changes

- patch working"
- Updated dependencies
  - @forklaunch/validator@1.0.13
  - @forklaunch/common@1.0.13

## 1.0.12

### Patch Changes

- try removing return type to let inference take over
- Updated dependencies
  - @forklaunch/validator@1.0.12
  - @forklaunch/common@1.0.12

## 1.0.11

### Patch Changes

- refinement
- Updated dependencies
  - @forklaunch/validator@1.0.11
  - @forklaunch/common@1.0.11

## 1.0.10

### Patch Changes

- store property as internal property instead of branding
- Updated dependencies
  - @forklaunch/validator@1.0.10
  - @forklaunch/common@1.0.10

## 1.0.9

### Patch Changes

- branding fixes
- Updated dependencies
  - @forklaunch/validator@1.0.9
  - @forklaunch/common@1.0.9

## 1.0.8

### Patch Changes

- remove brand from entity
- Updated dependencies
  - @forklaunch/validator@1.0.8
  - @forklaunch/common@1.0.8

## 1.0.7

### Patch Changes

- inconsistent state
- Updated dependencies
  - @forklaunch/validator@1.0.7
  - @forklaunch/common@1.0.7

## 1.0.6

### Patch Changes

- string brand instead of symbol
- Updated dependencies
  - @forklaunch/validator@1.0.6
  - @forklaunch/common@1.0.6

## 1.0.5

### Patch Changes

- entity rework
- fix compliance brands
- Updated dependencies
- Updated dependencies
  - @forklaunch/validator@1.0.5
  - @forklaunch/common@1.0.5

## 1.0.4

### Patch Changes

- Handle functional definitions on mikroorm entities
- Updated dependencies
  - @forklaunch/validator@1.0.4
  - @forklaunch/common@1.0.4

## 1.0.3

### Patch Changes

- Update packages and fix entity type
- Updated dependencies
  - @forklaunch/validator@1.0.3
  - @forklaunch/common@1.0.3

## 1.0.2

### Patch Changes

- Fix type agreement
- Updated dependencies
  - @forklaunch/validator@1.0.2
  - @forklaunch/common@1.0.2

## 1.0.1

### Patch Changes

- Version thrash
- Updated dependencies
  - @forklaunch/validator@1.0.1
  - @forklaunch/common@1.0.1

## 1.0.0

### Major Changes

- Compliance features first party

### Patch Changes

- Updated dependencies
  - @forklaunch/common@1.0.0
  - @forklaunch/validator@1.0.0

## 0.19.5

### Patch Changes

- Another fix
- Updated dependencies
  - @forklaunch/validator@0.11.5
  - @forklaunch/common@0.7.5

## 0.19.4

### Patch Changes

- correct extension for mappers
- Updated dependencies
  - @forklaunch/validator@0.11.4
  - @forklaunch/common@0.7.4

## 0.19.3

### Patch Changes

- mapper fix
- Updated dependencies
  - @forklaunch/validator@0.11.3
  - @forklaunch/common@0.7.3

## 0.19.2

### Patch Changes

- Update packages and remove EntityMapper wrapping
- Updated dependencies
  - @forklaunch/validator@0.11.2
  - @forklaunch/common@0.7.2

## 0.19.1

### Patch Changes

- package upgrades
- Updated dependencies
  - @forklaunch/validator@0.11.1
  - @forklaunch/common@0.7.1

## 0.19.0

### Minor Changes

- update packages and update to mikro orm v7

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.11.0
  - @forklaunch/common@0.7.0

## 0.18.15

### Patch Changes

- clean build
- Updated dependencies
  - @forklaunch/validator@0.10.38
  - @forklaunch/common@0.6.38

## 0.18.14

### Patch Changes

- fix mikroorm
- Updated dependencies
  - @forklaunch/common@0.6.37
  - @forklaunch/validator@0.10.37

## 0.18.13

### Patch Changes

- actually fix mikroorm

## 0.18.12

### Patch Changes

- fix mikroorm package versions

## 0.18.11

### Patch Changes

- internal package bump
- Updated dependencies
  - @forklaunch/validator@0.10.36
  - @forklaunch/common@0.6.36

## 0.18.10

### Patch Changes

- better trap on openapi ci proxy

## 0.18.9

### Patch Changes

- Make thenable fast resolve

## 0.18.8

### Patch Changes

- Downgrade mikro-orm back to normal
- Updated dependencies
  - @forklaunch/validator@0.10.35
  - @forklaunch/common@0.6.35

## 0.18.7

### Patch Changes

- bump packages and internal proxy await resilience
- Updated dependencies
  - @forklaunch/validator@0.10.34
  - @forklaunch/common@0.6.34

## 0.18.6

### Patch Changes

- proxy based injection for ci, and openapi path resiliency
- Updated dependencies
  - @forklaunch/validator@0.10.33
  - @forklaunch/common@0.6.33

## 0.18.5

### Patch Changes

- Small bugs
- Updated dependencies
  - @forklaunch/validator@0.10.32
  - @forklaunch/common@0.6.32

## 0.18.4

### Patch Changes

- Prevent 404 message hijacking and update packages
- Updated dependencies
  - @forklaunch/validator@0.10.31
  - @forklaunch/common@0.6.31

## 0.18.3

### Patch Changes

- Fix multiline config injection and update packages
- Updated dependencies
  - @forklaunch/validator@0.10.30
  - @forklaunch/common@0.6.30

## 0.18.2

### Patch Changes

- WS actually working probably, and package bumps
- Updated dependencies
  - @forklaunch/validator@0.10.29
  - @forklaunch/common@0.6.29

## 0.18.1

### Patch Changes

- 4e10567: Update dependency versions
- Updated dependencies [4e10567]
  - @forklaunch/validator@0.10.28
  - @forklaunch/common@0.6.28

## 0.18.0

### Minor Changes

- added logic in auth middleware to support billing based authorization

## 0.17.3

### Patch Changes

- Fix config propogation from app to route
- Updated dependencies
  - @forklaunch/common@0.6.27
  - @forklaunch/validator@0.10.27

## 0.17.2

### Patch Changes

- Package deps version bump
- Updated dependencies
  - @forklaunch/validator@0.10.26
  - @forklaunch/common@0.6.26

## 0.17.1

### Patch Changes

- package version bump
- Updated dependencies
  - @forklaunch/validator@0.10.25
  - @forklaunch/common@0.6.25

## 0.17.0

### Minor Changes

- Mapper instantiation syntax more readable and express port added. Also removed error schema thrash in live sdk

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.10.24
  - @forklaunch/common@0.6.24

## 0.16.1

### Patch Changes

- update framework pages
- Updated dependencies
  - @forklaunch/validator@0.10.23
  - @forklaunch/common@0.6.23

## 0.16.0

### Minor Changes

- Introduce fl websockets for easy, fully typed socket communication

## 0.15.12

### Patch Changes

- fix hyper express header, fix tests
- Updated dependencies
  - @forklaunch/validator@0.10.22
  - @forklaunch/common@0.6.22

## 0.15.11

### Patch Changes

- Update package versions, and add x-powered-by forklaunch
- Updated dependencies
  - @forklaunch/validator@0.10.21
  - @forklaunch/common@0.6.21

## 0.15.10

### Patch Changes

- update internal package versions
- Updated dependencies
  - @forklaunch/validator@0.10.20
  - @forklaunch/common@0.6.20

## 0.15.9

### Patch Changes

- Updates for openapi publishing mode

## 0.15.8

### Patch Changes

- update package versions
- Updated dependencies
  - @forklaunch/validator@0.10.19
  - @forklaunch/common@0.6.19

## 0.15.7

### Patch Changes

- update packages, make OpenTelemetryCollector type more transparent, attempt to fix error loggings
- Updated dependencies
  - @forklaunch/validator@0.10.18
  - @forklaunch/common@0.6.18

## 0.15.6

### Patch Changes

- Update internal packages
- Updated dependencies
  - @forklaunch/validator@0.10.17
  - @forklaunch/common@0.6.17

## 0.15.5

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.10.16
  - @forklaunch/common@0.6.16

## 0.15.4

### Patch Changes

- Minor bugfixes and package version bumps
- Updated dependencies
  - @forklaunch/validator@0.10.15
  - @forklaunch/common@0.6.15

## 0.15.3

### Patch Changes

- package upgrade
- Updated dependencies
  - @forklaunch/validator@0.10.14
  - @forklaunch/common@0.6.14

## 0.15.2

### Patch Changes

- upgrade package dependencies and add global options to nested routers
- Updated dependencies
  - @forklaunch/validator@0.10.13
  - @forklaunch/common@0.6.13

## 0.15.1

### Patch Changes

- Update internal packages and expose RegistryOptions from universal sdk
- Updated dependencies
  - @forklaunch/validator@0.10.12
  - @forklaunch/common@0.6.12

## 0.15.0

### Minor Changes

- Set the stage for improved universal sdk performance, and update internal packages

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.10.11
  - @forklaunch/common@0.6.11

## 0.14.16

### Patch Changes

- Update internal package versions
- Updated dependencies
  - @forklaunch/validator@0.10.10
  - @forklaunch/common@0.6.10

## 0.14.15

### Patch Changes

- expose cached jwks method

## 0.14.14

### Patch Changes

- better debugging for auth errors and fix hmac check

## 0.14.13

### Patch Changes

- missed surfaceRoles constraint loosening

## 0.14.12

### Patch Changes

- package version bump

## 0.14.11

### Patch Changes

- update internal packages and loosen global auth constraint
- Updated dependencies
  - @forklaunch/validator@0.10.9
  - @forklaunch/common@0.6.9

## 0.14.11

### Patch Changes

- update internal packages
- Updated dependencies
  - @forklaunch/validator@0.10.8
  - @forklaunch/common@0.6.8

## 0.14.10

### Patch Changes

- Allow for session schema typings

## 0.14.9

### Patch Changes

- slight hmac token creation signature change
- Updated dependencies
  - @forklaunch/validator@0.10.7
  - @forklaunch/common@0.6.7

## 0.14.8

### Patch Changes

- Update packages and expose hmac key creation function
- Updated dependencies
  - @forklaunch/validator@0.10.6
  - @forklaunch/common@0.6.6

## 0.14.7

### Patch Changes

- Remove correlation id from metrics to reduce cardinality. Metrics are no longer filterable by correlation id

## 0.14.6

### Patch Changes

- update internal packages
- Updated dependencies
  - @forklaunch/validator@0.10.5
  - @forklaunch/common@0.6.5

## 0.14.5

### Patch Changes

- extend mapServiceSchemas to accept more arguments

## 0.14.4

### Patch Changes

- Update internal package versions and add mapServiceSchemas method for clean DX in implemented modules
- Updated dependencies
  - @forklaunch/validator@0.10.4
  - @forklaunch/common@0.6.4

## 0.14.3

### Patch Changes

- toDomain -> toDto for more accurate naming conventions
- Updated dependencies
  - @forklaunch/validator@0.10.3
  - @forklaunch/common@0.6.3

## 0.14.2

### Patch Changes

- toDto -> toDomain
- Updated dependencies
  - @forklaunch/validator@0.10.2
  - @forklaunch/common@0.6.2

## 0.14.1

### Patch Changes

- request and response mapper discrimination and clean up of internal types
- Updated dependencies
  - @forklaunch/validator@0.10.1
  - @forklaunch/common@0.6.1

## 0.14.0

### Minor Changes

- remove class based mappers

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.10.0
  - @forklaunch/common@0.6.0

## 0.13.9

### Patch Changes

- add mappers as functions
- Updated dependencies
  - @forklaunch/validator@0.9.9
  - @forklaunch/common@0.5.8

## 0.13.8

### Patch Changes

- One more attempt at performance bump
- Updated dependencies
  - @forklaunch/validator@0.9.8
  - @forklaunch/common@0.5.7

## 0.13.7

### Patch Changes

- prettify req init for slightly faster sdk access
- Updated dependencies
  - @forklaunch/validator@0.9.7
  - @forklaunch/common@0.5.6

## 0.13.6

### Patch Changes

- attempt to make sdk pathing more efficient
- Updated dependencies
  - @forklaunch/validator@0.9.6
  - @forklaunch/common@0.5.5

## 0.13.5

### Patch Changes

- zod validator regex relaxation for email
- Updated dependencies
  - @forklaunch/validator@0.9.5
  - @forklaunch/common@0.5.4

## 0.13.4

### Patch Changes

- Update validator types for files to use raw streams, lazy load openapi for universal sdk, and remove private members from otel
- Updated dependencies
  - @forklaunch/validator@0.9.4
  - @forklaunch/common@0.5.3

## 0.13.3

### Patch Changes

- update package versions
- Updated dependencies
  - @forklaunch/validator@0.9.3
  - @forklaunch/common@0.5.2

## 0.13.2

### Patch Changes

- Auth fixes and add HMAC auth
- Updated dependencies
  - @forklaunch/validator@0.9.2

## 0.13.1

### Patch Changes

- bump internal packages
- Updated dependencies
  - @forklaunch/validator@0.9.1
  - @forklaunch/common@0.5.1

## 0.13.0

### Minor Changes

- Adds more configuration options for application and routers. Additionally adds optional cluster support built-in (experimental)

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.9.0
  - @forklaunch/common@0.5.0

## 0.12.3

### Patch Changes

- "bump fastmcp internal version"

## 0.12.2

### Patch Changes

- Allow for host specifier in mcp implementation

## 0.12.1

### Patch Changes

- Allows for server urls to be passed in as env vars for use with openapi

## 0.12.0

### Minor Changes

- Add more permissive body types and update schema validator types

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.8.0

## 0.11.7

### Patch Changes

- Add versions to contract details, migrate sdk and fetch to functions for much better ergonomics
- Updated dependencies
  - @forklaunch/validator@0.7.8
  - @forklaunch/common@0.4.6

## 0.11.6

### Patch Changes

- Fix universal sdk bugs and address fetch thrashing internally

## 0.11.5

### Patch Changes

- Update to zod v4, keeping zod v3 as active zod version
- Updated dependencies
  - @forklaunch/validator@0.7.7
  - @forklaunch/common@0.4.5

## 0.11.4

### Patch Changes

- Upgrade internal dependencies
- Updated dependencies
  - @forklaunch/validator@0.7.6
  - @forklaunch/common@0.4.4

## 0.11.3

### Patch Changes

- SDK client types simplified for better performance

## 0.11.2

### Patch Changes

- Lessen depth instation on Auth for better performance

## 0.11.1

### Patch Changes

- Fix auth header bugs

## 0.11.0

### Minor Changes

- Auth types are now propogated to live sdk types

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.7.5
  - @forklaunch/common@0.4.3

## 0.10.4

### Patch Changes

- remove enum from all packages for erasable syntax
- Updated dependencies
  - @forklaunch/validator@0.7.4
  - @forklaunch/common@0.4.2

## 0.10.3

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.7.3

## 0.10.2

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.7.2

## 0.10.1

### Patch Changes

- node types version upgrade
- Updated dependencies
  - @forklaunch/validator@0.7.1
  - @forklaunch/common@0.4.1

## 0.10.0

### Minor Changes

- package version upgrade, mcp generation and nicer universal sdk syntax

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.7.0
  - @forklaunch/common@0.4.0

## 0.9.22

### Patch Changes

- "move enrichment middleware to router level instead of application level"

## 0.9.21

### Patch Changes

- change dtoMapper to Mapper
- Updated dependencies
  - @forklaunch/validator@0.6.16
  - @forklaunch/common@0.3.14

## 0.9.20

### Patch Changes

- move internal utilities out of core and into internal

## 0.9.19

### Patch Changes

- remove internal type from core

## 0.9.18

### Patch Changes

- create internal package for internal utilities
- Updated dependencies
  - @forklaunch/validator@0.6.15
  - @forklaunch/common@0.3.13

## 0.9.17

### Patch Changes

- bump package subdependencies
- Updated dependencies
  - @forklaunch/validator@0.6.14
  - @forklaunch/common@0.3.12

## 0.9.16

### Patch Changes

- Slight test utility change

## 0.9.15

### Patch Changes

- update package deps
- Updated dependencies
  - @forklaunch/validator@0.6.13

## 0.9.14

### Patch Changes

- Surface error as string in OpenTelemetry Logs

## 0.9.13

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.6.12

## 0.9.12

### Patch Changes

- update package versions
- Updated dependencies
  - @forklaunch/validator@0.6.11
  - @forklaunch/common@0.3.11

## 0.9.11

### Patch Changes

- Export pino types for type safety

## 0.9.10

### Patch Changes

- bump package versions, allow for validator custom types that resolve as any, export http framework options type
- Updated dependencies
  - @forklaunch/validator@0.6.10
  - @forklaunch/common@0.3.10

## 0.9.9

### Patch Changes

- update package dependencies
- Updated dependencies
  - @forklaunch/validator@0.6.9
  - @forklaunch/common@0.3.9

## 0.9.8

### Patch Changes

- patch return types for create and update static methods

## 0.9.7

### Patch Changes

- Add back sensible entity utilities

## 0.9.6

### Patch Changes

- fix internal type

## 0.9.5

### Patch Changes

- Update internaldtomapper type

## 0.9.4

### Patch Changes

- Update request mapper signature types

## 0.9.3

### Patch Changes

- package conflict resolution
- fix minor buffer bugs and update subdependencies
- Updated dependencies
- Updated dependencies
  - @forklaunch/common@0.3.8
  - @forklaunch/validator@0.6.8

## 0.9.2

### Patch Changes

- Internal type adjustment

## 0.9.1

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.6.7
  - @forklaunch/common@0.3.7

## 0.9.0

### Minor Changes

- Mapper now only accepts async mapping methods, due to nature of entity retrieval

## 0.8.8

### Patch Changes

- Move getEnvVar into common package and allow for cors options during application instantiation
- Updated dependencies
  - @forklaunch/common@0.3.6
  - @forklaunch/validator@0.6.6

## 0.8.7

### Patch Changes

- increase package dependency versions
- Updated dependencies
  - @forklaunch/validator@0.6.5
  - @forklaunch/common@0.3.5

## 0.8.6

### Patch Changes

- simplify controller types

## 0.8.5

### Patch Changes

- Better file based ergonomics in validator, simplification of types and all but validator is checked by tsgo
- Updated dependencies
  - @forklaunch/validator@0.6.4
  - @forklaunch/common@0.3.4

## 0.8.4

### Patch Changes

- increase package versions
- Updated dependencies
  - @forklaunch/validator@0.6.3
  - @forklaunch/common@0.3.3

## 0.8.3

### Patch Changes

- split out infrastructure into separate packages

## 0.8.2

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.3.2
  - @forklaunch/validator@0.6.2

## 0.8.1

### Patch Changes

- Add additional options to framework to instantiate applications and routers
- Updated dependencies
  - @forklaunch/validator@0.6.1
  - @forklaunch/common@0.3.1

## 0.8.0

### Minor Changes

- Added support for content types in request/response and fixed edge cases in validator

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.3.0
  - @forklaunch/validator@0.6.0

## 0.7.4

### Patch Changes

- Increase package versions
- Updated dependencies
  - @forklaunch/validator@0.5.4
  - @forklaunch/common@0.2.11

## 0.7.3

### Patch Changes

- stringify logger arguments
- Updated dependencies
  - @forklaunch/validator@0.5.3
  - @forklaunch/common@0.2.10

## 0.7.2

### Patch Changes

- Various bugfixes, including deduplicated http metrics, multiple constructed singleton loading and leaking empty enqueued redis records"
- Updated dependencies
  - @forklaunch/validator@0.5.2
  - @forklaunch/common@0.2.9

## 0.7.1

### Patch Changes

- Upgrade package versions
- Updated dependencies
  - @forklaunch/validator@0.5.1
  - @forklaunch/common@0.2.8

## 0.7.0

### Minor Changes

- Added persistence into core package, better documentation and more validator utilities

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.5.0
  - @forklaunch/common@0.2.7

## 0.6.6

### Patch Changes

- Change name from dto mapper => mapper

## 0.6.5

### Patch Changes

- Service schema validators now accept keyword argument options for passing down to schemas

## 0.6.4

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.4.12

## 0.6.3

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.4.11

## 0.6.2

### Patch Changes

- Update package versions
- Updated dependencies
  - @forklaunch/validator@0.4.10
  - @forklaunch/common@0.2.6

## 0.6.1

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.4.9

## 0.6.0

### Minor Changes

- Syntactic QOL improvements (validator zod args, config injector, core utilities, test utilities, etc.)

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.4.8
  - @forklaunch/common@0.2.5

## 0.5.6

### Patch Changes

- Increase package dependency versions
- Updated dependencies
  - @forklaunch/validator@0.4.7
  - @forklaunch/common@0.2.4

## 0.5.5

### Patch Changes

- Enables docs configuration to be set by caller and sends parsing error information to client if api parsing fails

## 0.5.4

### Patch Changes

- Allow for constructed singletons in config validation and add latency metric for OpenTelemetryCollector (+small tweaks)

## 0.5.3

### Patch Changes

- Reintroduce request to auth and fix typing issues

## 0.5.2

### Patch Changes

- Constrain the auth request to only include discovered parameters for simplicity. Bump package versions.
- Updated dependencies
  - @forklaunch/validator@0.4.6
  - @forklaunch/common@0.2.3

## 0.5.1

### Patch Changes

- bump package versions
- Updated dependencies
  - @forklaunch/validator@0.4.5
  - @forklaunch/common@0.2.2

## 0.5.0

### Minor Changes

- Add support for built in monitoring

## 0.4.0

### Minor Changes

- Adds nascent support for OpenTelemetry (logs, metrics, traces)

## 0.3.6

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.4.4

## 0.3.5

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.2.1
  - @forklaunch/validator@0.4.3

## 0.3.4

### Patch Changes

- fix config injector ergonomics to be much nicer
- Updated dependencies
  - @forklaunch/validator@0.4.2

## 0.3.3

### Patch Changes

- Create an actual type from valid config injector since splay dropped methods

## 0.3.2

### Patch Changes

- Change return type of validateConfigSingletons to ValidConfigInjector to ensure validity

## 0.3.1

### Patch Changes

- Validator parse methods now return errors, and config injector now validates class based or schematic singletons, returning a ValidConfigInjector object
- Updated dependencies
  - @forklaunch/validator@0.4.1

## 0.3.0

### Minor Changes

- Changed build from tsc to tsup to accommodate cjs and esm consumers

### Patch Changes

- Updated dependencies
  - @forklaunch/validator@0.4.0
  - @forklaunch/common@0.2.0

## 0.2.37

### Patch Changes

- Remove uuid constraint for primarykey

## 0.2.36

### Patch Changes

- Include a mongo base entity for use with mongodb backends

## 0.2.35

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.1.14
  - @forklaunch/validator@0.3.13

## 0.2.34

### Patch Changes

- multiple improvements for config injector class

## 0.2.33

### Patch Changes

- Add schema check to validator interface, and for validating configurations, check if value is a schema and return any errors with pathing
- Updated dependencies
  - @forklaunch/validator@0.3.12

## 0.2.32

### Patch Changes

- append subrouters to router to enable openapi spec

## 0.2.31

### Patch Changes

- jose export for bun compatibility

## 0.2.30

### Patch Changes

- 59d4bfd: Upgrade core package to be more compatible with bun

## 0.2.29

### Patch Changes

- Move enum into validator, and bump package versions
- Updated dependencies
  - @forklaunch/common@0.1.13
  - @forklaunch/validator@0.3.11

## 0.2.28

### Patch Changes

- bump package versions to latest
- Updated dependencies
  - @forklaunch/validator@0.3.10
  - @forklaunch/common@0.1.12

## 0.2.27

### Patch Changes

- Adds utilities for removing trailing slashes and checking if a top level property should be optional if all children are optional. Additionally allows Application classes to use all Router methods as an extension.
- Updated dependencies
  - @forklaunch/common@0.1.11
  - @forklaunch/validator@0.3.9

## 0.2.26

### Patch Changes

- Adds utility type for controllers, utilities for constructing cache keys, and ensures that router registrations match the path of the typed handler

## 0.2.25

### Patch Changes

- includes wrapper functions for better ergonomics for typedHandler functions (get, delete\_, options, head, trace, post, patch, put, middleware)

## 0.2.24

### Patch Changes

- Improve typing on dto mapper methods

## 0.2.23

### Patch Changes

- Made TtlCacheRecord generic and improved ergonomics for mappers (inline and more appropriate names)

## 0.2.22

### Patch Changes

- scopedResolver in ConfigInjector should create a new scope if not supplied with one

## 0.2.21

### Patch Changes

- updates config injector with scopedResolver for nicer handling when used with routers

## 0.2.20

### Patch Changes

- last version not built

## 0.2.19

### Patch Changes

- loosen constraint on isExpressLikeSchemaHandler

## 0.2.18

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.1.10
  - @forklaunch/validator@0.3.8

## 0.2.17

### Patch Changes

- Remove unnecessary flatten

## 0.2.16

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.1.9
  - @forklaunch/validator@0.3.7

## 0.2.15

### Patch Changes

- Add ApiClient top level type for use with exporting live type routers

## 0.2.14

### Patch Changes

- Improve error messages

## 0.2.13

### Patch Changes

- Improve controller handler error message

## 0.2.12

### Patch Changes

- Removing es-module type, due to incompatibility with downstream dependencies.
- Updated dependencies
  - @forklaunch/validator@0.3.6
  - @forklaunch/common@0.1.8
