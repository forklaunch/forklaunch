---
name: imports-and-structure
description: "Imports: @core exports, import layers, module structure, file naming."
user-invokable: true
---

# ForkLaunch Imports & Project Structure

## When to Use This Skill

Use when writing any code in the ForkLaunch platform — imports and structure rules apply to EVERY file.

## The Central Import: @{{app-name}}/core

`@{{app-name}}/core` (`src/modules/core`) re-exports everything you need from the framework. This is the **primary import source** for all modules.

### What it exports

```typescript
// Schema primitives (natural object notation — NOT z.object/Type.Object)
import {
  string,
  number,
  boolean,
  optional,
  array,
  enum_,
  date,
  record,
  type,
  uuid,
  union,
  literal,
  email,
  uri,
  unknown,
  any,
  file,
  binary,
  null_,
  nullish,
  void_,
  never,
  bigint,
  symbol,
  function_,
  promise,
  undefined_,
} from "@{{app-name}}/core";

// Express framework
import {
  forklaunchExpress,
  forklaunchRouter,
  handlers,
  schemaValidator,
  SchemaValidator,
} from "@{{app-name}}/core";

// Auth
import {
  SHARED_SESSION_SCHEMA,
  generateHmacAuthHeaders,
} from "@{{app-name}}/core";

// Shared schemas
import { IdSchema, IdsSchema } from "@{{app-name}}/core";

// Types
import type {
  Request,
  Response,
  NextFunction,
  ExpressApplicationOptions,
} from "@{{app-name}}/core";

// Entity base class
import { SqlBaseEntity } from "@{{app-name}}/core";

// Feature flags and billing
import { FEATURE_FLAGS, PLAN_LIMITS } from "@{{app-name}}/core";
import type { BillingCacheService } from "@{{app-name}}/core";

// RBAC
import {
  PLATFORM_EDITOR_ROLES,
  PLATFORM_VIEWER_ROLES,
  PLATFORM_ADMIN_ROLES,
} from "@{{app-name}}/core";
```

### What is NOT in core (import directly)

```typescript
// Common utilities
import {
  isRecord,
  camelCase,
  hashString,
  safeStringify,
} from "@forklaunch/common";

// Core services (DI)
import { Lifetime, createConfigInjector } from "@forklaunch/core/services";

// Core HTTP (OpenTelemetry)
import { OpenTelemetryCollector } from "@forklaunch/core/http";

// Core mappers
import { requestMapper, responseMapper } from "@forklaunch/core/mappers";

// Core cache
import { createCacheKey, TtlCacheRecord } from "@forklaunch/core/cache";

// Core persistence
import { BaseEntity } from "@forklaunch/core/persistence";

// Infrastructure
import { RedisTtlCache } from "@forklaunch/infrastructure-redis";
import { S3ObjectStore } from "@forklaunch/infrastructure-s3";

// WebSocket
import { ForklaunchWebSocket, ForklaunchWebSocketServer } from "@forklaunch/ws";

// Testing
import { BlueprintTestHarness, TEST_TOKENS } from "@forklaunch/testing";
```

### Cross-module imports

```typescript
import { generateHmacAuthHeaders } from "@{{app-name}}/core";
import type { DeploymentAgentWorkerSdkClient } from "@{{app-name}}/deployment-agent-worker";
import type { IamSdkClient } from "@{{app-name}}/iam";
```

## Import Organization (7 Layers)

Always separate groups with blank lines:

```typescript
// 1. Node built-ins (always use node: prefix)
import crypto from "node:crypto";
import path from "node:path";
import { Buffer } from "node:buffer";

// 2. External dependencies
import { EntityManager, Collection, MikroORM } from "@mikro-orm/core";
import { Queue, Worker, Job } from "bullmq";

// 3. @{{app-name}}/core + other @forklaunch packages
import {
  string,
  optional,
  array,
  enum_,
  date,
  record,
  handlers,
  schemaValidator,
  forklaunchRouter,
  SHARED_SESSION_SCHEMA,
  SqlBaseEntity,
} from "@{{app-name}}/core";
import { isRecord } from "@forklaunch/common";
import { OpenTelemetryCollector } from "@forklaunch/core/http";
import { Lifetime, createConfigInjector } from "@forklaunch/core/services";

// 4. Cross-module imports (other modules in monorepo)
import { generateHmacAuthHeaders } from "@{{app-name}}/core";
import type { DeploymentAgentWorkerSdkClient } from "@{{app-name}}/deployment-agent-worker";

// 5. Local persistence layer
import { Service, Application, Environment } from "../../persistence/entities";

// 6. Local domain layer
import { ServiceStatusEnum } from "../enum/service-status.enum";
import { ServiceSchemas } from "../schemas/service.schema";
import type { PulumiOutputs } from "../types/aws.types";

// 7. Same directory
import { EncryptionService } from "./encryption.service";
import { DeploymentService } from "./deployment.service";
```

## Project Structure

### Backend Module

```
src/modules/<module-name>/
├── api/                          # HTTP interface layer
│   ├── controllers/             # handler functions (handlers.get/post/put/patch/delete)
│   │   ├── service.controller.ts
│   │   ├── application.controller.ts
│   │   └── index.ts             # RE-EXPORTS ALL controllers (required for SDK)
│   ├── routes/                  # forklaunchRouter definitions
│   │   ├── service.routes.ts
│   │   └── application.routes.ts
│   ├── middleware/              # custom middleware
│   └── utils/                   # API helpers
├── domain/                       # Business logic layer
│   ├── services/                # business logic classes (NO mappers, return entities)
│   │   ├── service.service.ts
│   │   └── application.service.ts
│   ├── schemas/                 # natural object notation schemas
│   │   ├── service.schema.ts
│   │   ├── application.schema.ts
│   │   ├── shared.schema.ts     # shared/reusable schema fragments
│   │   └── common.schema.ts
│   ├── types/                   # TypeScript interfaces and type definitions
│   ├── mappers/                 # requestMapper/responseMapper (controllers only)
│   │   ├── service.mappers.ts
│   │   └── application.mappers.ts
│   ├── enum/                    # const-as-const enums
│   │   ├── service-status.enum.ts
│   │   └── index.ts             # re-exports all enums
│   ├── constants/               # constant values
│   ├── guards/                  # type guards
│   └── utils/                   # domain utility functions
├── persistence/                  # Data layer
│   ├── entities/                # MikroORM @Entity classes (extend SqlBaseEntity)
│   │   ├── service.entity.ts
│   │   ├── application.entity.ts
│   │   └── index.ts             # re-exports all entities
│   └── seeders/                 # test/seed data
├── migrations-postgresql/        # timestamped migration files
├── websocket/                   # WebSocket handlers
├── registrations.ts             # DI setup (createConfigInjector + chain)
├── bootstrapper.ts              # env loading, creates DI container
├── server.ts                    # forklaunchExpress, route mounting, listen
├── sdk.ts                       # SDK client type definition
├── mikro-orm.config.ts          # MikroORM configuration
└── package.json                 # pnpm scripts (dev, test, build, migrate:*)
```

### Frontend

```
client/
├── app/                          # Next.js app router
│   ├── dashboard/               # authenticated pages
│   │   ├── services/[id]/page.tsx
│   │   ├── services/page.tsx
│   │   ├── applications/
│   │   ├── workers/
│   │   ├── environments/
│   │   └── layout.tsx
│   ├── login/page.tsx
│   ├── signup/page.tsx
│   ├── layout.tsx               # root layout
│   └── page.tsx
├── components/
│   ├── ui/                      # Shadcn/Radix components
│   ├── billing/                 # feature-gate, upgrade-modal
│   └── ...
├── contexts/
│   └── auth-context.tsx         # AuthProvider, useAuth
├── hooks/
│   ├── use-toast.ts
│   └── use-feature-access.tsx
├── lib/
│   ├── api.ts                   # pre-initialized SDK clients
│   └── hooks/use-api.ts         # useApi, useMutation
└── types/
```

## File Naming Conventions

| Type       | File Pattern                     | Naming Style        |
| ---------- | -------------------------------- | ------------------- |
| Controller | `<resource>.controller.ts`       | kebab-case filename |
| Service    | `<resource>.service.ts`          | kebab-case filename |
| Entity     | `<resource>.entity.ts`           | kebab-case filename |
| Schema     | `<resource>.schema.ts`           | kebab-case filename |
| Mapper     | `<resource>.mappers.ts`          | kebab-case filename |
| Routes     | `<resource>.routes.ts`           | kebab-case filename |
| Enum       | `<name>.enum.ts`                 | kebab-case filename |
| Types      | `<resource>.types.ts`            | kebab-case filename |
| Test       | `<resource>.test.ts`             | kebab-case filename |
| Migration  | `Migration<timestamp>_<desc>.ts` | timestamped         |

## Class/Function Naming

| Type                | Style      | Example                                                   |
| ------------------- | ---------- | --------------------------------------------------------- |
| Entity class        | PascalCase | `class Service extends SqlBaseEntity`                     |
| Service class       | PascalCase | `class ServiceService`                                    |
| Controller function | camelCase  | `export const createService = handlers.post(...)`         |
| Enum const          | PascalCase | `const ServiceStatusEnum = { ... } as const`              |
| Schema const        | PascalCase | `const ServiceSchemas = { CreateServiceSchema: { ... } }` |
| Mapper const        | PascalCase | `const ServiceMapper = responseMapper({ ... })`           |
