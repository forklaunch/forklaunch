# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# ForkLaunch

A TypeScript-first backend framework for building typed, modular Node.js services. The monorepo is organized into three main components:

- **framework** - Core runtime libraries: typed HTTP routing (Express/Hyper-Express), schema validation (Zod/TypeBox), DI, OpenTelemetry, universal SDK generation, and infrastructure adapters (Redis, S3).
- **cli** - Command-line tool (`forklaunch`) for scaffolding and managing apps, services, workers, and libraries. **Written in Rust** — compiled binary, Node.js package.json provides a postinstall wrapper.
- **blueprint** - Pre-built, production-ready service templates for common concerns (IAM, billing, workers) that can be generated and customized via the CLI.

---

## Commands

```bash
# Build all packages
pnpm -r run build

# Lint
pnpm lint
pnpm lint:fix

# Format
pnpm format

# Test (Vitest)
pnpm test

# Run a single test file
pnpm vitest <path/to/file.test.ts>

# Watch mode
pnpm vitest --watch

# CLI (Rust)
cargo build             # from cli/
cargo test              # run CLI tests
```

---

## Framework Architecture

### Package Layout (`framework/`)

| Package | Import Path | Purpose |
|---|---|---|
| `common` | `@forklaunch/common` | Utility functions, type guards (`isRecord`, `camelCase`, `hashString`, etc.) |
| `core` | `@forklaunch/core/*` | HTTP routing, DI (`ConfigInjector`), caching, mappers, persistence, object store |
| `express` | `@forklaunch/express` | Express adapter with typed routing |
| `hyper-express` | `@forklaunch/hyper-express` | High-performance Express-like adapter |
| `validator` | `@forklaunch/validator/zod` or `/typebox` | Schema validation wrappers |
| `universal-sdk` | `@forklaunch/universal-sdk` | Auto-generate typed client SDKs from route definitions |
| `infrastructure/redis` | `@forklaunch/infrastructure-redis` | `RedisTtlCache` — TTL-based caching |
| `infrastructure/S3` | `@forklaunch/infrastructure-s3` | `S3ObjectStore` — binary/large file storage |
| `ws` | `@forklaunch/ws` | WebSocket server/client with schema validation |
| `testing` | `@forklaunch/testing` | `TestContainerManager` for integration tests (Postgres, Redis, S3, Kafka, etc.) |
| `bunrun` | `@forklaunch/bunrun` | Bun runtime runner |

### Dependency Injection (`core/src/services/configInjector.ts`)

`createConfigInjector` provides a type-safe DI container with three lifetimes:

```typescript
import { createConfigInjector, Lifetime } from '@forklaunch/core/services';

const injector = createConfigInjector(schemaValidator, {
  myService: {
    lifetime: Lifetime.Singleton,
    type: MyService,
    factory: ({ dep }, resolve) => new MyService(resolve('dep'))
  }
});
```

- `Singleton` — shared instance
- `Transient` — new instance each resolution
- `Scoped` — new instance per scope

### Typed HTTP Routing

Routes are defined contract-first with schemas for body, params, query, and responses. This powers automatic OpenAPI docs, SDK generation, and OpenTelemetry tracing:

```typescript
import { forklaunchExpress } from '@forklaunch/express';
import { SchemaValidator, string, uuid } from '@forklaunch/validator/zod';

const app = forklaunchExpress(SchemaValidator(), openTelemetryCollector);

app.post('/deployments', {
  name: 'Create Deployment',
  summary: 'Creates a new deployment',
  body: { applicationId: uuid, environmentId: uuid },
  responses: {
    201: { id: uuid, status: string },
    400: { error: string }
  }
}, async (req, res) => {
  res.status(201).json(await service.create(req.body));
});
```

---

## Module Structure Convention

```
src/modules/<module-name>/
├── api/
│   ├── controllers/    # HTTP handlers — use mappers, return responses
│   ├── routes/
│   ├── middleware/
│   └── utils/
├── domain/
│   ├── services/       # Business logic — return entities, NO mappers
│   ├── schemas/
│   ├── types/
│   ├── mappers/        # Entity ↔ DTO transformations (controllers only)
│   ├── enum/
│   └── utils/
├── persistence/
│   ├── entities/
│   └── seeders/
├── migrations-postgresql/
├── websocket/
├── registrations.ts    # DI wiring
└── server.ts
```

**File naming:** `<resource>.<layer>.ts` — e.g. `deployment.controller.ts`, `deployment.service.ts`, `deployment.entity.ts`, `deployment.mappers.ts`.

---

## Key Rules

- **Never use `any` as a type.** Use `unknown`, a specific type, or a generic parameter instead.
- **Always import from `@forklaunch` packages**, never from `@modules/core`.
- **Mappers belong in controllers only.** Services return entities; controllers map to DTOs.
- **Never manually edit `.forklaunch/manifest.toml`** — always use CLI commands.
- **Always `--dry-run` before `forklaunch change`.**

---

## Import Order (7 Layers)

```typescript
// 1. Node built-ins
import crypto from 'node:crypto';
// 2. External dependencies
import { EntityManager } from '@mikro-orm/core';
// 3. Forklaunch framework packages
import { isRecord } from '@forklaunch/common';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
// 4. Cross-module imports
import { generateHmacAuthHeaders } from '@forklaunch-platform/iam';
// 5. Local persistence
import { Deployment } from '../../persistence/entities';
// 6. Local domain
import { DeploymentService } from '../services/deployment.service';
// 7. Same directory
import { EncryptionService } from './encryption.service';
```

---

## CLI Reference

```bash
# Scaffold
forklaunch init service <name> --database postgresql
forklaunch init worker <name> --type bullmq
forklaunch init library <name>

# Modify (always dry-run first)
forklaunch change application --runtime bun --dry-run
forklaunch change application --runtime bun

# Sync artifacts after manifest changes
forklaunch sync all
forklaunch sync service <name>

# Validate environment variables
forklaunch environment validate

# Export OpenAPI specs
forklaunch openapi export --output ./docs/api
```

---

## Cache vs Object Store

| Use case | Tool |
|---|---|
| Sessions, API responses, rate limiting (small, short-lived) | `RedisTtlCache` from `@forklaunch/infrastructure-redis` |
| Files, documents, large binary data | `S3ObjectStore` from `@forklaunch/infrastructure-s3` |

Always use `createCacheKey` / `createObjectStoreKey` for consistent key naming.

---

## Testing

- Unit tests: mock dependencies, use `vitest`
- Integration tests: use `TestContainerManager` from `@forklaunch/testing` for real Postgres/Redis/S3/Kafka containers
- Always `afterAll(() => manager.stopAll())` to clean up containers
- Auth testing: use `TEST_TOKENS` from `@forklaunch/testing`
- E2E examples live in `framework/e2e-tests/servers/`
