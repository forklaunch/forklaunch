# @forklaunch/implementation-cac-base

## 0.3.1

### Patch Changes

- Add the claim-scrubbing engine and the real-CPT extension point, and fix the package's `exports` map.

  - `ScrubbingService` checks a claim's charge lines and diagnosis codes in four layers: required fields (CO-16), NCCI procedure-to-procedure conflicts (CO-97), NCCI unit caps (CO-151) and LCD/NCD medical necessity (CO-50). Each finding carries a `message` and, when it is about one line or code, the 1-based `chargeLine` and the `code`. The rule tables are mock data; see the README section "Not production scrubbing yet".
  - `CptCodeProvider` and the `CptCodeSource` interface let an organization that holds its own AMA CPT license supply its own codes. The package contains no CPT content.
  - `normalizeCode` puts codes in one form (no dot, no spaces, upper case), so `E11.9`, `E119` and `e11.9` are the same code in every lookup. `MockProcedureCodeProvider` looks codes up through it.
  - `exports["./services"]` pointed at `lib/services/index.{js,mjs}`, which the build never emitted; it now points at `lib/index.{js,mjs}`. The published 0.2.9 and 0.3.0 ship only `lib/index.{js,mjs}`, so importing `@forklaunch/implementation-cac-base/services` at runtime could not resolve.

## 0.3.0

### Minor Changes

- 4ec1f4b: Built on `@forklaunch/core` 4 and `@mikro-orm/core` 7.2.10. The previous versions depend on core 3 and pin MikroORM 7.2.5, so an application on core 4 that used them installed two copies of each and its entity types stopped matching.

  Breaking, because core 4 changes defaults these packages run with: cross-origin browser access is denied unless the application names its origins (`CORS_ORIGINS` or a `cors` option), an HMAC-signed request is accepted once and only within 5 minutes of the server clock (multi-instance services need a shared nonce store), and JWT verification pins the signature algorithm. See the `@forklaunch/core` 4.0.0 changelog for what to change. 0.x packages take a minor bump, the 0.x equivalent of a major.

### Patch Changes

- Updated dependencies [4ec1f4b]
  - @forklaunch/interfaces-cac@0.3.0

## 0.2.9

### Patch Changes

- Built on @forklaunch/core 3.2.0 (default audit trail, listUnsubscribe, compliant fields decrypt under their loading tenant) and a dependency refresh; Stripe stays on 22.x.
- Updated dependencies
  - @forklaunch/interfaces-cac@0.2.8

## 0.2.8

### Patch Changes

- Dependency refresh: @forklaunch/core 3.1.0 (explicit Stripe apiKey wins over a detected managed contract; managed email for regular apps), MikroORM 7.2.5, and other minor updates. Stripe stays on 22.x.
- Updated dependencies
  - @forklaunch/interfaces-cac@0.2.7

## 0.2.7

### Patch Changes

- Depend on @forklaunch/core 3 (pii/phi/pci properties load as CompliantField; read them with .deanon or .anon) and refresh dependencies to their latest versions, including @mikro-orm 7.2.3.
- Updated dependencies
  - @forklaunch/interfaces-cac@0.2.6

## 0.2.6

### Patch Changes

- Refresh every dependency and move the blueprint onto the framework wave released alongside it.

  The whole set is versioned together on purpose. These packages reference each other as `workspace:^`, which is rewritten to the sibling's _current_ version at publish time — so releasing one ahead of the others pins their **previous** versions and a scaffolded app installs two copies of the same package. That is what produced the duplicate-`@forklaunch/core` type errors behind #311 and #331, and the fix is procedural rather than technical: release the whole set in one wave.

  Framework references now point at `@forklaunch/core@^2.1.1` and its siblings (`common@^1.2.28`, `validator@^1.2.29`, `express`/`hyper-express@^1.2.46`, `internal@^1.2.31`, `testing@^1.2.33`, `universal-sdk@^1.2.29`, `infrastructure-redis`/`infrastructure-s3@^1.4.17`).

  `@mikro-orm/*` is **deliberately held at 7.2.1** rather than moved to the 7.2.2 that shipped during this wave. The framework packages were published pinned to 7.2.1, and MikroORM's decorators and `EntityManager` identity do not survive two copies in one dependency graph — a blueprint on 7.2.2 against a framework on 7.2.1 is the same duplicate-package failure in a different package. `pnpm update --latest` moved six of the eleven `@mikro-orm/*` entries and left five, which is the split this normalizes; the pair moves together in the next wave.

  Also adds the three `__test__/test-utils.ts` files the shared `vitest.config.ts` names in `setupFiles` but that were never created — `implementations/iam/base`, `implementations/ecommerce/stripe`, and `implementations/worker/redis`. Without the file vitest cannot load the suite and reports "no tests" rather than an error, so those 16 tests had never run in CI. They pass.

- Updated dependencies
  - @forklaunch/interfaces-cac@0.2.5

## 0.2.4

### Patch Changes

- `@forklaunch/core` 1.6.5, and shape entities that carry no `any`.

  The implementation packages describe the entities they work with through minimal "shape"
  entities. Their enum columns were declared `fp.enum()` with no members, which infers
  `any`; `ResolvedEntity` in core 1.6.5 no longer papers over that, so every such column is
  now `fp.enum<string[]>()`: a string column whose members the application supplies.

  With the shape typed honestly, the `& { status: StatusEnum[keyof StatusEnum] }`
  intersections on the entity constraint types (`OrganizationEntities`, `BaseSmsEntities`,
  `BaseCheckoutSessionEntities`, `BasePaymentLinkEntities`, `BasePlanEntities`,
  `BaseSubscriptionEntities`, and the Stripe equivalents) had to go: a service queries with
  the shape schema (`entity as typeof Plan`), and that cast is only valid when the shape is
  comparable to the constraint, which a generic enum member never is. The services never
  read those columns; the DTO types still carry the enums, and the application's mapper is
  where entity and DTO meet. The constraint types therefore lose their enum type parameters
  (`BasePlanEntities<Cadence, Currency, Provider>` is now `BasePlanEntities`); services and
  mapper types keep theirs for the DTO side.

- Updated dependencies
  - @forklaunch/interfaces-cac@0.2.4

## 0.2.3

### Patch Changes

- Refresh dependencies to their latest published versions and move to `@forklaunch/core`
  1.6.4 on mikro-orm 7.2.1.

  The framework, the blueprint and the CLI's scaffold constants now agree on a single
  mikro-orm version (7.2.1, pinned exactly), so a generated app resolves one copy. The
  worker implementations (bullmq, database, kafka, redis) now declare `@forklaunch/validator`
  directly: their schema resolvers name `AnySchemaValidator` in emitted declarations, and
  TypeScript refuses to reference a package that is not a dependency of the emitting one.

- Updated dependencies
  - @forklaunch/interfaces-cac@0.2.3

## 0.2.2

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

- Updated dependencies
  - @forklaunch/interfaces-cac@0.2.2

## 0.2.1

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
  - @forklaunch/interfaces-cac@0.2.1

## 0.2.0

### Minor Changes

- 89e1036: Add cac (computer-assisted coding) module skeleton: CodeSetProvider interface with a mock procedure-code implementation (free, unlicensed placeholder codes) — the swappable abstraction that licensed CPT data will plug into once an organization's AMA license is active.

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

- Updated dependencies [89e1036]
- Updated dependencies [927378c]
  - @forklaunch/interfaces-cac@0.2.0
