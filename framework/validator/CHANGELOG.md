# @forklaunch/validator

## 0.10.33

### Patch Changes

- proxy based injection for ci, and openapi path resiliency
- Updated dependencies
  - @forklaunch/common@0.6.33

## 0.10.32

### Patch Changes

- Small bugs
- Updated dependencies
  - @forklaunch/common@0.6.32

## 0.10.31

### Patch Changes

- Prevent 404 message hijacking and update packages
- Updated dependencies
  - @forklaunch/common@0.6.31

## 0.10.30

### Patch Changes

- Fix multiline config injection and update packages
- Updated dependencies
  - @forklaunch/common@0.6.30

## 0.10.29

### Patch Changes

- WS actually working probably, and package bumps
- Updated dependencies
  - @forklaunch/common@0.6.29

## 0.10.28

### Patch Changes

- 4e10567: Update dependency versions
- Updated dependencies [4e10567]
  - @forklaunch/common@0.6.28

## 0.10.27

### Patch Changes

- Fix config propogation from app to route
- Updated dependencies
  - @forklaunch/common@0.6.27

## 0.10.26

### Patch Changes

- Package deps version bump
- Updated dependencies
  - @forklaunch/common@0.6.26

## 0.10.25

### Patch Changes

- package version bump
- Updated dependencies
  - @forklaunch/common@0.6.25

## 0.10.24

### Patch Changes

- Mapper instantiation syntax more readable and express port added. Also removed error schema thrash in live sdk
- Updated dependencies
  - @forklaunch/common@0.6.24

## 0.10.23

### Patch Changes

- update framework pages
- Updated dependencies
  - @forklaunch/common@0.6.23

## 0.10.22

### Patch Changes

- fix hyper express header, fix tests
- Updated dependencies
  - @forklaunch/common@0.6.22

## 0.10.21

### Patch Changes

- Update package versions, and add x-powered-by forklaunch
- Updated dependencies
  - @forklaunch/common@0.6.21

## 0.10.20

### Patch Changes

- update internal package versions
- Updated dependencies
  - @forklaunch/common@0.6.20

## 0.10.19

### Patch Changes

- update package versions
- Updated dependencies
  - @forklaunch/common@0.6.19

## 0.10.18

### Patch Changes

- update packages, make OpenTelemetryCollector type more transparent, attempt to fix error loggings
- Updated dependencies
  - @forklaunch/common@0.6.18

## 0.10.17

### Patch Changes

- Update internal packages
- Updated dependencies
  - @forklaunch/common@0.6.17

## 0.10.16

### Patch Changes

- Introduce testing package and deepclone openapi objects
- Updated dependencies
  - @forklaunch/common@0.6.16

## 0.10.15

### Patch Changes

- Minor bugfixes and package version bumps
- Updated dependencies
  - @forklaunch/common@0.6.15

## 0.10.14

### Patch Changes

- package upgrade
- Updated dependencies
  - @forklaunch/common@0.6.14

## 0.10.13

### Patch Changes

- upgrade package dependencies and add global options to nested routers
- Updated dependencies
  - @forklaunch/common@0.6.13

## 0.10.12

### Patch Changes

- Update internal packages and expose RegistryOptions from universal sdk
- Updated dependencies
  - @forklaunch/common@0.6.12

## 0.10.11

### Patch Changes

- Set the stage for improved universal sdk performance, and update internal packages
- Updated dependencies
  - @forklaunch/common@0.6.11

## 0.10.10

### Patch Changes

- Update internal package versions
- Updated dependencies
  - @forklaunch/common@0.6.10

## 0.10.9

### Patch Changes

- update internal packages and loosen global auth constraint
- Updated dependencies
  - @forklaunch/common@0.6.9

## 0.10.8

### Patch Changes

- update internal packages
- Updated dependencies
  - @forklaunch/common@0.6.8

## 0.10.7

### Patch Changes

- slight hmac token creation signature change
- Updated dependencies
  - @forklaunch/common@0.6.7

## 0.10.6

### Patch Changes

- Update packages and expose hmac key creation function
- Updated dependencies
  - @forklaunch/common@0.6.6

## 0.10.5

### Patch Changes

- update internal packages
- Updated dependencies
  - @forklaunch/common@0.6.5

## 0.10.4

### Patch Changes

- Update internal package versions and add mapServiceSchemas method for clean DX in implemented modules
- Updated dependencies
  - @forklaunch/common@0.6.4

## 0.10.3

### Patch Changes

- toDomain -> toDto for more accurate naming conventions
- Updated dependencies
  - @forklaunch/common@0.6.3

## 0.10.2

### Patch Changes

- toDto -> toDomain
- Updated dependencies
  - @forklaunch/common@0.6.2

## 0.10.1

### Patch Changes

- request and response mapper discrimination and clean up of internal types
- Updated dependencies
  - @forklaunch/common@0.6.1

## 0.10.0

### Minor Changes

- remove class based mappers

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.6.0

## 0.9.9

### Patch Changes

- add mappers as functions
- Updated dependencies
  - @forklaunch/common@0.5.8

## 0.9.8

### Patch Changes

- One more attempt at performance bump
- Updated dependencies
  - @forklaunch/common@0.5.7

## 0.9.7

### Patch Changes

- prettify req init for slightly faster sdk access
- Updated dependencies
  - @forklaunch/common@0.5.6

## 0.9.6

### Patch Changes

- attempt to make sdk pathing more efficient
- Updated dependencies
  - @forklaunch/common@0.5.5

## 0.9.5

### Patch Changes

- zod validator regex relaxation for email
- Updated dependencies
  - @forklaunch/common@0.5.4

## 0.9.4

### Patch Changes

- Update validator types for files to use raw streams, lazy load openapi for universal sdk, and remove private members from otel
- Updated dependencies
  - @forklaunch/common@0.5.3

## 0.9.3

### Patch Changes

- update package versions
- Updated dependencies
  - @forklaunch/common@0.5.2

## 0.9.2

### Patch Changes

- Auth fixes and add HMAC auth

## 0.9.1

### Patch Changes

- bump internal packages
- Updated dependencies
  - @forklaunch/common@0.5.1

## 0.9.0

### Minor Changes

- Adds more configuration options for application and routers. Additionally adds optional cluster support built-in (experimental)

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.5.0

## 0.8.0

### Minor Changes

- Add more permissive body types and update schema validator types

## 0.7.8

### Patch Changes

- Add versions to contract details, migrate sdk and fetch to functions for much better ergonomics
- Updated dependencies
  - @forklaunch/common@0.4.6

## 0.7.7

### Patch Changes

- Update to zod v4, keeping zod v3 as active zod version
- Updated dependencies
  - @forklaunch/common@0.4.5

## 0.7.6

### Patch Changes

- Upgrade internal dependencies
- Updated dependencies
  - @forklaunch/common@0.4.4

## 0.7.5

### Patch Changes

- Auth types are now propogated to live sdk types
- Updated dependencies
  - @forklaunch/common@0.4.3

## 0.7.4

### Patch Changes

- remove enum from all packages for erasable syntax
- Updated dependencies
  - @forklaunch/common@0.4.2

## 0.7.3

### Patch Changes

- Reduce zod depth inference a little bit further

## 0.7.2

### Patch Changes

- Lower zod depth checking threshold to help with type inference

## 0.7.1

### Patch Changes

- node types version upgrade
- Updated dependencies
  - @forklaunch/common@0.4.1

## 0.7.0

### Minor Changes

- package version upgrade, mcp generation and nicer universal sdk syntax

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.4.0

## 0.6.16

### Patch Changes

- change dtoMapper to Mapper
- Updated dependencies
  - @forklaunch/common@0.3.14

## 0.6.15

### Patch Changes

- create internal package for internal utilities
- Updated dependencies
  - @forklaunch/common@0.3.13

## 0.6.14

### Patch Changes

- bump package subdependencies
- Updated dependencies
  - @forklaunch/common@0.3.12

## 0.6.13

### Patch Changes

- update package deps

## 0.6.12

### Patch Changes

- use tsgo and export zod types

## 0.6.11

### Patch Changes

- update package versions
- Updated dependencies
  - @forklaunch/common@0.3.11

## 0.6.10

### Patch Changes

- bump package versions, allow for validator custom types that resolve as any, export http framework options type
- Updated dependencies
  - @forklaunch/common@0.3.10

## 0.6.9

### Patch Changes

- update package dependencies
- Updated dependencies
  - @forklaunch/common@0.3.9

## 0.6.8

### Patch Changes

- package conflict resolution
- fix minor buffer bugs and update subdependencies
- Updated dependencies
- Updated dependencies
  - @forklaunch/common@0.3.8

## 0.6.7

### Patch Changes

- update package versions
- Updated dependencies
  - @forklaunch/common@0.3.7

## 0.6.6

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.3.6

## 0.6.5

### Patch Changes

- increase package dependency versions
- Updated dependencies
  - @forklaunch/common@0.3.5

## 0.6.4

### Patch Changes

- Better file based ergonomics in validator, simplification of types and all but validator is checked by tsgo
- Updated dependencies
  - @forklaunch/common@0.3.4

## 0.6.3

### Patch Changes

- increase package versions
- Updated dependencies
  - @forklaunch/common@0.3.3

## 0.6.2

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.3.2

## 0.6.1

### Patch Changes

- Add additional options to framework to instantiate applications and routers
- Updated dependencies
  - @forklaunch/common@0.3.1

## 0.6.0

### Minor Changes

- Added support for content types in request/response and fixed edge cases in validator

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.3.0

## 0.5.4

### Patch Changes

- Increase package versions
- Updated dependencies
  - @forklaunch/common@0.2.11

## 0.5.3

### Patch Changes

- stringify logger arguments
- Updated dependencies
  - @forklaunch/common@0.2.10

## 0.5.2

### Patch Changes

- Various bugfixes, including deduplicated http metrics, multiple constructed singleton loading and leaking empty enqueued redis records"
- Updated dependencies
  - @forklaunch/common@0.2.9

## 0.5.1

### Patch Changes

- Upgrade package versions
- Updated dependencies
  - @forklaunch/common@0.2.8

## 0.5.0

### Minor Changes

- Added persistence into core package, better documentation and more validator utilities

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.2.7

## 0.4.12

### Patch Changes

- Change Typebox catchall from TSchema => TKind for more permissive type matching

## 0.4.11

### Patch Changes

- Fix minor issue where typebox resolution was breaking

## 0.4.10

### Patch Changes

- Update package versions
- Updated dependencies
  - @forklaunch/common@0.2.6

## 0.4.9

### Patch Changes

- Schema Validator types now have runtime typechecking as well

## 0.4.8

### Patch Changes

- Syntactic QOL improvements (validator zod args, config injector, core utilities, test utilities, etc.)
- Updated dependencies
  - @forklaunch/common@0.2.5

## 0.4.7

### Patch Changes

- Increase package dependency versions
- Updated dependencies
  - @forklaunch/common@0.2.4

## 0.4.6

### Patch Changes

- Constrain the auth request to only include discovered parameters for simplicity. Bump package versions.
- Updated dependencies
  - @forklaunch/common@0.2.3

## 0.4.5

### Patch Changes

- bump package versions
- Updated dependencies
  - @forklaunch/common@0.2.2

## 0.4.4

### Patch Changes

- Update dependencies under the hood

## 0.4.3

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.2.1

## 0.4.2

### Patch Changes

- fix config injector ergonomics to be much nicer

## 0.4.1

### Patch Changes

- Validator parse methods now return errors, and config injector now validates class based or schematic singletons, returning a ValidConfigInjector object

## 0.4.0

### Minor Changes

- Changed build from tsc to tsup to accommodate cjs and esm consumers

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.2.0

## 0.3.13

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.1.14

## 0.3.12

### Patch Changes

- Add schema check to validator interface, and for validating configurations, check if value is a schema and return any errors with pathing

## 0.3.11

### Patch Changes

- Move enum into validator, and bump package versions
- Updated dependencies
  - @forklaunch/common@0.1.13

## 0.3.10

### Patch Changes

- bump package versions to latest
- Updated dependencies
  - @forklaunch/common@0.1.12

## 0.3.9

### Patch Changes

- Adds utilities for removing trailing slashes and checking if a top level property should be optional if all children are optional. Additionally allows Application classes to use all Router methods as an extension.
- Updated dependencies
  - @forklaunch/common@0.1.11

## 0.3.8

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.1.10

## 0.3.7

### Patch Changes

- Updated dependencies
  - @forklaunch/common@0.1.9

## 0.3.6

### Patch Changes

- Removing es-module type, due to incompatibility with downstream dependencies.
- Updated dependencies
  - @forklaunch/common@0.1.8
