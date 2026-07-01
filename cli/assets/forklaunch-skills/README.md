# ForkLaunch Skills for Claude Code

Skills that teach Claude Code (and developers) how to build with ForkLaunch.

## Available Skills

### Building & Scaffolding
- `/studio` — Fast app generation: greenfield, existing Next.js, backend migration.
- `/cli` — All CLI commands: init, change, delete, deploy, release, sync, sdk, openapi. **Supply ALL flags or CLI hangs.**
- `/quick-reference` — Cheat sheet: imports, patterns, templates, commands at a glance.

### Backend
- `/backend-patterns` — Handlers, services, entities, schemas, routes, DI, auth, feature gating.
- `/common-tasks` — Step-by-step: add endpoints, create entities, add pages, feature gating, debugging, migrations.
- `/framework` — HTTP handler definitions, route config, validation, auth, OpenAPI, MCP, SDK gen, streaming.
- `/imports-and-structure` — Import layers, module structure, file naming. **Always import from `@{{app-name}}/core`.**
- `/websockets-and-mappers` — WebSockets, real-time, log streaming, requestMapper/responseMapper.
- `/compliance` — fp property builder, defineComplianceEntity, access levels, audit CLI, encryption, tenant isolation.

### Frontend
- `/frontend-patterns` — Pages, SDK client, useApi/useMutation hooks, auth, feature gating, forms, tables.
- `/tanstack` — TanStack Start: routing, server functions, SSR, data loading.
- `/design-system` — Design philosophy router: Stripe, Linear, Robinhood, Fidelity, Clinical, Airbnb, Retool, Notion.

### Infrastructure
- `/infrastructure-and-utilities` — Redis cache, S3 object store, TestContainers, utilities.
- `/platform-architecture` — Modules, DDD, deployment workflow, Pulumi, multi-tenancy, worker queues.
- `/development-guidelines` — Toolchain: runtimes (node/bun), validators (zod/typebox), databases, formatters, linters, tests, workers.

### Planning & Review
- `/plan` — 4-phase plan pipeline: CEO review, eng review, diagrams, plan doc.
- `/plan-ceo-review` — CEO/founder scope challenge and premise audit.
- `/plan-eng-review` — Engineering architecture, code quality, tests, performance review.

## Critical Rules

1. **Import from `@{{app-name}}/core`** for schema primitives, handlers, router, validator. NEVER from `@forklaunch/validator/*` or `@forklaunch/express`.
2. **Schemas use natural notation:** `{ name: string, age: optional(number) }`. NEVER `z.object()`.
3. **Enums use const objects:** `const X = { A: 'a' } as const`. NEVER TypeScript `enum`.
4. **Handler `name` cannot contain slashes.** Use PascalCase: `'GetRestaurant'`.
5. **Services return entities.** Mapping happens in controllers only.
6. **Always `em.flush()` after mutations.**
7. **Use `forklaunch init` for structural changes.** Don't manually create service directories.
8. **Don't edit `manifest.toml` by hand.** Use `forklaunch change` commands.
9. **Supply ALL CLI flags** or the CLI drops into interactive mode and hangs.
10. **TSC for backend:** `cd src/modules/<service> && ./node_modules/.bin/tsc --noEmit`

## Quick Start for New Features

```
1. Schema:     src/modules/<svc>/domain/schemas/<name>.schema.ts
2. Entity:     src/modules/<svc>/persistence/entities/<name>.entity.ts
3. Service:    src/modules/<svc>/domain/services/<name>.service.ts
4. Controller: src/modules/<svc>/api/controllers/<name>.controller.ts
5. Routes:     src/modules/<svc>/api/routes/<name>.routes.ts
6. Wire:       registrations.ts + bootstrapper.ts
7. Export:     api/controllers/index.ts
8. Migrate:    cd src/modules/<svc> && pnpm migrate:create && pnpm migrate:up
```

## Running

```bash
# Prerequisites
docker compose up -d                    # Start postgres, redis, minio, etc.
pnpm install                            # Install all deps

# Backend (per-module)
cd src/modules/<service> && pnpm dev    # Start a single service in dev mode (tsx watch)

# Frontend
cd client && pnpm dev                   # Next.js dev server (default: localhost:3000)

# All services at once (from repo root)
pnpm dev                                # Starts all modules + client concurrently

# Migrations
cd src/modules/<service> && pnpm migrate:create   # Create new migration
cd src/modules/<service> && pnpm migrate:up        # Run pending migrations

# Type checking
cd src/modules/<service> && ./node_modules/.bin/tsc --noEmit   # Backend
cd client && pnpm tsgo --noEmit                                 # Frontend

# Tests
cd src/modules/<service> && pnpm test   # Run module tests
```

## Module Structure

```
src/modules/<module>/
├── api/
│   ├── controllers/          # handlers.get/post/put/patch/delete
│   │   └── index.ts          # re-exports all (for SDK generation)
│   ├── routes/               # forklaunchRouter definitions
│   └── middleware/
├── domain/
│   ├── services/             # business logic (NO mappers)
│   ├── schemas/              # natural object notation
│   ├── types/
│   ├── mappers/              # requestMapper/responseMapper
│   ├── enum/                 # const-as-const enums
│   └── utils/
├── persistence/
│   ├── entities/             # MikroORM @Entity (SqlBaseEntity)
│   │   └── index.ts
│   └── seeders/
├── migrations-postgresql/
├── registrations.ts          # createConfigInjector + chain
├── bootstrapper.ts           # env loading, DI container
├── server.ts                 # forklaunchExpress, routes, listen
└── package.json              # pnpm scripts for migrate, dev, test
```
