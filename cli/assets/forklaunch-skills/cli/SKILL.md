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

**Package manager follows the runtime:** use `pnpm` for `--runtime node`, use `bun` for `--runtime bun`. This applies to all commands (`install`, `dev`, `test`, `migrate:*`, etc.).

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
pnpm install && pnpm dev

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
bun install && bun dev

# CORRECT — init a service with all flags
forklaunch init service billing --path ./src/modules --database postgresql --description "Billing service"

# WRONG — missing required flags, will hang in interactive mode
forklaunch init application my-app  # missing --database, --runtime, --modules, --formatter, --linter, --test-framework, etc.
```

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

Adds a preconfigured module (billing or IAM) to an existing application:

```bash
forklaunch init module <name> --path <app-path> --module <module-type> --database <db>

# Module types:
# billing-base    — Billing app hooks only (no payment provider)
# billing-stripe  — Stripe billing implementation
# iam-base        — IAM authorization only (no auth provider)
# iam-better-auth — Better Auth implementation for IAM

# Example:
forklaunch init module billing --path ./src/modules --module billing-stripe --database postgresql
forklaunch init module iam --path ./src/modules --module iam-better-auth --database postgresql
```

### `forklaunch init router`

Adds a new controller + route + schema + service to an existing service or worker module.

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
--modules <module>       # billing-base, billing-stripe, iam-base, iam-better-auth (can repeat -m)

# Example:
forklaunch init app my-platform --runtime bun --http-framework express
```

#### Initialize Service

```bash
forklaunch init service <service_name>

# Options:
--path <app-path>       # Application path to scaffold in
--description "Service description"
--database postgresql|mysql|mariadb|mssql|mongodb|libsql|sqlite|better-sqlite
--infrastructure redis|s3  # Can specify multiple: -i redis,s3
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
--description "Library description"

# Example:
forklaunch init library shared-utils
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

Remove project components safely.

```bash
# Delete service
forklaunch delete service <service_name>

# Delete worker
forklaunch delete worker <worker_name>

# Delete library
forklaunch delete library <library_name>

# Delete router (use --path to specify the service directory)
forklaunch delete router <router_name> --path <service_directory>

# Examples:
forklaunch delete service old-billing
forklaunch delete worker deprecated-processor
forklaunch delete router legacy-api --path ./src/modules/platform-management
```

### 4. Deploy Commands (`deploy`)

Deploy applications to the cloud.

```bash
forklaunch deploy

# Options:
--environment <name>    # Environment to deploy to (dev, staging, prod)
--region <region>       # AWS region
--dry-run              # Preview deployment plan
--auto-approve         # Skip confirmation prompts

# Examples:
forklaunch deploy --environment staging --region us-east-1
forklaunch deploy --environment production --dry-run
```

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

```bash
# Create release from current state
forklaunch release create

# Options:
--version <version>    # Release version (semantic versioning)
--message "Release message"
--git-ref <ref>       # Git commit/branch/tag

# List releases
forklaunch release list

# Show release details
forklaunch release show <version>

# Rollback to previous release
forklaunch release rollback <version>

# Examples:
forklaunch release create --version 1.2.3 --message "Add user authentication"
forklaunch release list
forklaunch release show 1.2.3
forklaunch release rollback 1.2.2
```

### 7. Integrate Commands (`integrate`)

Integrate with external services and tools.

```bash
forklaunch integrate <service>

# Supported integrations:
# - github        # GitHub repository integration
# - stripe        # Stripe billing
# - aws           # AWS services
# - datadog       # Datadog monitoring
# - sentry        # Sentry error tracking

# Options vary by service

# Examples:
forklaunch integrate github --repo owner/repo-name
forklaunch integrate stripe --api-key sk_test_...
forklaunch integrate aws --access-key-id ... --secret-access-key ...
```

### 8. OpenAPI Commands (`openapi`)

Generate OpenAPI specifications from your code.

```bash
# Generate OpenAPI specs for all services
forklaunch openapi generate

# Generate for specific service
forklaunch openapi generate --service <service_name>

# Validate OpenAPI specs
forklaunch openapi validate

# Options:
--output <path>        # Output directory (default: .forklaunch/openapi)
--version <version>    # OpenAPI version (3.0, 3.1)

# Examples:
forklaunch openapi generate
forklaunch openapi generate --service platform-management
forklaunch openapi validate
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

## Related Documentation

For more information, refer to:

- ForkLaunch CLI Reference: `/docs/cli.md`
- Adding Projects Guide: `/docs/adding-projects.md`
- Changing Projects Guide: `/docs/changing-projects.md`
- Framework Reference: `/docs/framework.md`
