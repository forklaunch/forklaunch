---
name: cli
description: "CLI: init, change, delete, deploy, environment, release, sync, sdk, openapi."
user-invokable: true
---

# ForkLaunch CLI Skill

## Prerequisites

Before running any `forklaunch` command, verify the CLI is installed:

```bash
# Check if installed
forklaunch version

# If "command not found", install globally
npm install -g @forklaunch/cli
```

**When Claude invokes CLI commands, always check first:**

```bash
# Pre-flight check — install if missing
command -v forklaunch >/dev/null 2>&1 || npm install -g @forklaunch/cli
```

**Other prerequisites:**
- **Node.js** >= 20 (or Bun >= 1.1 if using `--runtime bun`)
- **Docker** — required for `docker compose up -d` (postgres, redis, etc.)
- **pnpm** (for Node runtime) or **bun** (for Bun runtime) as package manager

## When to Use This Skill

Use this skill when the user asks to:

- Create new services, workers, libraries, or routers in the Forklaunch platform
- Modify existing project components (changing databases, infrastructure, frameworks)
- Initialize a new Forklaunch application
- Work with the Forklaunch CLI commands
- Understand Forklaunch project structure and conventions
- Add or modify Forklaunch manifest configuration

## Overview

ForkLaunch is a TypeScript-first backend framework with incremental adoption, optional static typing, and code-driven architecture. The CLI provides powerful commands for managing modular monolith applications with services, workers, and libraries.

## CRITICAL: Supply ALL Arguments or the CLI Hangs

**The ForkLaunch CLI will drop into interactive mode if ANY required argument is missing.** Interactive mode blocks indefinitely when run from scripts, CI, or AI assistants — the process will hang waiting for stdin input that never comes.

**YOU MUST supply every required flag on the command line. There is no way to skip interactive mode other than providing all arguments.**

**The `--path` flag** specifies where to scaffold. It defaults to the current working directory but should always be explicit when running from a script or AI assistant.

**Package manager follows the runtime:** use `pnpm` for `--runtime node`, use `bun` for `--runtime bun`. For Studio build/codegen plans, package-manager commands are restricted to dependency installation only; runtime, verification, and migration commands are handled by the platform.

**Bun runtime constraints:**

- `better-sqlite` database is NOT supported with Bun
- `hyper-express` is NOT compatible with Bun — the CLI forces `express` when `--runtime bun`

```bash
# Node runtime — use pnpm (ALL flags required to avoid interactive mode)
forklaunch init application my-app \
  --path . \
  --modules-path src/modules \
  --database postgresql \
  --runtime node \
  --validator zod \
  --http-framework express \
  --formatter biome \
  --linter oxlint \
  --test-framework vitest \
  --modules iam-better-auth billing-stripe \
  --license MIT \
  --author "Author Name" \
  --description "App description"
pnpm install

# Bun runtime — use bun
forklaunch init application my-app \
  --path . \
  --modules-path src/modules \
  --database postgresql \
  --runtime bun \
  --validator zod \
  --http-framework express \
  --formatter biome \
  --linter oxlint \
  --test-framework vitest \
  --modules iam-better-auth billing-stripe \
  --license MIT \
  --author "Author Name" \
  --description "App description"
bun install

# CORRECT — init a service with all flags
forklaunch init service billing --path ./src/modules --database postgresql --description "Billing service"

# WRONG — missing required flags, will hang in interactive mode
forklaunch init application my-app  # missing --database, --runtime, --modules, --formatter, --linter, --test-framework, etc.
```

## Studio Build Rules for Init Commands

When generating commands for Studio build/codegen runs, failed `forklaunch init ...` commands are the only tolerated build-pane failure class, and every failure must become a lesson for the next full plan. Subsequent plans must correct the command template instead of repeating the failing command.

Studio CLI command blocks are scaffold-only. They may contain:

- `forklaunch init application ...`
- `forklaunch init service ...`
- `forklaunch init worker ...`
- `forklaunch init library ...`
- `forklaunch init router ...`
- exactly one package install command after all init commands: `pnpm install` for Node runtime or `bun install` for Bun runtime

Studio CLI command blocks must NOT contain:

- `bun run build`, `pnpm run build`, `npm run build`
- `bun run typecheck`, `pnpm run typecheck`, `npm run typecheck`
- `bun run lint`, `pnpm run lint`, `npm run lint`
- `bun test`, `pnpm test`, `npm test`
- `bun dev`, `pnpm dev`, `npm run dev`, `start`, `serve`, or `watch`
- `database:setup` or `migrate:*` commands in the plan CLI block
- chained commands such as `pnpm install && pnpm dev`

The Studio executor starts preview/watch surfaces and defers verification/repair after codegen. Do not put those operations in the plan.

Use these rules for every Studio-safe init command:

- Service, worker, and library init commands run from the application root must use `--path src/modules` or `--path ./src/modules`; never use `--path .` for these module-level commands.
- `init application` may use `--path .`, but it must also set `--modules-path src/modules`.
- All init commands must be non-interactive and include every required flag, including `--database` and `--description` for services.
- Do not generate service, worker, or library names reserved by the application scaffold: `core`, `monitoring`, `client-sdk`, `iam`, or `billing`.
- Avoid numeric module or router names; spell numbers out in lowercase kebab-case.
- If an init command fails, record the command, the failure tail, and the corrected rule, then feed that lesson into the next full plan before emitting new CLI commands.

## Studio Generation Failure Lessons

Use these lessons when repairing or planning generated ForkLaunch Studio apps:

- Do not invent exports from scaffolded core packages. Inspect `src/modules/core` exports before importing names such as billing plan enums, IAM roles, feature flags, or SDK helpers.
- Use only exported RBAC role constants. For generated IAM role checks, prefer scaffolded roles such as `VIEWER`, `EDITOR`, `ADMIN`, or `SYSTEM` when those are the only exported members.
- Do not invent ForkLaunch persistence helper methods. If an entity property needs a numeric field and `fp.number` is not present in the scaffold, follow the existing generated entity patterns or MikroORM property typing instead of guessing helper names.
- In current ForkLaunch persistence builders, integer counters and byte sizes should use `fp.integer()` when nearby scaffold entities do; do not write `fp.number()` unless that helper exists in the generated package.
- For decimal amounts or measured numeric values in generated ForkLaunch persistence entities, prefer `fp.double()` when sibling scaffolded entities use it. `fp.number()` has caused runtime migration failures in Studio generations.
- Route `allowedRoles` belongs inside the route `auth` object for protected ForkLaunch controllers. Do not attach it as a sibling of `auth`; mirror the existing scaffolded protected routes.
- Billing plan enums and IAM role constants must be imported from the actual generated core exports, commonly `@<app>/core`, not assumed subpaths such as `@<app>/core/billing`.
- For ForkLaunch schema validators, do not pass raw string arrays where a record of literal schemas is expected. Mirror nearby generated enum/literal schema patterns exactly.
- ForkLaunch validator `record` requires both key and value schemas. Use `record(string, unknown)` or `record(string, valueSchema)`, not `record(unknown)`.
- When generated starter `{ message }` schemas are replaced with domain schemas, update the starter GET/POST handlers or isolate them behind dedicated starter schemas. Do not return `{ message: "Name" }` from a handler whose `responses[200]` points at a domain entity schema.
- Keep service method signatures and controllers in lockstep. If a method now expects `{ reportId, userId, text }`, controllers should pass `req.body` or an explicit object with those keys, not `req.body.message` or another single string.
- Current Studio-generated services often type interfaces with `Schema<typeof RequestSchema, SchemaValidator>` and return schema DTOs directly. Follow that sibling pattern when present; do not add mappers or entity-return services just because older examples do.
- Live Studio containers run under `tsx watch`; restart logs, shutdown spam, `Process didn't exit in 5s. Force killing`, and MaxListeners warnings are informational when the service starts again. Repair should follow scoped build/typecheck errors, not watch-mode churn.
- In Studio dev runtime, generated workspace package imports should resolve without a production build. If a runtime error says a workspace package cannot find `dist/index.js`, fix the dev source entrypoint/package metadata instead of scattering relative package internals across every service.
- Generated runtime values must be plain runtime values, not schema literal objects. Keep schema declarations separate from service defaults and seeded payloads.
- Frontend wiring must not add imports from packages that are not already installed or declared in the client package manifest. If a new dependency is unavoidable, update the correct package manifest so the platform dependency refresh can install it.
- Generated React/TanStack clients must own a local TypeScript dev dependency. Do not rely on ambient host-global `tsc`; it can be too old for TanStack Router declarations and can reject modern `moduleResolution` values.
- Vite React clients that read `import.meta.env` or import CSS need a `src/vite-env.d.ts` with `/// <reference types="vite/client" />`.
- For TanStack Router, every non-ignored file under `apps/client/src/routes` must export `Route`. Helper-only route-local files need the `-` ignore prefix or should live under `components`. Do not create duplicate route variants for the same URL, such as both `share.$token.tsx` and `share/$token.tsx`.
- If frontend route files use CSS custom properties, either use the exact names defined in global CSS or add aliases. Mismatched tokens such as `--color-text` vs `--color-text-primary` make the UI fall back to browser defaults and look broken.
- Keep frontend API barrels and hooks consistent. If hooks import `apiFetch`, either export that exact helper from the API client barrel or update hooks to the generated helper name such as `httpFetch`.
- Type UI option arrays explicitly when only some entries have optional properties such as `proOnly`; otherwise `as const` unions can make optional property reads fail typecheck.
- Do not emit duplicate object literal keys in inline React style objects; TypeScript rejects duplicate properties even when the browser would ignore one.
- When generating regexes in TypeScript, preserve escape characters. For example, slash-trimming regexes must be valid source such as `/\/$/`, not `//$/`.
- API smoke tests should not cast generic error payloads directly to success response types. Assert status/error branches separately, then narrow successful payloads.
- SDK method names are generated from the actual route contract. Inspect the generated SDK or OpenAPI surface before writing tests that assume names such as `notificationWorkerGet`.

**Complete flag reference for `init application`** (all should be provided):
| Flag | Required | Values |
|------|----------|--------|
| `--path` | Yes | Project directory path |
| `--modules-path` | Yes | `src/modules` or `modules` |
| `--database` | Yes | `postgresql`, `mysql`, `mariadb`, `mssql`, `mongodb`, `libsql`, `sqlite`, `better-sqlite` |
| `--runtime` | Yes | `node`, `bun` |
| `--validator` | Yes | `zod`, `typebox` |
| `--http-framework` | Yes | `express`, `hyper-express` |
| `--formatter` | Yes | `prettier`, `biome` |
| `--linter` | Yes | `eslint`, `oxlint` |
| `--test-framework` | Yes | `vitest`, `jest` |
| `--license` | Yes | `MIT`, `Apache-2.0`, `none`, etc. |
| `--author` | Yes | Author name string |
| `--modules` | Yes (non-interactive) | At least one module required: `iam-better-auth`, `iam-base`, `billing-stripe`, `billing-base` (can repeat `-m`) |
| `--description` | No | App description string |

## What Gets Generated

### `forklaunch init application`

Scaffolds a complete monorepo with:

- **`src/modules/`** — pnpm workspace for isolated backend modules (services, workers, libraries)
  - **`core/`** — shared package (`@{{app-name}}/core`) that re-exports framework primitives, schema validators, session schema, RBAC roles, feature flags
  - **`monitoring/`** — OpenTelemetry metrics definitions
  - **`client-sdk/`** — typed SDK client package for services
- **Docker Compose** — `docker-compose.yaml` with PostgreSQL, Redis, MinIO (S3), and all services as hot-reloading containers
- **`.forklaunch/manifest.toml`** — source of truth for project configuration (never edit manually)
- **Root `package.json`** — workspace root at `src/modules/`

**NOTE:** `init application` does NOT generate a frontend client. If you need a client (React, Next.js, etc.), create it separately (e.g., `npm create vite@latest client -- --template react-ts`). The client lives outside the pnpm workspace and connects to services via HTTP/WebSocket.

### `forklaunch init service`

Adds an isolated module under `src/modules/<name>/` with:

- Full DDD structure: `api/controllers/`, `api/routes/`, `domain/services/`, `domain/schemas/`, `persistence/entities/`
- `registrations.ts`, `bootstrapper.ts`, `server.ts`, `sdk.ts`
- `package.json` with `dev`, `build`, `test`, `migrate:*` scripts
- MikroORM config + migrations directory
- Docker Compose service entry with hot-reloading (`tsx watch`)

### `forklaunch init worker`

Same as service but with BullMQ/Kafka worker infrastructure instead of HTTP routes.

### `forklaunch init library`

Adds a shared library under `src/modules/<name>/` — no server, just exports.

### `forklaunch init module`

Adds a preconfigured module (billing, IAM, or the relay callback listener) to an existing application:

```bash
forklaunch init module <name> --path <app-path> --module <module-type> --database <db>

# Module types:
# billing-base    — Billing app hooks only (no payment provider)
# billing-stripe  — Stripe billing implementation
# iam-base        — IAM authorization only (no auth provider)
# iam-better-auth — Better Auth implementation for IAM
# relay           — Managed-mode relay callback listener. Scaffolds a signed,
#                   universal callback-acceptor endpoint into the app's iam
#                   service: the platform relay forwards a verified provider
#                   callback (Epic OAuth today; any per-instance event) to it,
#                   and it replay-guards, dispatches, and hands back a session.
#                   Injects into iam, so no -d/--database. See the /managed-relay skill.

# Example:
forklaunch init module billing --path ./src/modules --module billing-stripe --database postgresql
forklaunch init module iam --path ./src/modules --module iam-better-auth --database postgresql
forklaunch init module -m relay -p ./src/modules   # adds the relay callback listener to iam
```

**`iam-base` has no login or session flow at all** — it's user/role/permission CRUD, not
authentication. There is no way to obtain a real token from it, so any other service in the app
still can't make a genuinely authenticated call to anything. If the goal is real cross-service
auth (a service actually calling another as a logged-in user, testing auth-gated routes,
exercising a real request chain), use `iam-better-auth` instead — `iam-base` will silently satisfy
"the app has an iam module" while leaving that goal completely unmet, and discovering the mismatch
later means re-scaffolding, not just editing config.

### `forklaunch init router`

Adds a new controller + route + schema + service to an existing service or worker module.

**Router naming rules:**
- Must be lowercase kebab-case (letters and hyphens only)
- Must NOT contain numbers (e.g. `hl7` is invalid — use `lab-message` instead)
- Must NOT be a substring of the application name
- Must NOT be empty or contain spaces
- Must be unique within the service

## Workspace Architecture

```
my-app/                          # root pnpm workspace
├── package.json                 # workspaces: ["client", "src/modules/*"]
├── docker-compose.yml           # all services + infra (postgres, redis, minio)
├── .forklaunch/manifest.toml    # CLI source of truth (never edit manually)
├── client/                      # Next.js frontend
│   ├── package.json
│   ├── app/
│   ├── components/
│   └── lib/api.ts              # imports from universal-sdk
└── src/modules/                 # pnpm workspace for backend
    ├── pnpm-workspace.yaml
    ├── core/                    # @{{app-name}}/core — shared re-exports
    ├── monitoring/              # OpenTelemetry metrics
    ├── universal-sdk/           # auto-generated SDK
    ├── iam/                     # IAM service (auth, users, orgs)
    ├── billing/                 # billing service
    ├── my-service/              # your service
    └── my-worker/               # your worker
```

**Key design:**

- Each module is an **isolated package** with its own `package.json`, `node_modules`, and build
- Modules import shared code from `@{{app-name}}/core` (never cross-import directly)
- Docker Compose mounts each module as a volume with `tsx watch` for hot-reloading
- The `client/` lives at the top level alongside `src/modules/`, both in the root workspace
- `universal-sdk` auto-generates typed clients from each service's OpenAPI spec

## All CLI Commands

The Forklaunch CLI provides these commands:

| Command       | Description                                                     |
| ------------- | --------------------------------------------------------------- |
| `init`        | Initialize new projects (app, service, worker, library, router) |
| `change`      | Modify existing projects                                        |
| `delete`      | Remove projects                                                 |
| `deploy`      | Deploy applications to cloud                                    |
| `environment` | Manage environments                                             |
| `release`     | Create and manage releases                                      |
| `integrate`   | Integrate with external services                                |
| `openapi`     | Generate OpenAPI specifications                                 |
| `sdk`         | Generate client SDKs                                            |
| `sync`        | Sync local and remote state                                     |
| `config`      | Pull/push environment configuration                             |
| `depcheck`    | Check dependency alignment                                      |
| `eject`       | Eject from Forklaunch management                                |
| `login`       | Authenticate with platform                                      |
| `logout`      | Log out from platform                                           |
| `whoami`      | Show current user                                               |
| `version`     | Show CLI version                                                |

## Core Commands

### 1. Project Initialization (`init`)

Initialize new Forklaunch projects.

#### Initialize Application

```bash
forklaunch init app <app_name>

# Options:
--database <type>        # postgresql, mysql, mariadb, mssql, mongodb, libsql, sqlite, better-sqlite
--runtime <type>         # node, bun
--validator <type>       # zod, typebox
--http-framework <type>  # express, hyper-express (bun forces express)
--test-framework <type>  # vitest, jest
--formatter <type>       # prettier, biome
--linter <type>          # eslint, oxlint
--modules <module>       # billing-base, billing-stripe, iam-base, iam-better-auth,
                         # ecommerce-stripe, messaging-base, messaging-twilio, cac-base
--path <path>            # REQUIRED in practice — see below
--modules-path <path>    # src/modules | modules

# Example:
forklaunch init application my-platform --path . --modules-path src/modules \
  --runtime bun --http-framework express ...
```

**`--path` is effectively required.** `--help` calls it optional because the CLI
will prompt for it — which means that off a TTY, omitting it dies with
`Error: EOF`. Always pass it.

**Know where the workspace lands.** The manifest goes to the app root; the
pnpm/bun workspace goes **inside** `--modules-path`:

```
my-app/.forklaunch/manifest.toml       <- app root, and where forklaunch commands run
my-app/src/modules/package.json        <- workspace root, and where pnpm commands run
my-app/src/modules/pnpm-workspace.yaml
my-app/src/modules/<service>/          <- each module
```

So `cd my-app && pnpm install` fails with `ERR_PNPM_NO_PKG_MANIFEST`. Run
`forklaunch` commands from the app root and `pnpm` commands from
`<app>/<modules-path>`. Application names must be letters only — no digits, no
spaces.

#### Initialize Service

```bash
forklaunch init service <service_name>

# Options:
--path <app-path>       # Application path to scaffold in
--description "Service description"
--database postgresql|mysql|mariadb|mssql|mongodb|libsql|sqlite|better-sqlite
--infrastructure redis|s3  # Can specify multiple by repeating: -i redis -i s3
--mappers               # Generate mapper files for entity/DTO transformation

# Example:
forklaunch init service billing --path ./src/modules --database postgresql --description "Billing service"
```

#### Initialize Worker

```bash
forklaunch init worker <worker_name>

# Options:
--path <app-path>       # Application path to scaffold in
--type bullmq|kafka|database|redis
--database postgresql|mysql|mariadb|mssql|mongodb|libsql|sqlite|better-sqlite  # Only for database workers
--description "Worker description"
--mappers               # Generate mapper files

# Example:
forklaunch init worker email-worker --path ./src/modules --type bullmq
```

#### Initialize Library

```bash
forklaunch init library <library_name>

# Options:
--path <app-path>       # Application modules path to scaffold in
--description "Library description"

# Example:
forklaunch init library shared-utils --path ./src/modules --description "Shared utility contracts"
```

#### Initialize Router

```bash
forklaunch init router <router_name> --path <service_directory>

# Options:
--path <path>            # Path to the service directory (must be inside a service)
--infrastructure redis|s3  # Optional: add infrastructure support (can repeat)
--dryrun                 # Preview changes without applying

# Example:
forklaunch init router user-profile --path ./src/modules/platform-management
forklaunch init router payments --path ./src/modules/billing --infrastructure redis
```

### 2. Change Commands (`change`)

Modify existing project components safely.

#### Change Application

```bash
forklaunch change application [--path <app_root>]

# Options:
--path <path>            # Application root (default: current directory)
--runtime bun|node
--http-framework express|hyper-express
--formatter prettier|biome
--linter eslint|oxlint
--test-framework vitest|jest
--validator zod|typebox
-N <name>                # Rename the application
-D "description"         # Update description
--dryrun                 # Preview changes without applying
--confirm                # Skip confirmation prompts

# Example:
forklaunch change application --runtime bun --formatter biome --dryrun
forklaunch change application --runtime bun --formatter biome
```

#### Change Service

```bash
forklaunch change service [--path <service_directory>]

# Options:
--path <path>            # Service path (default: current directory)
--database postgresql|mysql|mariadb|mssql|mongodb|libsql|sqlite|better-sqlite
--infrastructure redis|s3
-N <name>                # Rename the service
-D "description"         # Update description
--to worker              # Convert service to worker (requires -t)
-t bullmq|kafka|database|redis  # Worker type (required with --to worker)
--dryrun                 # Preview changes without applying
--confirm                # Skip confirmation prompts

# Example:
forklaunch change service --path ./src/modules/my-service --database postgresql --dryrun
forklaunch change service --path ./src/modules/email --to worker --type bullmq --dryrun
```

##### `--to worker` ADDS a worker. It does not remove the HTTP surface.

The word "convert" is misleading and has caused at least one wrong call. What
actually happens:

- **`server.ts` is kept in full.** Routers stay mounted; the command only
  prepends a `// TODO: Review` comment above the existing file. Nothing is
  deleted.
- **`type = "Worker"` in `manifest.toml` means "this module HAS a worker."** It
  does not mean the module stopped being a service. Proof in this repo:
  `observability-api` is `type = "Worker"` and still has
  `routers = ["monitoring", "observability-api"]` and a live HTTP server, as
  does `deployment-agent-worker`.

So a module that must keep public endpoints (an OAuth callback, a webhook, a
gateway) is still the right candidate for `--to worker`. **Do not hand-roll a
`worker.ts` to "protect" the HTTP surface** — that bypasses the manifest and
silently violates the "don't edit `manifest.toml` by hand" rule, because the
manifest keeps saying `type = "Service"` while the module ships a worker.

##### It requires a `.env.local`, which is gitignored

`change service --to worker` reads `<module>/.env.local` and **panics** with
`called 'Option::unwrap()' on a 'None' value` if it is absent. `*.env.local` is
gitignored repo-wide, so a fresh checkout has none and the command fails on
every module. Create one from the service's compose `environment:` block first.

##### What it writes

`worker.ts`, `<name>EventRecord` entity + types, `registrations.ts` (adds
`WorkerProducer`/`WorkerConsumer`/`WorkerOptions`), `package.json`
(`dev:worker`, `start:worker`), `docker-compose.yaml` (a worker service),
`server.ts` (TODO banner), `.env.local` (adds `QUEUE_NAME`), and
`.forklaunch/manifest.toml`. Also writes `README-MIGRATION.md` with next steps.

##### Reconcile after running — the generator makes assumptions

Its templates assume a fresh scaffold, so on a hand-modified module expect to
fix, in roughly this order:

1. **Install the new deps** — it adds `@forklaunch/implementation-worker-<type>`
   and `@forklaunch/interfaces-worker` to `package.json` but does not install.
2. **`registrations.ts` is edited IN PLACE and keeps what it finds.** It does
   not regenerate from a template, so hand-added config entries and service
   registrations survive — verified by converting a scaffold carrying both.
   (An earlier version of this note claimed the rewrite dropped
   `APP_BASE_DOMAIN`. That was wrong: a `git revert` in the platform repo
   removed it, not the CLI. Diff before blaming the generator.)
3. **Entity used as a type.** Generated code writes
   `WorkerProcessFunction<FooEventRecord>`, but `defineEntity(...)` yields a
   *value*. Alias it: `type FooEventRecordType = InferEntity<typeof FooEventRecord>`.
4. **Import path.** `worker.ts` imports from `./services/<name>.service`; this
   repo uses `./domain/services/`.
5. **Register the event entity in the entities barrel.** The generated
   `<name>EventRecord` entity is created but not exported from
   `persistence/entities/index.ts`, and that barrel is how the ORM discovers
   entities — so no migration is ever generated for the worker's table and the
   worker has nothing to read.
6. **Add `QUEUE_NAME` to `.env.test`.** The conversion writes it into
   `.env.local` only, so the module fails config validation under vitest with
   `Path: QUEUE_NAME / Message: Required`.
7. **Parameterize the `WorkerProducer` registration.** It is emitted
   unparameterized while `WorkerConsumer` is typed correctly, so `enqueueJob`
   degrades to the base `WorkerEventEntity` and rejects the `message` column the
   generated entity actually has.
8. **Write the handlers.** It imports `processEvents` / `processErrors`, which
   do not exist yet. Type them against the real contract:
   `WorkerProcessFunction<T> = (events: T[]) => Promise<WorkerProcessFailureResult<T>[]>`
   and `WorkerFailureHandler<T> = (results: WorkerProcessFailureResult<T>[]) => Promise<void>`,
   where `WorkerProcessFailureResult<T> = { value: T; error: Error }`.

##### Choosing `-t`

`database` keeps the queue in the same Postgres as the domain rows, so a job and
the state it mutates commit together and cannot drift — prefer it when the work
mutates rows that already carry its status. `bullmq` (Redis) is the default
elsewhere in this repo and is better for high-throughput or cross-service fanout.

#### Change Worker

```bash
forklaunch change worker [--path <worker_directory>]

# Options:
--path <path>            # Worker path (default: current directory)
--type bullmq|kafka|database|redis
--database postgresql|mysql|mariadb|mssql|mongodb|libsql|sqlite|better-sqlite
-N <name>                # Rename the worker
-D "description"         # Update description
--to service             # Convert worker to service
--dryrun                 # Preview changes without applying
--confirm                # Skip confirmation prompts

# Example:
forklaunch change worker --path ./src/modules/email-worker --type kafka
forklaunch change worker --path ./src/modules/email-worker --to service --dryrun
```

#### Change Router

```bash
forklaunch change router [--path <service_directory>]

# Options:
--path <path>            # Service path (must be inside a service directory)
-e <existing-name>       # Current name of the router to change
-N <new-name>            # Rename the router
--add-mappers            # Generate mapper files from existing schemas and entities
--dryrun                 # Preview changes without applying
--confirm                # Skip confirmation prompts

# Example:
forklaunch change router --path ./src/modules/platform-management -e user-profile -N user-management --dryrun
forklaunch change router --path ./src/modules/billing --add-mappers
```

### 3. Delete Commands (`delete`)

Remove project components. **Destructive — get explicit user approval before running any of
these**, and confirm the exact target (service/worker/library/router name) they approved, not just
that "delete something" was approved.

```bash
# Delete service
forklaunch delete service <service_name>

# Delete worker
forklaunch delete worker <worker_name>

# Delete library
forklaunch delete library <library_name>

# Delete router (use --path to specify the service directory)
forklaunch delete router <router_name> --path <service_directory>

# Options:
-c, --continue         # Continue/skip the eject confirmation prompt — only pass this after
                        # approval already covers the exact target above; it removes the CLI's
                        # own safety prompt, not the need for yours.

# Examples:
forklaunch delete service old-billing
forklaunch delete worker deprecated-processor
forklaunch delete router legacy-api --path ./src/modules/platform-management
```

### 4. Deploy Commands (`deploy`)

Deploy applications to the cloud. `deploy` has subcommands — there is no bare `forklaunch deploy`.
Deploying is also effectively irreversible in the moment (see rollback caveats elsewhere in this
file) — get explicit approval for the exact release/environment/region before running `create`.

```bash
forklaunch deploy create --release <version> --environment <name> --region <region>

# Required:
--release <version>     # Release version to deploy (alias: -v/--version)
--environment <name>    # Environment name (dev, staging, production)
--region <region>       # AWS region (e.g. us-east-1)

# Other options:
--dry-run              # Preview only — does not deploy, so it doesn't need approval on its own
--full                 # Force a full deployment with Pulumi state refresh
--no-wait              # Don't wait for deployment to complete
--cluster-type <type>  # platform-shared | org-shared | dedicated (first deploy to an env/region only)

# There is no --auto-approve; a missing required env var (e.g. ENCRYPTION_KEY) can still drop the
# command into a blocking interactive prompt on a real TTY — see the ENCRYPTION_KEY notes below.
# --node-env <production|development> avoids one such prompt on a first deploy.

# Examples:
forklaunch deploy create --release 1.2.0 --environment staging --region us-east-1
forklaunch deploy create --release 1.2.0 --environment production --region us-east-1 --dry-run
forklaunch deploy info -d <id>           # status of a deployment (omit -d for the latest)
forklaunch deploy logs                   # logs of the latest deployment for this env/region
forklaunch deploy logs <id> --all        # the whole log for one deployment
forklaunch deploy logs -l error          # only the error lines — start here on a failed deploy
forklaunch deploy destroy ...            # tear down application infrastructure
```

**The first deploy asks where the app should run.** The first deployment of an
application to a given environment × region is gated: the platform answers `428`
with the three placements and a monthly cost estimate for each, and the deploy
does not proceed until one is chosen.

```
First deploy — choose a cluster
  • Platform shared cluster       ~$2.00/mo
  • Organization shared cluster   ~$31.00/mo
  • Dedicated cluster             ~$108.00/mo
```

- On a terminal, the CLI prompts. Without one it exits telling you the flag to
  re-run with — so **in a non-interactive session, pass `--cluster-type`**, or
  settle it earlier with `forklaunch app create --cluster-type`.
- A placement chosen at `app create` satisfies the gate; it is not asked twice.
- Options can come back **unavailable** with a reason: no org compute pool in the
  region, fewer than two apps in the org to amortize a shared host, or the app's
  declared compliance frameworks forbidding cross-tenant compute.
- The estimates are compute only, per month. They are the honest reason to ask —
  the spread between the cheapest and the most isolated option is roughly 50×.

Explain the tradeoff before the user picks. Shared placements are cheaper because
the app's containers share hosts; a dedicated cluster gives it its own load
balancer and network path. See `/deploy-mode`, which covers this decision and the
single-app-vs-managed one together.

**After any deploy, check it — and read the logs when it failed.**

```bash
forklaunch deploy info -e <env> -r <region>
forklaunch observe logs -e <env> --deployment <id> --source cloudwatch --limit 200
```

`deploy logs` reads the deployment's own log (what Pulumi and the container build
emitted while deploying). For the logs a running service emits afterwards, use
`forklaunch observe logs`.

`--source cloudwatch` is not optional for a startup failure: the default `otel`
source only carries what the app's telemetry pipeline flushed, and a container
that dies before it starts flushes nothing — so the logs look empty when they are
not. Missing environment variables are the most common cause, in two shapes (a
pre-flight block that names the keys, and a runtime crash loop). `/investigator`
covers both.

### 5. Environment Commands (`environment`)

Manage application environments.

```bash
# Create environment
forklaunch environment create <name>

# Options:
--region <region>      # AWS region
--description "Environment description"

# List environments
forklaunch environment list

# Delete environment
forklaunch environment delete <name>

# Show environment details
forklaunch environment show <name>

# Examples:
forklaunch environment create staging --region us-west-2
forklaunch environment list
forklaunch environment show production
```

### 6. Release Commands (`release`)

Create and manage application releases.

`release` has four subcommands: `create`, `info`, `list`, `eject`. There is no
`release show` or `release rollback` — rollback lives on
`forklaunch deploy rollback`. (`release list` is recent; older CLIs have only
the first two and `eject`. Check `forklaunch release --help` if unsure.)

```bash
forklaunch release create --version <version> --local --yes

# Options:
-v, --version <version>   # REQUIRED. Release version, e.g. 1.0.0 (aliases: -r, --release)
-n, --notes <notes>       # Release notes (optional)
    --local               # Package local code and upload to S3
    --git                 # Use the git-based release flow
    --skip-sync           # Skip syncing projects with the manifest first
    --dry-run             # Simulate without uploading
-y, --yes                 # Skip confirmation prompts
-p, --path <base_path>

forklaunch release info                  # details for a release
forklaunch release list                  # releases for this application
forklaunch release eject                 # emit the Pulumi IaC a release would deploy
```

**Non-interactive callers must pass a mode and `--yes`.** With neither `--local`
nor `--git`, the command prompts for the release mode and dies with
`Error: IO error: not a terminal` off a TTY. The working form is:

```bash
forklaunch release create --version 0.1.0 --local --yes
```

`release create` **runs `pnpm install` and exports OpenAPI specs via `tsx`**, so
it needs the Node toolchain present even though the CLI itself does not. See
`/prereqs`.

### 7. Integrate Commands (`integrate`)

`integrate` links a **local checkout to an existing platform application**. That is
its entire job — it takes no service name.

```bash
forklaunch integrate --app <platform-application-id>

# Options:
-a, --app <app>         # Platform application ID to link to (REQUIRED)
-p, --path <base_path>  # Path to application root (optional)
```

It writes `platform_application_id` and `platform_organization_id` into
`.forklaunch/manifest.toml`. `forklaunch app create` runs this step for you, so
`integrate` on its own is for linking to an application that already exists —
one created in the dashboard, or created earlier with `--no-integrate`.

**There is no `integrate github`, `integrate stripe`, `integrate aws`,
`integrate datadog` or `integrate sentry`.** Third-party providers go two other
ways:

- **GitHub** — the `forklaunch github` family (section 7a below).
- **Everything else** — credentials are environment variables, set with
  `forklaunch config set`. See `/integrations` for the full runbook, including
  the exact Stripe variable names.

### 7a. GitHub Commands (`github`)

Connect an application to a repository and configure push-to-deploy.

```bash
forklaunch github status      # installation + this app's repo connection
forklaunch github install     # prints an installation link — a human must open it
forklaunch github connect --repo <url> [options]
forklaunch github disconnect

# connect options:
-r, --repo <url>                       # REQUIRED, e.g. https://github.com/org/repo
    --default-branch <branch>          # default: main
    --auto-deploy                      # release + deploy on every push
    --release-environment <env>        # environment autodeploys target
    --region <region>                  # region autodeploys target
    --branch-mapping <BRANCH=ENV>      # repeatable
-p, --path <base_path>

# Examples:
forklaunch github connect --repo https://github.com/acme/portal --default-branch main
forklaunch github connect --repo https://github.com/acme/portal --default-branch main \
  --auto-deploy --release-environment staging --region us-west-2
```

`install` cannot complete on its own — GitHub requires a human to grant the App
access. Print the link, wait, then confirm with `status`.

`--auto-deploy` spends money on every push with no further confirmation. Get
explicit agreement before enabling it on a production branch.

### 7b. Application Commands (`app`)

Create and inspect the **platform** application record — the control-plane object
your local checkout links to.

```bash
forklaunch app create [options]     # create + integrate in one step
forklaunch app services             # list services and workers
forklaunch app domain               # inspect the custom domain
forklaunch app resize ...           # resize components (cuts a release + deploys)
forklaunch app route <id>           # route details
forklaunch app controller <id>      # controller details
```

`app create` settles **three independent decisions** at registration, so the
first deploy has nothing to ask:

```bash
forklaunch app create \
  --name "Clinic Portal" \
  --description "Patient records for dental practices" \
  --cluster-type dedicated \
  --compliance-framework HIPAA \
  --managed

# Options:
-n, --name <name>                       # defaults to app_name from manifest.toml
-D, --description <description>
    --cluster-type <type>               # WHERE it runs: platform-shared | org-shared | dedicated
    --compliance-framework <framework>  # WHAT RULES its data is under; repeatable
                                        # HIPAA | PCI-DSS | "SOC 2" | GDPR | CCPA
    --managed                           # HOW MANY run: one instance per customer
    --no-integrate                      # create the platform app without linking this checkout
-p, --path <base_path>
```

The three are orthogonal — a managed app still has a placement and still declares
its frameworks. They interact in one direction only: **a compliance-scoped app
cannot run on cross-tenant compute**, so declaring HIPAA, SOC 2 or PCI-DSS
removes `platform-shared` from the available placements.

Omitting `--cluster-type` is fine — the first deploy will ask instead. Omitting
`--compliance-framework` means "not assessed", which is *not* the same as "none
apply": an undeclared app is unconstrained. Ask the user rather than leaving it
blank by default.

There is no `app delete`. Removing an application record is a dashboard action.

### 8. OpenAPI Commands (`openapi`)

Export OpenAPI specifications from your code. There is no `generate` or `validate` subcommand —
only `export`.

```bash
# Export OpenAPI specs for the application
forklaunch openapi export

# Options:
-p, --path <base_path>  # Path to application root (optional)
-o, --output <output>   # Output directory (default: .forklaunch/openapi)

# Examples:
forklaunch openapi export
forklaunch openapi export --output ./specs
```

### 9. SDK Commands (`sdk`)

Generate client SDKs from OpenAPI specifications.

```bash
# Generate SDK for all services
forklaunch sdk generate

# Generate for specific service
forklaunch sdk generate --service <service_name>

# Generate for specific language
forklaunch sdk generate --language <language>

# Supported languages:
# - typescript
# - python
# - go
# - java
# - swift
# - kotlin

# Options:
--output <path>        # Output directory
--package-name <name>  # Package/module name

# Examples:
forklaunch sdk generate --language typescript
forklaunch sdk generate --service iam --language python
forklaunch sdk generate --language go --package-name forklaunch-client
```

### 10. Sync Commands (`sync`)

Synchronize local and remote state.

```bash
# Sync all changes with platform
forklaunch sync

# Pull changes from platform
forklaunch sync pull

# Push changes to platform
forklaunch sync push

# Show sync status
forklaunch sync status

# Options:
--force                # Force sync, overwrite conflicts
--dry-run             # Show what would be synced

# Examples:
forklaunch sync
forklaunch sync pull --dry-run
forklaunch sync push --force
forklaunch sync status
```

### 11. Config Commands (`config`)

Pull and push environment configuration between local `.env` files and the platform.

```bash
# Pull environment config to a local .env file
forklaunch config pull -a <APP_ID> -r <REGION> -e <ENV> [-s <SERVICE>] [-o <FILE>]

# Push a local .env file to the platform
forklaunch config push -a <APP_ID> -r <REGION> -e <ENV> [-i <FILE>]

# Options for pull:
--app / -a          # Application ID (required)
--region / -r       # Region, e.g. us-east-1 (required)
--environment / -e  # Environment name, e.g. production (required)
--service / -s      # Filter to a specific service name (optional)
--output / -o       # Output file path (defaults to <environment>.env)

# Options for push:
--app / -a          # Application ID (required)
--region / -r       # Region (required)
--environment / -e  # Environment name (required)
--input / -i        # Input file path (defaults to <environment>.env)

# Examples:
forklaunch config pull -a app-123 -r us-east-1 -e production
forklaunch config pull -a app-123 -r us-east-1 -e staging -s billing-service -o .env.staging
forklaunch config push -a app-123 -r us-east-1 -e production
forklaunch config push -a app-123 -r us-east-1 -e production -i ./config/.env.prod
```

#### DANGER: `pull` is per-service, `push` is environment-wide

`pull` accepts `--service`. **`push` does not.** Push applies the input file to the WHOLE
environment, so pushing a file produced by a filtered pull deletes every key belonging to every
other service.

```bash
# CATASTROPHIC — do not do this:
forklaunch config pull  ... -s deployment-agent-worker-worker -o worker.env   # ~1 service's keys
forklaunch config push  ... -i worker.env                                     # applied to ALL services
```

This caused a multi-hour production outage on 2026-08-09. The blast radius went well past the
config store, because a deploy regenerates infrastructure from it:

1. Every other service's env vars were dropped from the config store.
2. **SSM parameters were deleted** for keys no longer present (secret VALUES are unrecoverable
   unless they exist in SSM parameter *history* or an old `production.env`).
3. Existing SSM parameters were **overwritten by value** — e.g. `iam-better-auth-secret` was
   replaced, so better-auth could no longer decrypt its stored JWKS private key and every
   `get-session` returned 500 (which then looks like auth rate-limiting, because clients retry
   the device-code flow, which polls).
4. The next deploy **regenerated the exec-role IAM policies** from the narrowed config, revoking
   `ssm:GetParameters` on the dropped parameters, so tasks died with `AccessDeniedException`.
5. The next deploy also **regenerated task definitions**, silently reverting any manual ECS
   rollback. Recovery does not stick until the config store itself is correct.

The underlying AWS account belongs to ForkLaunch, never the customer, regardless of cluster type —
an agent working in a customer's project has no AWS credentials for it, full stop. None of the
`aws ecs`/`aws ssm` commands below are ever runnable from that context; they exist only as an
internal ForkLaunch-operator runbook (this is the actual playbook used to resolve the outage
above), not something to suggest to a customer or run on their behalf.

**Rules**

- NEVER `push` a file that came from a `--service`-filtered `pull`. Only push a file from a full
  (unfiltered) pull of the same environment.
- Before any push, diff the input against a full `config pull` of the same environment and confirm
  no service loses keys.
- Treat `push` as a destructive, environment-wide replace. Take a full unfiltered pull first and
  keep it as the rollback artifact — this is the only recovery path available to a customer-facing
  agent; everything below this point is ForkLaunch-internal only.

**If it has already happened**

- Customer-facing agent: restore from the full unfiltered `config pull` taken before the push (see
  Rules above) via `config push`, then redeploy. If no such backup exists, this is a ForkLaunch
  support escalation — don't guess at secret values or fabricate a replacement config, and don't
  suggest AWS CLI commands to the customer.
- ForkLaunch operator only, with real AWS access to ForkLaunch's own account: before any push,
  cross-check the running task definitions —
  `aws ecs describe-task-definition --task-definition <td> --query 'taskDefinition.containerDefinitions[0].environment[*].name'`
  — and verify SSM before/after a push:
  `aws ssm get-parameters-by-path --path /<env-prefix> --query 'length(Parameters)'`. If damage has
  already happened, task definitions hold the correct secret ARNs and non-secret env values — use
  them to identify which keys and versions need restoring (`aws ecs describe-task-definition`).
  For any container env entry that is itself a secret ARN reference, restore by ARN/version, not by
  reading the plaintext value. Overwritten (not deleted) parameters are recoverable from history:
  first list versions without decrypting (`aws ssm get-parameter-history --name <param>`, which
  returns every version but never decrypts any of them) and match the version number by
  `LastModifiedDate` around the push. `--with-decryption` on `get-parameter-history` decrypts the
  WHOLE history at once, not just one version — instead fetch only the identified version with
  `aws ssm get-parameter --name <param>:<version> --with-decryption` (colon-suffixed version
  label), and never paste the decrypted value into chat, logs, or a
  committed file — pipe it straight into the restore command.
- Fix the config store BEFORE deploying again, or the deploy will re-apply the damaged state and
  undo any ECS/IAM repair.

Behavioral asymmetry to know: `config pull` fails with "Environment not found" if the environment doesn't exist yet, but `config push` **auto-creates** the environment. On a never-deployed app, push first, then pull.

The `.env` file uses comment headers to separate variables by source:

```env
# application
DATABASE_URL=postgres://...

# billing-service (svc-id-123)
STRIPE_KEY=sk_test_...
```

### 12. Dependency Check (`depcheck`)

Check dependency alignment across projects.

```bash
forklaunch depcheck

# Options:
--fix                  # Auto-fix mismatched dependencies
--strict              # Fail on any mismatches
--ignore <packages>    # Ignore specific packages

# Examples:
forklaunch depcheck
forklaunch depcheck --fix
forklaunch depcheck --strict
forklaunch depcheck --ignore "typescript,eslint"
```

### 13. Eject Command (`eject`)

Eject from Forklaunch management (irreversible).

```bash
forklaunch eject

# Options:
--keep-dependencies    # Keep Forklaunch dependencies
--confirm             # Skip confirmation prompt

# WARNING: This is irreversible!
# Example:
forklaunch eject --keep-dependencies
```

### 14. Authentication Commands

#### Login

```bash
forklaunch login

# Options:
--email <email>        # Login email
--token <token>        # API token for CI/CD

# Examples:
forklaunch login
forklaunch login --email user@example.com
forklaunch login --token $FORKLAUNCH_TOKEN  # For CI
```

#### Logout

```bash
forklaunch logout

# Example:
forklaunch logout
```

#### Whoami

```bash
forklaunch whoami

# Shows:
# - Current user
# - Organization
# - Email
# - Plan

# Example:
forklaunch whoami
```

### 15. Version Command

```bash
forklaunch version

# Shows:
# - CLI version
# - Framework versions
# - Latest available version

# Example:
forklaunch version
```

### Modifying Components

**Change Application Settings:**

```bash
forklaunch change application

# Options:
--runtime bun|node
--http-framework express|hyper-express
--formatter prettier|biome
--linter eslint|oxlint
--test-framework vitest|jest
--validator zod|typebox
--dry-run              # Preview changes without applying
```

**Change Services:**

```bash
forklaunch change service <service_name>

# Options:
--database postgresql|mysql|mariadb|mssql|mongodb|libsql|sqlite|better-sqlite
--infrastructure redis|s3
--new-name <name>
--description "New description"
--dry-run
```

**Change Workers:**

```bash
forklaunch change worker <worker_name>

# Options:
--type bullmq|kafka|database
--database postgresql|mysql|mariadb|mssql|mongodb|libsql|sqlite|better-sqlite
--new-name <name>
--description "New description"
--dry-run
```

**Change Routers:**

```bash
forklaunch change router --path <service_directory> -e <existing-name> -N <new-name>

# Options:
--path <path>            # Service path (default: current directory)
-e <existing-name>       # Current router name
-N <new-name>            # New router name
--add-mappers            # Generate mapper files from existing schemas
--dryrun
--confirm
```

### Deleting Components

```bash
forklaunch delete service <service_name>
forklaunch delete worker <worker_name>
forklaunch delete library <library_name>
forklaunch delete router <router_name> --path <service_directory>
```

### Development Utilities

```bash
# Check dependency alignment across projects
forklaunch depcheck

# Eject from ForkLaunch management
forklaunch eject

# Pull/push environment configuration
forklaunch config pull -a <APP_ID> -r <REGION> -e <ENV>
forklaunch config push -a <APP_ID> -r <REGION> -e <ENV>
```

### Platform Commands

```bash
# Authentication
forklaunch login
forklaunch logout
forklaunch whoami

# Version info
forklaunch version
```

## Forklaunch Best Practices

### 1. Project Structure Conventions

#### Service Structure

```
src/modules/<service-name>/
├── api/
│   ├── controllers/     # HTTP request handlers
│   ├── routes/         # Route definitions
│   └── middleware/     # Service-specific middleware
├── domain/
│   ├── services/       # Business logic
│   ├── schemas/        # Validation schemas (Zod/TypeBox)
│   ├── types/          # TypeScript types
│   └── utils/          # Domain utilities
├── persistence/
│   ├── entities/       # Database entities (MikroORM)
│   ├── repositories/   # Data access layer
│   └── migrations/     # Database migrations
├── registrations.ts    # Dependency injection setup
├── server.ts          # Service entry point
└── worker.ts          # Worker entry point (if applicable)
```

#### Worker Structure

```
src/modules/<worker-name>/
├── api/
│   ├── controllers/     # Worker job handlers
│   └── routes/         # Worker queue routes
├── domain/
│   ├── services/       # Processing logic
│   ├── schemas/        # Validation schemas
│   └── types/          # TypeScript types
├── registrations.ts    # Dependency injection
└── worker.ts          # Worker entry point
```

#### Library Structure

```
src/modules/<library-name>/
├── domain/
│   ├── services/       # Shared services
│   ├── types/          # Shared types
│   └── utils/          # Shared utilities
└── index.ts           # Public exports
```

### 2. Naming Conventions

- **Services**: Lowercase with hyphens (e.g., `platform-management`, `user-auth`)
- **Workers**: Lowercase with hyphens, `-worker` suffix (e.g., `deployment-agent-worker`)
- **Libraries**: Lowercase with hyphens (e.g., `core`, `monitoring`, `universal-sdk`)
- **Routers**: camelCase (e.g., `billingPortal`, `organizationManagement`)
- **Files**: Lowercase with hyphens (e.g., `deployment.service.ts`, `user.entity.ts`)
- **Classes**: PascalCase (e.g., `DeploymentService`, `UserEntity`)

### 3. File Naming Patterns

Follow these patterns consistently:

- Controllers: `<name>.controller.ts`
- Services: `<name>.service.ts`
- Entities: `<name>.entity.ts`
- Schemas: `<name>.schema.ts`
- Types: `<name>.types.ts`
- Routes: `<name>.routes.ts`
- Tests: `<name>.test.ts` or `<name>.spec.ts`

### 4. Dependency Injection Pattern

Always use the registrations.ts pattern:

```typescript
// registrations.ts
import { DependencyContainer } from "@forklaunch/core";

export function registerDependencies(container: DependencyContainer) {
  // Register services
  container.registerSingleton("DeploymentService", DeploymentService);
  container.registerSingleton("PulumiGeneratorService", PulumiGeneratorService);

  // Register repositories
  container.registerSingleton("DeploymentRepository", DeploymentRepository);
}
```

### 5. Manifest-Driven Development

The `.forklaunch/manifest.toml` is the source of truth:

- DO NOT manually edit the manifest - use CLI commands
- The manifest tracks all services, workers, libraries, and routers
- Contains application-wide configuration (runtime, frameworks, tools)
- Used for infrastructure generation and deployment

### 6. Router Organization

Group related endpoints into routers:

- One router per resource or domain concept
- Use descriptive router names (e.g., `deployment`, `application`, `user`)
- Keep routers focused and cohesive

### 7. Database and Infrastructure

**Database Configuration:**

- Services typically use `postgresql` in production
- Use `redis` for caching and session storage
- Workers can use `bullmq` (backed by Redis) for job queues

**Infrastructure Resources:**

```toml
[projects.resources]
database = "postgresql"
cache = "redis"

# For workers
[projects.resources]
cache = "bullmq"
```

**Where a component runs — and who decides.**

"Hosting type" means two different things, and conflating them is the fastest way
to a wrong answer. Both end up in the same `hostingType` column:

| you mean | vocabulary | who decides |
|---|---|---|
| which **substrate** the app sits on | `platform-shared` / `org-shared` / `dedicated` — a **cluster type** | the control plane, at `app create` or first deploy |
| which **compute host** one component needs | `ecs-fargate` / `ecs-ec2` — a **hosting type** | the manifest seeds it; the control plane owns it after |

**The control plane is authoritative.** Cluster placement is recorded on the
application record — set with `forklaunch app create --cluster-type`, or chosen at
the first deploy to an environment/region — and it maps onto every component's
hosting row. The local manifest does not override it and cannot fight it on each
deploy.

**The manifest seeds the initial value.** A component's `[projects.metadata]`
entry is read **when the platform first learns about that component**, and then
the control plane owns it:

```toml
[projects.metadata]
type = "bullmq"
hostingType = "ecs-ec2"   # optional; seeds the component's initial hosting type.
                          # Absent means "inherit the app's cluster placement".
privileged = true         # optional; only valid with "ecs-ec2" (Fargate forbids
                          # privileged mode). Needed when the container must build
                          # images or run a sandboxed build (Docker/nsjail).
```

- `hostingType` (alias `hosting_type`): `"ecs-fargate"` or `"ecs-ec2"`. Use
  `ecs-ec2` for a worker that must build container images locally — the same
  reason the deployment-agent-worker is EC2-hosted.
- `privileged`: boolean, absent means false, only honored on `ecs-ec2`.

Editing the value in `manifest.toml` **after** the platform already knows the
component does nothing — the seed has already been taken. Change it with the
knob instead.

**Changing placement after the fact.** A placement change is *recorded* now and
*enacted by the next deployment* — nothing moves the moment you change it.

- Before the first deploy, the record is all there is: the change is free and the
  first deploy places the app there.
- After the app is deployed, recording the change needs `--confirm-downtime`,
  because the next deployment will copy its data to the new substrate and cut
  over. That deployment does not deploy your release directly; it hands off to
  the migration, which redeploys on the target when the copy completes.

Either way it is one command against the control plane, never a manifest edit:

```bash
forklaunch app hosting                             # current placement, and whether a change is free
forklaunch app hosting --cluster-type dedicated \
  --region us-west-2 [--confirm-downtime]          # record the change
```

None of this is a Pulumi or generator change: the pulumi-generator already
branches on the resolved value.

### 8. Testing Patterns

Follow the testing hierarchy:

```
<module>/
├── domain/
│   └── services/
│       └── __test__/
│           └── deployment.service.test.ts
├── api/
│   └── controllers/
│       └── __test__/
│           └── deployment.controller.test.ts
```

### 9. Import Organization

Order imports consistently:

```typescript
// 1. External dependencies
import { injectable, inject } from "tsyringe";
import { Request, Response } from "express";

// 2. Forklaunch framework
import { BaseService } from "@forklaunch/core";
import { z } from "@forklaunch/validator/zod";

// 3. Internal cross-module imports
import { CoreLogger } from "@modules/core";

// 4. Local module imports
import { DeploymentService } from "../services/deployment.service";
import { DeploymentSchema } from "../schemas/deployment.schema";
```

### 10. Safe Change Workflow

Always follow this workflow when modifying projects:

```bash
# 1. Commit current state
git add .
git commit -m "Before changing database to PostgreSQL"

# 2. Preview changes
forklaunch change service my-service --database postgresql --dry-run

# 3. Make the change
forklaunch change service my-service --database postgresql

# 4. Install dependencies
pnpm install  # or bun install

# 5. Test
pnpm test
pnpm dev

# 6. Commit changes
git add .
git commit -m "Changed my-service database to PostgreSQL"
```

### 11. Development Commands

All commands use `pnpm` (or `bun` if runtime is bun). These are run from the **root** of the project.

```bash
# First time setup (run in order)
pnpm install            # Install all dependencies
pnpm dev                # Start Docker Compose (postgres, redis, minio, all services)
pnpm database:setup     # Apply migrations + seed data (run AFTER pnpm dev, services must be up)

# IMPORTANT: pnpm database:setup must run AFTER pnpm dev
# The database container must be running before migrations can execute.
# Order: pnpm dev → wait for services → pnpm database:setup

# Daily development
pnpm dev                # Start all services (docker compose up with hot-reload)

# Database operations (run from individual module dir OR root)
pnpm database:setup     # Migrations + seed (requires running containers)
pnpm migrate:create     # Create new migration
pnpm migrate:up         # Run pending migrations
pnpm migrate:down       # Rollback last migration

# Testing
pnpm test              # Run all tests
pnpm test:watch        # Watch mode
pnpm test:coverage     # With coverage

# Code quality
pnpm lint              # Run linter
pnpm format            # Format code
pnpm type-check        # TypeScript type checking
```

### 12. Preconfigured Modules

ForkLaunch supports preconfigured modules for common functionality:

| Module            | Description                               |
| ----------------- | ----------------------------------------- |
| `billing-base`    | Billing hooks only (no payment provider)  |
| `billing-stripe`  | Full Stripe billing integration           |
| `iam-base`        | IAM authorization only (no auth provider) |
| `iam-better-auth` | Better Auth authentication implementation |

Add modules during `init application` with `-m` (repeatable) or later with `init module`:

```bash
# During application init
forklaunch init application my-app ... -m billing-stripe -m iam-better-auth

# Add to existing application
forklaunch init module billing --path ./src/modules --module billing-stripe --database postgresql
forklaunch init module iam --path ./src/modules --module iam-better-auth --database postgresql
```

### 13. Incremental Adoption

Forklaunch can be adopted incrementally:

- Drop into existing Express apps
- Upgrade routes one at a time
- Use framework features as needed
- No lock-in - you can always eject

### 14. Multi-Environment Configuration

Structure environment-specific config:

```
.env.local          # Local development
.env.development    # Development environment
.env.staging        # Staging environment
.env.production     # Production environment
```

### 15. OpenAPI and SDK Generation

Forklaunch auto-generates:

- OpenAPI specs from route definitions
- AsyncAPI specs for workers
- Type-safe SDKs for clients
- OpenTelemetry metrics, logs, traces

Specs are available at:

- `.forklaunch/openapi/<module>/openapi.json`
- `.forklaunch/openapi/<module>/asyncapi.json`

## Handler Contract Details & Typed req/res

ForkLaunch handlers use a **contract object** as the second argument to `handlers.get()`, `handlers.post()`, etc. The contract drives full type inference for the `req` and `res` callback parameters — no manual type annotations needed.

### Contract Fields

| Field             | Drives Type Of                     | Description                                      |
| ----------------- | ---------------------------------- | ------------------------------------------------ |
| `name`            | —                                  | Handler name (used in OpenAPI and tracing)       |
| `summary`         | —                                  | Handler description                              |
| `params`          | `req.params`                       | URL path parameters (e.g. `{ id: string }`)      |
| `query`           | `req.query`                        | Query string parameters                          |
| `body`            | `req.body`                         | Request body (POST/PUT/PATCH only)               |
| `requestHeaders`  | `req.headers`                      | Typed request headers                            |
| `responseHeaders` | `res.setHeader()`                  | Typed response headers (constrains allowed keys) |
| `responses`       | `res.status(N).json()` / `.send()` | Map of status code → response body type          |
| `auth`            | `req.session`                      | Authentication config (see below)                |
| `options`         | —                                  | Validation mode, MCP/OpenAPI toggles             |

### How Typing Works

```typescript
export const getUser = handlers.get(
  schemaValidator,
  "/:id",
  {
    name: "Get User",
    summary: "Gets a user by ID",
    params: { id: string }, // → req.params.id: string
    query: { includeRoles: optional(boolean) }, // → req.query.includeRoles?: boolean
    requestHeaders: { "x-tenant": string }, // → req.headers['x-tenant']: string
    responseHeaders: { "x-request-id": string }, // → res.setHeader('x-request-id', ...)
    access: "protected",
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_URL },
      allowedRoles: PLATFORM_VIEWER_ROLES,
    },
    responses: {
      200: { id: string, name: string }, // → res.status(200).json({ id, name })
      404: string, // → res.status(404).send('Not found')
      500: string,
    },
  },
  async (req, res) => {
    // req.params, req.query, req.body, req.headers, req.session — all fully typed
    // res.status(200).json() — only accepts the shape defined in responses[200]
    // res.setHeader('x-request-id', '...') — only accepts keys from responseHeaders
  },
);
```

### Key Typing Rules

1. **`req.params`** — Inferred from `params` in the contract. If the path has `:id`, the contract must include `params: { id: ... }`.
2. **`req.body`** — Only available for `handlers.post()`, `.put()`, `.patch()`. Typed from `body` field. Supports multiple content types:
   - Plain object → `application/json` (default)
   - `{ text: string, contentType?: 'text/plain' }` → text body
   - `{ file: file, contentType?: 'application/octet-stream' }` → file upload
   - `{ multipartForm: { ... } }` → multipart form data
   - `{ event: { id: string, data: ... } }` → server-sent events
3. **`req.query`** — Typed from `query` field. Use `optional(...)` for optional query params.
4. **`req.session`** — Typed from `auth.sessionSchema`. Contains JWT payload fields plus your custom schema. Only available when `auth` is configured.
5. **`res.status(N)`** — Returns a typed response object. `.json()` accepts the type defined at `responses[N]`, `.send()` accepts string/buffer when `responses[N]` is `string`.
6. **`res.setHeader(key, value)`** — Key is constrained to keys defined in `responseHeaders` plus framework headers (`x-correlation-id`). Omitting `responseHeaders` means only framework headers are allowed.
7. **`responses`** — Status codes map to response body types. The type system enforces that you call `res.status()` with a declared code and pass the matching body shape.

### Auth Variants

```typescript
// JWT with roles
access: 'protected',
auth: {
  sessionSchema: SHARED_SESSION_SCHEMA,
  jwt: { jwksPublicKeyUrl: JWKS_URL },
  allowedRoles: PLATFORM_VIEWER_ROLES
}

// JWT with permissions
access: 'protected',
auth: {
  sessionSchema: SHARED_SESSION_SCHEMA,
  jwt: { jwksPublicKeyUrl: JWKS_URL },
  allowedPermissions: new Set(['platform:read'])
}

// HMAC (internal service-to-service)
access: 'internal',
auth: {
  hmac: { secretKeys: { default: HMAC_SECRET_KEY } }
}

// Basic auth
access: 'authenticated',
auth: {
  basic: { login: (user, pass) => user === 'admin' && pass === 'secret' }
}
```

### Schema Primitives

Import from `@forklaunch-platform/core` (or `@forklaunch/validator`):

```typescript
import {
  string,
  number,
  boolean,
  date,
  optional,
  array,
  record,
  union,
  literal,
  enum_,
  type,
  unknown,
  any,
  file,
  binary,
  uuid,
  email,
  uri,
  null_,
  undefined_,
  never,
  void_,
} from "@forklaunch-platform/core";
```

These are validator-agnostic (work with both Zod and TypeBox). The simple ones (`string`, `number`, `boolean`, `date`) are bare values used directly in schema objects. The complex ones are functions:

#### `optional(schema)` — Makes a field optional

```typescript
// req.query.service is string | undefined
query: { environment: string, service: optional(string) }
```

#### `array(schema)` — Array of items

```typescript
// Typed array of objects
body: {
  tags: array(string);
} // string[]
body: {
  items: array({ key: string, value: string });
} // { key: string; value: string }[]
```

#### `record(keySchema, valueSchema)` — Dynamic key-value map

```typescript
// Record<string, unknown> — arbitrary metadata
body: {
  metadata: record(string, unknown);
}

// Record<string, number> — string keys, number values
body: {
  scores: record(string, number);
}
```

#### `literal(value)` — Exact constant value

```typescript
// Must be exactly the string "active"
body: {
  status: literal("active");
}

// Combine with union for string literal unions
body: {
  direction: union([literal("asc"), literal("desc")]);
}
```

#### `union(schemas[])` — One of several types

Takes an **array** of schemas. The value must match exactly one:

```typescript
// string | number
body: {
  id: union([string, number]);
}

// Discriminated union of literal strings — this is the most common pattern
body: {
  type: union([literal("select"), literal("list")]);
}
// → type: 'select' | 'list'

// Mixed types
body: {
  value: optional(union([string, number, boolean, unknown]));
}
```

#### `enum_(obj)` — Enum from a `const` object's values

Pass an `as const` object. The resulting type is a union of its **values** (not keys):

```typescript
// Define the enum object (typically in a domain/enum/ file)
const EnvironmentVariableScope = {
  APPLICATION: "application",
  SERVICE: "service",
  WORKER: "worker",
} as const;

// Use in schema — type becomes 'application' | 'service' | 'worker'
body: {
  scope: enum_(EnvironmentVariableScope);
}
```

This pattern is used throughout the codebase. The convention is to co-export a type alias:

```typescript
export const MyEnum = { A: "a", B: "b" } as const;
export type MyEnum = (typeof MyEnum)[keyof typeof MyEnum]; // 'a' | 'b'
```

#### `type(fn)` — Custom/complex type constructor

For advanced cases where you need to reference a complex schema type that doesn't fit the other primitives. Rarely needed in practice.

#### Combining primitives — real-world examples

```typescript
// Compliance feature config options
const FeatureConfigOptionsSchema = {
  type: union([literal("select"), literal("list")]),
  options: optional(array({ value: string, label: string })),
  listEntryTypes: optional(array(union([literal("cidr"), literal("dns")]))),
  defaultValue: optional(string),
};

// Environment variable with component metadata
const EnvironmentVariableSchema = {
  key: string,
  value: string,
  required: boolean,
  hasValue: boolean,
  isDeleted: optional(boolean),
  source: enum_(EnvironmentVariableScope),
  scopeId: optional(string),
  component: optional({
    type: enum_(EnvironmentVariableComponentType),
    property: enum_(EnvironmentVariableComponentProperty),
    target: optional(string),
    path: optional(string),
  }),
};

// Integration config with dynamic shape
const IntegrationConfigSchema = {
  type: enum_(IntegrationType),
  config: record(string, unknown),
};
```

## Common Patterns

### Creating a New API Feature

```bash
# 1. Add router to existing service
forklaunch init router user-profile --path ./src/modules/platform-management

# 2. Implement the RCSIDES stack:
# - Routes: Define HTTP endpoints
# - Controllers: Handle requests
# - Services: Business logic
# - Interfaces: Type definitions
# - Data: Data transfer objects
# - Entities: Database models
# - Seeders: Test data
```

### Adding a Background Worker

```bash
# 1. Create worker
forklaunch init worker email-worker --path ./src/modules --type bullmq

# 2. Add job processing routers
forklaunch init router send-email --path ./src/modules/email-worker
forklaunch init router process-bounce --path ./src/modules/email-worker

# 3. Implement job handlers in controllers
```

### Sharing Code Across Services

```bash
# 1. Create shared library
forklaunch add library shared-types

# 2. Export types/utilities from library
# 3. Import in services: import { Type } from '@modules/shared-types'
```

### Migrating from SQLite to PostgreSQL

```bash
# 1. Commit current state
git add . && git commit -m "Before database migration"

# 2. Change database
forklaunch change service my-service --database postgresql

# 3. Update environment variables
# 4. Run migrations
pnpm migration:up

# 5. Test thoroughly
pnpm test
```

## When Claude Code Should Use This Skill

1. **User wants to add a new service/worker/library**: Use `forklaunch add` commands
2. **User wants to modify project configuration**: Use `forklaunch change` commands with `--dry-run` first
3. **User mentions Forklaunch patterns**: Apply the best practices from this skill
4. **Creating new files in a Forklaunch project**: Follow the structure conventions
5. **User asks about manifest**: Reference manifest structure and CLI commands
6. **User needs to add infrastructure**: Guide them through adding resources

## Important Notes

- ALWAYS use `--dry-run` before applying changes to preview effects
- NEVER manually edit `.forklaunch/manifest.toml` - use CLI commands
- Follow the established project structure conventions
- Use dependency injection via registrations.ts
- Maintain consistent naming conventions
- Test after every change operation
- Commit before and after making structural changes

## Known Scaffold Bugs

**Client-SDK compliance namespace:** The scaffolded client-sdk compliance client uses `config.iam.compliance` (not `config.iam.core.compliance`). If you see a reference to `config.iam.core.compliance`, it is a scaffold bug -- fix to `config.iam.compliance`.

**Worker DI factory parsing:** ForkLaunch's config injector inspects factory argument syntax. Every factory first parameter must be object destructuring. Do not use `(container)` factories or nested destructuring IIFEs/helper arrows inside `WorkerConsumer`/`WorkerProducer`, such as `(({ QUEUE_NAME, WorkerOptions }) => ...)(container)`. Use one top-level factory parameter destructure, for example `({ QUEUE_NAME, WorkerOptions, EventEncryptor })`.

Still present in v1.3.3 — apply these workarounds:

- **`openapi export` / `release create` crashes or hangs for services with a DI-resolved `ExpressApplicationOptions`** (e.g. `iam-better-auth`, which needs it to merge in BetterAuth's async OpenAPI content) — root-caused in v1.3.3: the CLI sets `FORKLAUNCH_MODE=1` on the child process to mean "generate the spec and exit, don't boot for real," but the installed `@forklaunch/core`/`@forklaunch/express` packages only recognize `FORKLAUNCH_MODE === "openapi"` for that shortcut (both the DI singleton-stubbing in `resolveInstance` and the write-spec-and-exit branch in `Application.listen()`). Because of the mismatch the shortcut never engages, so the service boots as a real live server instead — including starting a real MCP tool-server that hangs indefinitely, and resolving a real Orm/BetterAuth stack that needs a live DB and secrets. App-side fix, add to the top of `server.ts` before any `ci.resolve(...)` call:
  ```typescript
  if (process.env.FORKLAUNCH_MODE === '1') {
    process.env.FORKLAUNCH_MODE = 'openapi';
  }
  ```
  Additionally, resolving `ExpressApplicationOptions` through DI in genuine `openapi` mode hits a *second* bug — `resolveInstance` stubs it with a no-op Proxy whose `Symbol.toPrimitive` coerces to `""`, and `contentParse` passes that straight into `express.json()`, so body-parser's strict `limit` validation throws `option limit "" is invalid` at Application-construction time (before `.listen()` is ever reached). Skip DI for that one token in openapi mode instead of resolving it:
  ```typescript
  const app = forklaunchExpress(
    schemaValidator,
    openTelemetryCollector,
    process.env.FORKLAUNCH_MODE === 'openapi'
      ? {}
      : await ci.resolve(tokens.ExpressApplicationOptions)
  );
  ```
- **`init` requires a platform login in v1.2.6** — even for purely local scaffolding, `init application` fails with "No token found" when logged out. Run `forklaunch login` first (CI needs a token).
- **iam `.env.local` template omits `JWKS_PUBLIC_KEY_URL`** — the iam service refuses to boot until you add it (e.g. `JWKS_PUBLIC_KEY_URL=http://localhost:<iam-port>/api/auth/jwks`).
- **Generated seeders are not idempotent** — re-running `pnpm seed` on an already-seeded DB fails with unique-constraint violations; seed only fresh databases (or clear first).
- **`compliance audit -e` platform upload has broken auth** — it hangs on a browser device flow or reports "No token found" even while `release`/`config` work with the same session. Local audit (no `-e`) is unaffected.
- **One platform app links ONE local app** — `integrate` refuses a second local app ("already integrated with another local app"); there is no CLI unlink and no CLI command to create a platform app (dashboard-only).
- **`deploy create` can drop into a real interactive prompt that no flag or piped input can skip.** If required env vars (e.g. `ENCRYPTION_KEY`) aren't already stored for the target environment, it writes a template file to a deterministic path (`/var/folders/.../forklaunch-env-<env>-<region>.env` on macOS) and blocks waiting for an actual keypress on a real TTY — piping `\n` via stdin fails with `Error: IO error: not a terminal` because it checks for a genuine terminal, not just readable input. The file is regenerated from scratch on every invocation (edits made before starting a new run are wiped), so there is no way to pre-fill it. If running non-interactively, ask the user to run the command themselves in a real terminal, or use the dashboard's environment-variables "Save & Deploy" flow instead. Note: `FORKLAUNCH_MODE` can show up in this template as a variable that "needs configuration" — this is a false positive from the variable scanner picking up `process.env.FORKLAUNCH_MODE` references in generated code; leave it as `NONE`/unset, since a real deployed service must never have it set (setting it to `openapi` makes the live server write a spec file and exit instead of serving traffic).

- **"User Supplied" env vars permanently override "Platform Injected" ones — never hand-supply a placeholder for an infra-derived var, on the CLI or the dashboard.** Both the CLI's env-var template file (from the interactive prompt above) and the dashboard's Manage → Environment Variables panel write to the same underlying config store, and that store treats anything you set as authoritative forever — it is never overwritten by the real Pulumi-provisioned value, even after the resource exists and reports healthy. This bites hardest on vars that only make sense once real infra exists: `DB_HOST`, `DB_PORT`, `DB_USER`, `DB_PASSWORD`, `DB_SSL`, `PGSSLMODE`, `REDIS_URL`, `CORS_ORIGINS`, `NODE_ENV`, `JWKS_PUBLIC_KEY_URL`, `OTEL_EXPORTER_OTLP_ENDPOINT`, `BETTER_AUTH_URL`. Typing a placeholder for one of these (e.g. `DB_HOST=placeholder`) unblocks whichever gate is asking for it, but Pulumi will go on to provision the real RDS/ElastiCache/ALB endpoints successfully (confirm under the app's Resources page, status "Running") while the ECS task keeps launching with the literal placeholder string — the service crash-loops forever (0/1 replicas, "Degraded") with zero logs ever reaching the OTel/Loki pipeline, since it dies before the app can log anything. (Exception: on the shared data plane the partition's generated `DB_*`/`REDIS_*` connection-var set is authoritative over user-supplied values at deploy time, so stale dedicated-era placeholders for those keys no longer reach the running task even if the store still lists them as "User Supplied".) Verified against a healthy sibling app: once fully deployed, all such vars should show as "Platform Injected" with real masked values and zero "User Supplied" entries for them — that's the target state, regardless of which tool you deployed with. Fix: in Manage → Environment Variables, delete the placeholder entries for the infra-derived keys above (they move to an "Unset" bucket, which is expected and fine pre-deploy) and redeploy; a second deploy against existing infra only updates the ECS task definition and finishes in a couple of minutes rather than the ~10 the first one takes. Keep as genuine "User Supplied" only the vars with no platform-managed equivalent: app secrets (`ENCRYPTION_KEY`, `BETTER_AUTH_SECRET`), anything with no default in code, and `FORKLAUNCH_MODE=NONE` (see the false-positive note above — never anything else). Optional integrations with a safe code fallback (e.g. `GOOGLE_CLIENT_ID`/`GOOGLE_CLIENT_SECRET` behind `?? ''`) still need *some* non-empty value to pass a "missing vars" gate even though the app treats them as disabled when unset — an obvious placeholder string is fine there since the app never uses it unless the feature is actually enabled.

- **Generated `mikro-orm.config.ts` derives `ssl` from `NODE_ENV` instead of the platform's injected `DB_SSL`/`PGSSLMODE`, breaking real deploys made with `--node-env development`** (verified on a real deploy, CLI v1.3.3). The scaffold ships `driverOptions: { ssl: NODE_ENV !== 'development' }` — so `ssl` is only `false` when `NODE_ENV` is literally `'development'`, `true` for anything else including `production`. That's fine if you deploy with `--node-env production`, but the CLI's own `deploy create` example (`--node-env development`) sets `NODE_ENV=development` on a real ECS task talking to a real RDS instance that requires SSL, and the connection fails: `DriverException: no pg_hba.conf entry for host "...", user "...", no encryption`. This surfaces specifically on better-auth's JWT plugin (`/api/auth/jwks`) because MikroORM only opens its connection (and runs the one-time `ensureDatabase` check) lazily on the first real query — in this scaffold that's whichever request touches the DB first, not app boot, so `/health` and the boot logs look clean while auth endpoints 500. Fix in `src/modules/<iam|billing>/mikro-orm.config.ts`, keyed off the platform's real value instead of the environment name:
  ```typescript
  driverOptions: {
    ssl: process.env.DB_SSL === 'true'
      ? { rejectUnauthorized: process.env.PGSSLMODE !== 'no-verify' }
      : validConfigInjector.resolve(tokens.NODE_ENV) !== 'development'
  }
  ```
  A redeploy after this fix (same infra, new release) resolves in ~3 minutes vs. the ~10 for the first deploy.
- **`forklaunch environment validate` misses `IAM_URL` as a required var for billing** — the scanner's Application/Service-Level Variables report never lists `IAM_URL`, yet billing throws `Validation failed: Path: IAM_URL Message: Required` at boot without it (confirmed both locally and on a real deploy, where the platform injects it correctly once `integrate`/`release` have run — it's just invisible to `environment validate`'s static scan). Don't treat a clean `environment validate` as proof the service will actually boot; also check the module's `registrations.ts` `environmentConfig` chain, or just boot it once and read the crash.
- **When a deployed service shows 0 replicas/"Degraded" with no logs, check the dashboard's CloudWatch log source before assuming you need AWS console access.** Dashboard → Monitoring → Logs defaults to a "Live (OTel)" source, which is silent for any crash that happens before the app's own OpenTelemetry collector initializes (e.g. a config-validation failure on boot) — that silence looks identical to "still starting." Switch the log source dropdown from "Live (OTel)" to "CloudWatch" instead: it shows the container's raw stdout/stderr, including the actual stack trace (e.g. `_ConfigInjector.validateConfigSingletons` failures, `Expected number, received nan` on a bad `DB_PORT`, `ELIFECYCLE ... exit code 1`) with no AWS console access required.

- **Empty secrets in generated `.env.local` / `.env.test`** (`ENCRYPTION_KEY=`, `HMAC_SECRET_KEY=`, `BETTER_AUTH_SECRET=`): `pnpm database:setup` fails with `MissingEncryptionKeyError` and tests fail env validation until filled. Generate values before first run: `openssl rand -base64 32` for ENCRYPTION_KEY, `openssl rand -hex 32` for the others. (`forklaunch environment sync` adds missing keys but only with blank values.)
- **iam seeder FK violation** (`account_user_id_foreign`): the generated `persistence/seeders/` omits `user.seeder.ts` even though `seed.data.ts` defines the user, and `DatabaseSeeder` runs seeders via `Object.values(namespace)` which enumerates ALPHABETICALLY (Account before User). Fix: add a `UserSeeder` (mirror `account.seeder.ts`) and replace `Object.values(seeders)` in `persistence/seeder.ts` with an explicit FK-ordered array `[UserSeeder, AccountSeeder, SessionSeeder, VerificationSeeder]`.
- **`pnpm lint` cleanliness depends on module mix** — iam+billing-only apps lint clean, but generated worker/service templates (kafka workers etc.) still ship unused-var errors.
- **`pnpm test` cannot run green out of the box** for DB-backed modules. Three fixable layers: fill `.env.test` (as above); change `ssl: NODE_ENV !== 'development'` to `=== 'production'` in each `mikro-orm.config.ts`; fix `migrationsPath` in `__test__/test-utils.ts` from `'../migrations'` to `'../migrations-postgresql'`. A fourth layer (the harness cannot import `.ts` migration files at runtime) has no app-side fix — treat DB-harness suites as broken and verify endpoints via live HTTP instead. Pure unit tests (no DB) work fine.
- **`migrate:down` never works on the initial migration**: generated migrations ship without a `down()` implementation ("This migration cannot be reverted").
- **Dockerfile pnpm drift**: `RUN npm install -g pnpm` (unpinned) pulls pnpm 11, whose default `minimumReleaseAge` policy rejects lockfile entries published recently. This breaks BOTH local `docker compose build` AND real platform deploys — the same error (`ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION`) shows up in AWS CodeBuild logs and fails `forklaunch deploy create` after all the infrastructure has already been provisioned. Fix: pin the Dockerfile to the host's version, e.g. `RUN npm install -g pnpm@10.12.1`, before releasing/deploying.
- **`change worker --type` is destructive — avoid it**: it rewrites code but does NOT update the manifest (variant/resources keep the old type), and it silently DELETES the `minio:` service from docker-compose while `minio-init`/`tempo` still depend on it, leaving an invalid compose project that `sync all` cannot heal. Prefer deleting and re-creating the worker with the new type; if you must convert, restore the minio block and fix the manifest by hand afterwards.
- **`change service --infrastructure redis` also drops the `minio:` compose service** — same failure mode as `change worker --type` above, just triggered by a different command. After running it, check `docker compose config --quiet` before `docker compose up`; if it errors with `service "minio-init" depends on undefined service "minio"`, the `minio:` block needs to be restored by hand (copy it from another scaffolded app's `docker-compose.yaml`, or reconstruct: `image: minio/minio:RELEASE.2025-04-22T22-12-26Z`, ports `9000:9000`/`9001:9001`, env `MINIO_ROOT_USER`/`MINIO_ROOT_PASSWORD` both `minioadmin`, healthcheck `mc ready local`).
- **`change service --infrastructure redis` wires a `TtlCache` factory that doesn't compile** — it adds a `TtlCache` registration to `registrations.ts` whose factory destructures `{ REDIS_URL, OtelCollector, ENCRYPTION_KEY }` and calls `new FieldEncryptor(ENCRYPTION_KEY)`, but neither `ENCRYPTION_KEY` (in the `environmentConfig` chain) nor the `FieldEncryptor` import (`from '@forklaunch/core/persistence'`) get added. Fails with "Unable to resolve dependency ENCRYPTION_KEY" at runtime. Add both manually after running the command.
- **Service/worker conversions (`--to worker` / `--to service`) strand compose entries** of the old type; grep docker-compose for the module name after converting (and after deleting a converted module).
- **`eject` is unusable** (panics or misparses every path form; error path exits 0). Eject manually if needed.
- **Generated kafka-init script** uses `$DESC` unescaped, so every compose command prints `The "DESC" variable is not set` warnings (cosmetic; should be `$$DESC`).
- **`forklaunch init service` doesn't wire `ENCRYPTION_KEY` for a new service locally** — even though it correctly auto-detects sibling `iam`/`billing` modules and wires `IAM_URL`/`BILLING_URL`/`HMAC_SECRET_KEY` (shared, app-level values) into the new service's `registrations.ts` and `docker-compose.yaml`. `ENCRYPTION_KEY` is left out of the new service's `docker-compose.yaml` block entirely (fails with `MissingEncryptionKeyError` on `pnpm migrate:create`/boot) and out of its `.env.local`, so generate one for local dev (`openssl rand -base64 32`) and add it to `docker-compose.yaml`. On a real deploy this is no longer a blocker: deploy validation auto-generates a value for `ENCRYPTION_KEY` (and the other app-internal secret-class keys `BETTER_AUTH_SECRET`, `PASSWORD_ENCRYPTION_SECRET`, `HMAC_SECRET_KEY`, `CONFIG_ENCRYPTION_KEY`) when the required key is missing or hidden by an unset tombstone, and persists it through the component env-config path so it's stable across deploys. It does not need to match other services' keys — it's only used to encrypt that service's own compliance-tagged fields at rest.
- **Billing's generated `src/modules/billing/surfacing.ts` has two bugs that break `requireActiveSubscription`/`requiredFeatures` out of the box when scaffolded alongside `iam-better-auth`** (verified on a real deploy, CLI v1.3.3, adding a third service to an app with existing iam+billing modules): (1) `createSurfaceSubscription`/`createSurfaceFeatures` read the JWT payload's org claim as `payload.organizationId`, but IAM's own generated `src/modules/iam/auth.ts` (`definePayload`) actually issues it as `activeOrganizationId` — the claim is always `undefined`, so the surfacing function throws `Error: organizationId is required in JWT payload`, an uncaught 500 on every request gated by `requireActiveSubscription`. (2) The same two functions sign the wrong HMAC path for their internal calls — they sign `` `/${organizationId}/subscription` `` and `` `/${productId}/plan` ``, but the actual routes (registered on `subscriptionRouter`/`planRouter`) are `/organization/:id` and `/:id`, so the correct route-relative paths are `` `/organization/${organizationId}` `` and `` `/${productId}` ``. A wrong path fails HMAC verification on the receiving service with `Invalid Authorization signature.` even though the request itself is legitimate. Fix both in `surfacing.ts`: accept `payload.activeOrganizationId ?? payload.organizationId`, and correct the two signed paths.
- **`@forklaunch/implementation-billing-base@1.0.26`'s `getOrganizationSubscription`/`getUserSubscription` hardcode the wrong case for `partyType` in their DB query** — they filter with the literal strings `"ORGANIZATION"`/`"USER"` (the `PartyEnum` object *keys*), but the `subscription.party_type` column only ever stores the enum *values* (`'organization'`/`'user'`, per the DB check constraint and everywhere else `PartyEnum` is used to persist a row). Any real subscription lookup by party therefore always returns `NotFoundError: Subscription not found`, even for a row that genuinely exists and matches — confirmed by inserting a subscription directly and still getting a 404. This is a bug in the vendored package itself, not generated app code. Fix with `pnpm patch @forklaunch/implementation-billing-base@1.0.26`, lowercasing both `partyType: "USER"` → `"user"` and `partyType: "ORGANIZATION"` → `"organization"` in both `lib/services/index.js` and `index.mjs`.
- **`billing-stripe`'s `getOrganizationSubscription`/`getUserSubscription` always make a live call to the real Stripe API** (`stripeClient.subscriptions.retrieve(...)`) to enrich `stripeFields`, even on a plain read of already-persisted local data — so `requireActiveSubscription`/`requiredFeatures` (and the `surfacing.ts` helpers above) cannot work in ANY environment without a real, valid `STRIPE_API_KEY`, including local dev with a placeholder key (`Error: Invalid API Key provided: replace-***-key`). This contradicts the surrounding cache-first design (`BillingCacheService`'s own doc comment says subscription data should come from "billing cache (populated by webhook events) or via SDK") — there is currently no way to read subscription status without hitting Stripe live, and no workaround short of patching this method to skip the live retrieve.
- **`iam-better-auth`'s `organization()` plugin config ships `dynamicAccessControl: { enabled: true }` with zero seeded permissions**, which makes a freshly-created organization unusable for any role other than its creator's default `owner` role via the API alone: the sole owner can't invite another member (`POST /api/auth/organization/invite-member` → 403 `YOU_ARE_NOT_ALLOWED_TO_INVITE_USERS_TO_THIS_ORGANIZATION`, since dynamicAccessControl checks a per-role `organization_role` permission table that's empty for a fresh org), and can't self-demote (`update-member-role` → 400 `YOU_CANNOT_LEAVE_THE_ORGANIZATION_WITHOUT_AN_OWNER`, since they're the only member). There is no exposed endpoint to seed default org-role permissions. If you need a non-owner-role test user, the only way is to write the `member` row's `role` column directly in the iam database — only possible with DB access (e.g. locally via `docker exec ... psql`), not available against a real deployed RDS instance.

## Related Documentation

For more information, refer to:

- ForkLaunch CLI Reference: `/docs/cli.md`
- Adding Projects Guide: `/docs/adding-projects.md`
- Changing Projects Guide: `/docs/changing-projects.md`
- Framework Reference: `/docs/framework.md`
