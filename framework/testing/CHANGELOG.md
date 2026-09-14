# @forklaunch/testing

## 1.2.31

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

## 1.2.30

### Patch Changes

- Release the rest of the workspace alongside the dependency refresh, so every
  published package moves together on this pass.

  `up:packages` reached these differently than the five that changed runtime
  dependencies: `universal-sdk`, `ws` and `infrastructure-redis` picked up
  devDependency movement only (`jest` 30.4.2 → 30.5.0), and `bunrun`, `common`,
  `internal` and `testing` saw no manifest change at all. Their emitted output is
  therefore unchanged.

  They are released regardless to keep the whole set on one refresh, rather than
  leaving consumers to work out which packages a given update did and did not
  touch.

## 1.2.29

### Patch Changes

- Update internal package versions

## 1.2.28

### Patch Changes

- 9334446: Fix BlueprintTestHarness so generated app E2E tests can actually run.

  `clearTestDatabase` now reads the entity list from `orm.config.get('entities')`
  (MikroORM v7's `getMetadata().getAll()` returns an empty object, so the old
  code cleared nothing and re-seeds hit duplicate-key errors), and clears in a
  foreign-key-safe retry-until-stable order instead of a single reverse pass.

  Pairs with the blueprint test-utils change that hands the harness its own
  `discovery` object, so `MikroORM.init()` can't mutate the app's shared config
  and leave the app container's `new MikroORM(config)` with an undefined `.em`.

## 1.2.23

### Patch Changes

- restrict wildcard subpath exports to the types condition
- setupTestORM returns AnyMikroORM to match the relaxed harness contract

## 1.2.22

### Patch Changes

- add wildcard subpath exports for per-file declaration output

## 1.2.21

### Patch Changes

- relax harness ORM types to AnyMikroORM (MikroORM v7 readonly entities arrays)

## 1.2.19

### Patch Changes

- TypeScript 7 build pipeline: tsgo declaration emit replaces tsup --dts

## 1.2.20

### Patch Changes

- update packages

## 1.2.19

### Patch Changes

- 92c06f9: dep upgrades

## 1.2.18

### Patch Changes

- update dependency versions

## 1.2.17

### Patch Changes

- Update internal versions and allow ZodType early release

## 1.2.16

### Patch Changes

- Export wrapEmWithTenantContext for tenant based filtering

## 1.2.15

### Patch Changes

- chore: update internal package versions

## 1.2.14

### Patch Changes

- update enum logic

## 1.2.13

### Patch Changes

- Update packages and enum constraint fix

## 1.2.12

### Patch Changes

- sync changes across packages

## 1.2.11

### Patch Changes

- Align package vers

## 1.2.10

### Patch Changes

- fix nested app and router

## 1.2.9

### Patch Changes

- Perf improvement

## 1.2.8

### Patch Changes

- bump package versions

## 1.2.7

### Patch Changes

- export consolidated retention logic

## 1.2.6

### Patch Changes

- Encryptor required on redis and s3

## 1.2.5

### Patch Changes

- Make private fields respect interfaces

## 1.2.4

### Patch Changes

- up packages

## 1.2.3

### Patch Changes

- update packages

## 1.2.2

### Patch Changes

- tenant and rls configuration

## 1.2.1

### Patch Changes

- fix compliance entity

## 1.2.0

### Minor Changes

- Validator 25% performance uptick and cleaner Config Injector syntax

## 1.1.8

### Patch Changes

- Simplify property chain for easier consumption

## 1.1.7

### Patch Changes

- More relations covered for compliance entity

## 1.1.6

### Patch Changes

- Restore MaybeOpt

## 1.1.5

### Patch Changes

- cross boundary inference fix compliance entities

## 1.1.4

### Patch Changes

- improve performance of entity branding

## 1.1.3

### Patch Changes

- Package versions and simplified compliance entity typing

## 1.1.2

### Patch Changes

- move FieldEncryptor into persistence (previously not exported)

## 1.1.1

### Patch Changes

- add compliance utilities

## 1.1.0

### Minor Changes

- retention policy update

## 1.0.13

### Patch Changes

- patch working"

## 1.0.12

### Patch Changes

- try removing return type to let inference take over

## 1.0.11

### Patch Changes

- refinement

## 1.0.10

### Patch Changes

- store property as internal property instead of branding

## 1.0.9

### Patch Changes

- branding fixes

## 1.0.8

### Patch Changes

- remove brand from entity

## 1.0.7

### Patch Changes

- inconsistent state

## 1.0.6

### Patch Changes

- string brand instead of symbol

## 1.0.5

### Patch Changes

- entity rework
- fix compliance brands

## 1.0.4

### Patch Changes

- Handle functional definitions on mikroorm entities

## 1.0.3

### Patch Changes

- Update packages and fix entity type

## 1.0.2

### Patch Changes

- Fix type agreement

## 1.0.1

### Patch Changes

- Version thrash

## 1.0.0

### Major Changes

- Compliance features first party

## 0.1.5

### Patch Changes

- Another fix

## 0.1.4

### Patch Changes

- correct extension for mappers

## 0.1.3

### Patch Changes

- mapper fix

## 0.1.2

### Patch Changes

- Update packages and remove EntityMapper wrapping

## 0.1.1

### Patch Changes

- package upgrades

## 0.1.0

### Minor Changes

- update packages and update to mikro orm v7

## 0.0.29

### Patch Changes

- clean build

## 0.0.28

### Patch Changes

- fix mikroorm

## 0.0.27

### Patch Changes

- actually fix mikroorm

## 0.0.26

### Patch Changes

- fix mikroorm package versions

## 0.0.25

### Patch Changes

- internal package bump

## 0.0.24

### Patch Changes

- Downgrade mikro-orm back to normal

## 0.0.23

### Patch Changes

- bump packages and internal proxy await resilience

## 0.0.22

### Patch Changes

- proxy based injection for ci, and openapi path resiliency

## 0.0.21

### Patch Changes

- Small bugs

## 0.0.20

### Patch Changes

- Prevent 404 message hijacking and update packages

## 0.0.19

### Patch Changes

- Fix multiline config injection and update packages

## 0.0.18

### Patch Changes

- WS actually working probably, and package bumps

## 0.0.17

### Patch Changes

- 4e10567: Update dependency versions

## 0.0.16

### Patch Changes

- Fix config propogation from app to route

## 0.0.15

### Patch Changes

- Package deps version bump

## 0.0.14

### Patch Changes

- package version bump

## 0.0.13

### Patch Changes

- Mapper instantiation syntax more readable and express port added. Also removed error schema thrash in live sdk

## 0.0.12

### Patch Changes

- update framework pages

## 0.0.11

### Patch Changes

- fix hyper express header, fix tests

## 0.0.10

### Patch Changes

- Update package versions, and add x-powered-by forklaunch

## 0.0.9

### Patch Changes

- update internal package versions

## 0.0.8

### Patch Changes

- update package versions

## 0.0.7

### Patch Changes

- update packages, make OpenTelemetryCollector type more transparent, attempt to fix error loggings

## 0.0.6

### Patch Changes

- Change database cleanup function to accept object style parameter

## 0.0.5

### Minor Changes

- Add Kafka testcontainer support
- Add S3/MinIO testcontainer support
- Add environment variable setup for Kafka (KAFKA_BROKERS, KAFKA_CLIENT_ID, KAFKA_GROUP_ID)
- Add environment variable setup for S3 (S3_ENDPOINT, S3_ACCESS_KEY_ID, S3_SECRET_ACCESS_KEY, S3_REGION, S3_BUCKET)
- Update BlueprintTestHarness to support `needsKafka` and `needsS3` options
- Export new config types: KafkaConfig and S3Config

## 0.0.4

### Patch Changes

- Make database options optional

## 0.0.3

### Patch Changes

- Internal fixes

## 0.0.2

### Patch Changes

- Introduce testing package and deepclone openapi objects

## 0.0.1

### Patch Changes

- Introduce testing package and deepclone openapi objects
