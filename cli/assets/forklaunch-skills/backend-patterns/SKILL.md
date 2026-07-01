---
name: backend-patterns
description: "Backend: handlers, services, entities, schemas, routes, DI, auth, feature gating."
user-invokable: true
---

# ForkLaunch Backend Patterns

## When to Use This Skill

Use when the user asks to:

- Add or modify API endpoints (controllers, routes, handlers)
- Create or modify database entities
- Implement business logic in services
- Define validation schemas
- Set up DI registrations
- Configure authentication or authorization
- Implement feature gating
- Work with migrations or database operations

## The Central Import: @{{app-name}}/core

Almost everything imports from `@{{app-name}}/core`, which re-exports from `@forklaunch/validator/zod`, `@forklaunch/express`, and more:

```typescript
// Schema primitives (natural object notation)
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
} from "@{{app-name}}/core";

// Express/Router/Handlers
import {
  forklaunchExpress,
  forklaunchRouter,
  handlers,
  schemaValidator,
  SchemaValidator,
  IdSchema,
  IdsSchema,
  SHARED_SESSION_SCHEMA,
  generateHmacAuthHeaders,
} from "@{{app-name}}/core";

// Types
import type { Request, Response, NextFunction } from "@{{app-name}}/core";

// Entity base
import { SqlBaseEntity } from "@{{app-name}}/core";
```

Other @forklaunch packages imported directly when NOT in core:

```typescript
import { isRecord, camelCase } from "@forklaunch/common";
import { createCacheKey } from "@forklaunch/core/cache";
import { OpenTelemetryCollector } from "@forklaunch/core/http";
import { Lifetime, createConfigInjector } from "@forklaunch/core/services";
import { requestMapper, responseMapper } from "@forklaunch/core/mappers";
import { RedisTtlCache } from "@forklaunch/infrastructure-redis";
```

## Schema Pattern (Natural Object Notation)

Schemas are **plain objects** with validator primitives. NEVER use `z.object()`, `Type.Object()`, or any wrapping function.

```typescript
// domain/schemas/service.schema.ts
import {
  string,
  number,
  optional,
  array,
  enum_,
  date,
  boolean,
  record,
  type,
} from "@{{app-name}}/core";
import { ServiceStatusEnum } from "../enum/service-status.enum";
import type { ReleaseManifest } from "../types/release-manifest.types";
import { SharedSchemas } from "./shared.schema";

export const ServiceSchemas = {
  // Request schemas (what the client sends)
  CreateServiceSchema: {
    name: string,
    description: optional(string),
    version: string,
    applicationId: string,
  },

  UpdateServiceSchema: {
    id: string,
    name: optional(string),
    description: optional(string),
    status: optional(string),
  },

  // Response schemas (what the API returns)
  ServiceSchema: {
    id: string,
    name: string,
    description: optional(string),
    status: string,
    version: string,
    applicationId: string,
    createdAt: date,
    updatedAt: date,
  },

  // Complex nested schemas
  ServiceDetailResponseSchema: {
    id: string,
    name: string,
    controllers: array({
      id: string,
      name: string,
      routes: optional(
        array({
          path: string,
          method: string,
          topology: optional(type<CodeNode>()), // WARNING: type<X>() resolves to unknown at runtime — use `as X` casts
        }),
      ),
    }),
    integrations: array({
      id: string,
      name: string,
      type: enum_(IntegrationType),
      config: SharedSchemas.IntegrationConfigSchema,
    }),
    deployedFqdns: optional(record(string, string)),
    metadata: optional(record(string, string)),
  },

  // Query parameter schemas
  ListServicesQuerySchema: {
    applicationId: optional(string),
    status: optional(string),
    page: optional(number),
    limit: optional(number),
  },
};
```

**Key patterns:**

- Primitives: `string`, `number`, `boolean`, `date`
- Optional: `optional(string)`, `optional(number)`
- Arrays: `array(string)`, `array({ id: string, name: string })`
- Enums: `enum_(ServiceStatusEnum)`
- Records: `record(string, string)`
- Complex types: `type<TypeScriptType>()` -- **WARNING:** `type<X>()` resolves to `unknown` at runtime. You will need `as X` casts when passing validated values to typed functions. For arrays of objects, consider using `array()` with a flat schema instead.
- Nullable: `string.nullable()`, `optional(string.nullable())`
- Reusable fragments: extract as local `const` and reference inline

## Handler/Controller Pattern

Controllers are **standalone exported functions** using `handlers.METHOD(schemaValidator, path, config, handler)`.

```typescript
// api/controllers/service.controller.ts
import {
  handlers,
  schemaValidator,
  string,
  optional,
  array,
} from "@{{app-name}}/core";
import { ci, tokens } from "../../bootstrapper";
import {
  JWKS_PUBLIC_KEY_URL,
  PLATFORM_EDITOR_ROLES,
  PLATFORM_VIEWER_ROLES,
} from "../../constants";
import { ServiceSchemas } from "../../domain/schemas/service.schema";
import { ServiceMapper } from "../../domain/mappers/service.mappers";

// Resolve scoped dependencies — call the factory each invocation
const serviceFactory = ci.scopedResolver(tokens.ServiceService);
const emFactory = ci.scopedResolver(tokens.EntityMgr);

// --- GET (list) ---
export const listServices = handlers.get(
  schemaValidator,
  "/",
  {
    name: "List Services",
    summary: "Get all services for the organization",
    access: "protected",
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_VIEWER_ROLES,
    },
    query: ServiceSchemas.ListServicesQuerySchema,
    responses: {
      200: array(ServiceMapper.schema),
      401: string,
      500: string,
    },
  },
  async (req, res) => {
    const em = emFactory({ context: { tenantId: req.session.organizationId } });
    const service = serviceFactory();

    const results = await service.listServices({
      organizationId: req.session.organizationId,
      applicationId: req.query.applicationId,
      em,
    });

    const dtos = await Promise.all(results.map((s) => ServiceMapper.toDto(s)));
    res.status(200).json(dtos);
  },
);

// --- GET (by id) ---
export const getService = handlers.get(
  schemaValidator,
  "/:id",
  {
    name: "Get Service",
    summary: "Get a service by ID",
    access: "protected",
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_VIEWER_ROLES,
    },
    params: { id: string },
    responses: {
      200: ServiceSchemas.ServiceDetailResponseSchema,
      401: string,
      404: string,
      500: string,
    },
  },
  async (req, res) => {
    const em = emFactory({ context: { tenantId: req.session.organizationId } });
    const service = serviceFactory();

    const result = await service.getService({
      id: req.params.id,
      organizationId: req.session.organizationId,
      em,
    });

    if (!result) {
      res.status(404).send("Service not found");
      return;
    }

    res.status(200).json(await ServiceMapper.toDetailDto(result));
  },
);

// --- POST ---
export const createService = handlers.post(
  schemaValidator,
  "/",
  {
    name: "Create Service",
    summary: "Create a new service",
    access: "protected",
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_EDITOR_ROLES,
    },
    body: ServiceSchemas.CreateServiceSchema,
    responses: {
      201: ServiceMapper.schema,
      401: string,
      403: string,
      500: string,
    },
  },
  async (req, res) => {
    const em = emFactory({ context: { tenantId: req.session.organizationId } });
    const service = serviceFactory();

    const result = await service.createService({
      data: req.body,
      organizationId: req.session.organizationId,
      em,
    });

    await em.flush();
    res.status(201).json(await ServiceMapper.toDto(result));
  },
);

// --- PATCH ---
export const updateService = handlers.patch(
  schemaValidator,
  "/:id",
  {
    name: "Update Service",
    summary: "Update a service",
    access: "protected",
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_EDITOR_ROLES,
    },
    params: { id: string },
    body: ServiceSchemas.UpdateServiceSchema,
    responses: {
      200: ServiceMapper.schema,
      401: string,
      404: string,
    },
  },
  async (req, res) => {
    const em = emFactory({ context: { tenantId: req.session.organizationId } });
    const service = serviceFactory();

    const result = await service.updateService({
      id: req.params.id,
      data: req.body,
      em,
    });

    if (!result) {
      res.status(404).send("Service not found");
      return;
    }
    await em.flush();
    res.status(200).json(await ServiceMapper.toDto(result));
  },
);

// --- DELETE ---
export const deleteService = handlers.delete(
  schemaValidator,
  "/:id",
  {
    name: "Delete Service",
    summary: "Delete a service",
    access: "protected",
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_EDITOR_ROLES,
    },
    params: { id: string },
    responses: {
      204: string,
      401: string,
      404: string,
    },
  },
  async (req, res) => {
    const em = emFactory({ context: { tenantId: req.session.organizationId } });
    const service = serviceFactory();

    await service.deleteService({
      id: req.params.id,
      organizationId: req.session.organizationId,
      em,
    });

    await em.flush();
    res.status(204).send("Deleted");
  },
);
```

## Tenant-Safe Controller Reads

For IAM and other tenant-encrypted modules, do **not** hydrate an entity just to
discover which tenant owns it. If the row contains encrypted scalar columns
(`email`, `token`, `name`, etc.), a naive `em.findOne(...)` can decrypt with the
wrong tenant key and fail before you even reach the ownership check.

Use this sequence instead:

1. Read the unencrypted foreign key with a raw query or dedicated lookup helper.
2. Fork the `EntityManager` with that tenant id.
3. Only then hydrate the full entity or call a service that hydrates it.

```typescript
const rows = (await em.getConnection().execute(
  `
    select organization_id as "organizationId"
    from invitation
    where id = ?
    limit 1
  `,
  [req.params.id]
)) as Array<{ organizationId: string }>;

const organizationId = rows[0]?.organizationId;
if (!organizationId) {
  res.status(404).send('Invitation not found');
  return;
}

if (organizationId !== req.session.organizationId) {
  res.status(403).send('Cannot access another organization');
  return;
}

const scopedEm = emFactory({ context: { tenantId: organizationId } });
const invitation = await invitationService.resendInvitation(
  req.params.id,
  organizationId,
  scopedEm
);
```

Do not rely on eager relation hydration for authorization checks on encrypted
entities.

For IAM auth surfaces (`/me`, JWT payload creation, invite accept/resend,
organization switching), use narrow tenant-aware helpers instead of hydrating a
full `UserEntity` or `InvitationEntity` from an unscoped EM:

- raw lookup helper for `user.id -> organizationId`
- scoped helper for user/organization display fields
- raw authorization-surface helper for roles and permissions
- lookup-hash columns for encrypted values such as email, token, and account
  login

If a UI flag depends on invite state, do not trust one metadata bit only. Derive
it from non-PII membership/invitation signals too, such as active
`OrganizationUser.invitedBy` and pending/accepted `Invitation` rows matched by
`email_lookup_hash + organization_id`.

**Handler config fields:**

- `name` — PascalCase/spaced, NO forward slashes (breaks OpenAPI)
- `summary` — description for docs
- `access` — **REQUIRED**: `'public'`, `'authenticated'`, `'protected'`, or `'internal'`
- `auth` — `sessionSchema`, `jwt`, `hmac`, `allowedRoles`, `requiredFeatures`
- `body` — request body schema (POST/PUT/PATCH only)
- `params` — URL params schema (`/:id` => `{ id: string }`)
- `query` — query params schema
- `responses` — keyed by HTTP status code
- `requestHeaders`, `responseHeaders` — header schemas

**Controller index export (required for SDK generation):**

```typescript
// api/controllers/index.ts
export {
  listServices,
  createService,
  getService,
  updateService,
  deleteService,
} from "./service.controller";
export { listApplications, createApplication } from "./application.controller";
```

## Service Pattern

Services accept a **params object with `em: EntityManager`** and return **entities (never DTOs)**.

```typescript
// domain/services/service.service.ts
import { EntityManager } from "@mikro-orm/core";
import { Service, Application } from "../../persistence/entities";
import { ServiceStatusEnum } from "../enum/service-status.enum";

export class ServiceService implements IServiceService {
  async listServices(params: {
    organizationId: string;
    applicationId?: string;
    em: EntityManager;
  }): Promise<Service[]> {
    const { organizationId, applicationId, em } = params;

    const where: Record<string, unknown> = {
      application: { organizationId },
    };
    if (applicationId) {
      where.application = {
        ...(where.application as object),
        id: applicationId,
      };
    }

    return em.find(Service, where, {
      populate: ["application", "controllers"],
      orderBy: { createdAt: "DESC" },
    });
  }

  async createService(params: {
    data: {
      name: string;
      description?: string;
      version: string;
      applicationId: string;
    };
    organizationId: string;
    em: EntityManager;
  }): Promise<Service> {
    const { data, organizationId, em } = params;

    // Verify ownership (multi-tenancy check)
    const application = await em.findOneOrFail(Application, {
      id: data.applicationId,
      organizationId,
    });

    const service = em.create(Service, {
      name: data.name,
      description: data.description,
      version: data.version,
      application,
      status: ServiceStatusEnum.PENDING,
    });

    em.persist(service);
    return service;
  }

  async updateService(params: {
    id: string;
    data: Partial<{ name: string; description: string; status: string }>;
    em: EntityManager;
  }): Promise<Service | null> {
    const { id, data, em } = params;

    const service = await em.findOne(Service, { id });
    if (!service) return null;

    em.assign(service, data);
    return service;
  }

  // For complex operations, use transactions
  async deleteService(params: {
    id: string;
    organizationId: string;
    em: EntityManager;
  }): Promise<void> {
    const { id, organizationId, em } = params;

    await em.transactional(async (txEm) => {
      const service = await txEm.findOneOrFail(Service, { id });
      // Verify ownership
      await txEm.populate(service, ["application"]);
      if (service.application.organizationId !== organizationId) {
        throw new Error("Access denied");
      }
      txEm.remove(service);
    });
  }
}
```

**Key rules:**

- `em: EntityManager` always passed in params (not injected)
- Return entities, NEVER DTOs
- No mappers imported or used
- Verify `organizationId` for multi-tenancy
- Use `em.transactional()` for multi-entity operations
- MikroORM methods: `find`, `findOne`, `findOneOrFail`, `create`, `persist`, `assign`, `remove`

## Entity Pattern

Extend `SqlBaseEntity` from `@{{app-name}}/core` (provides `id: string`, `createdAt: Date`, `updatedAt: Date`).

```typescript
// persistence/entities/service.entity.ts
import {
  Entity,
  Property,
  ManyToOne,
  OneToMany,
  Collection,
  Enum,
} from "@mikro-orm/core";
import { SqlBaseEntity } from "@{{app-name}}/core";
import { ServiceStatusEnum } from "../../domain/enum/service-status.enum";
import { Application } from "./application.entity";
import { Controller } from "./controller.entity";

@Entity()
export class Service extends SqlBaseEntity {
  @Property({ index: true })
  name!: string;

  @Property({ type: "text", nullable: true })
  description?: string;

  @Property()
  version!: string;

  @Enum({ items: () => ServiceStatusEnum })
  status!: ServiceStatusEnum;

  @ManyToOne("Application")
  application!: Application;

  @OneToMany("Controller", "service")
  controllers = new Collection<Controller>(this);

  @Property({ type: "json", nullable: true })
  metadata?: Record<string, unknown>;

  @Property({ type: "json", nullable: true })
  deployedFqdns?: Record<string, string>;
}
```

**Decorator reference:**
| Decorator | Usage |
|-----------|-------|
| `@Property()` | basic column |
| `@Property({ index: true })` | indexed column |
| `@Property({ type: 'text', nullable: true })` | nullable text |
| `@Property({ type: 'json', nullable: true })` | JSON column |
| `@Enum({ items: () => EnumType })` | enum column |
| `@ManyToOne('EntityName')` | foreign key |
| `@OneToMany('Entity', 'inverseField')` | reverse relationship |
| `@ManyToMany('Entity')` | many-to-many |
| `@Unique()` | unique constraint |

Entity re-exports: `persistence/entities/index.ts` re-exports all entities.

## Enum Pattern

```typescript
// domain/enum/service-status.enum.ts
export const ServiceStatusEnum = {
  PENDING: "pending",
  RUNNING: "running",
  STOPPED: "stopped",
  ERROR: "error",
} as const;

export type ServiceStatusEnum =
  (typeof ServiceStatusEnum)[keyof typeof ServiceStatusEnum];
```

NEVER use TypeScript `enum` keyword.

## Route Pattern

```typescript
// api/routes/service.routes.ts
import { forklaunchRouter, schemaValidator } from "@{{app-name}}/core";
import { ci, tokens } from "../../bootstrapper";
import {
  listServices,
  createService,
  getService,
  updateService,
  deleteService,
} from "../controllers/service.controller";

const otel = ci.resolve(tokens.OpenTelemetryCollector);

const serviceRouter = forklaunchRouter("/services", schemaValidator, otel);

export const listServicesRoute = serviceRouter.get("/", listServices);
export const createServiceRoute = serviceRouter.post("/", createService);
export const getServiceRoute = serviceRouter.get("/:id", getService);
export const updateServiceRoute = serviceRouter.patch("/:id", updateService);
export const deleteServiceRoute = serviceRouter.delete("/:id", deleteService);
```

## DI / Registrations

```typescript
// registrations.ts
import { Lifetime, createConfigInjector } from "@forklaunch/core/services";
import { SchemaValidator, string, number } from "@{{app-name}}/core";
import { getEnvVar } from "@forklaunch/common";
import { EntityManager, MikroORM } from "@mikro-orm/core";

const ci = createConfigInjector(SchemaValidator(), {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: { name: string, version: string },
    value: { name: "my-service", version: "0.1.0" },
  },
});

// Chain env config
const envConfig = ci.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar("HOST"),
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar("PORT")),
  },
});

// Chain runtime deps — factory first param MUST use destructuring
const runtimeDeps = envConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(config),
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }) => MikroORM.em.fork(), // destructured param REQUIRED
  },
});

// Chain service deps
const serviceDeps = runtimeDeps.chain({
  ServiceService: {
    lifetime: Lifetime.Scoped,
    type: ServiceService,
    factory: () => new ServiceService(),
  },
});

export const tokens = serviceDeps.tokens();
```

**Key:** Factory first param uses **destructuring** — the DI system introspects argument names to resolve dependencies.

## Auth Configuration

```typescript
// JWT with roles (user-facing, role-gated endpoints)
access: 'protected',
auth: {
  sessionSchema: SHARED_SESSION_SCHEMA,
  jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
  allowedRoles: PLATFORM_EDITOR_ROLES  // Set<string>
}

// HMAC (service-to-service — receiving end)
access: 'internal',
auth: {
  hmac: { secretKeys: { default: HMAC_SECRET_KEY } }
}

// App-level auth (server.ts)
forklaunchExpress(SchemaValidator(), otel, {
  auth: {
    surfaceRoles: async (orgId, req) => { /* fetch from IAM */ },
    surfacePermissions: async (orgId, req) => { /* fetch perms */ },
    surfaceFeatures: async (orgId, req) => { /* fetch from billing */ },
    surfaceSubscription: async (orgId, req) => { /* fetch subscription */ }
  }
});
```

### Making HMAC Calls (Service-to-Service)

Use `generateHmacAuthHeaders` to call HMAC-protected endpoints from other services.

```typescript
import { generateHmacAuthHeaders } from "@{{app-name}}/core";

// GET request
const headers = generateHmacAuthHeaders({
  secretKey: hmacSecretKey,
  method: "GET",
  path: `/deployments/${deploymentId}`,
});

await otherServiceSdk.internal.getDeploymentInternal({
  params: { id: deploymentId },
  headers,
});

// POST/PUT/PATCH — include body in signature
const headers = generateHmacAuthHeaders({
  secretKey: hmacSecretKey,
  method: "PATCH",
  path: `/deployments/${deploymentId}/status`,
  body: updatePayload,
});

await otherServiceSdk.internal.updateDeploymentStatusInternal({
  params: { id: deploymentId },
  body: updatePayload,
  headers,
});
```

**HMAC `path` rules:**

- The path must match `req.path` on the **receiving** server (Express strips the router mount prefix)
- Use the **route path as defined on the router**, NOT the full URL
- Replace param placeholders (`:id`, `:environment`) with actual values
- Do NOT include the router base path (e.g., if router is mounted at `/internal`, use `/deployments/${id}` not `/internal/deployments/${id}`)
- Do NOT include the full URL path (no `/api/v1/service-name/...`)
- **Do NOT include query parameters** — Express `req.path` never includes query strings. Pass query params separately via the SDK's `query` field, but sign only the path portion
- Nested params: `/applications/${appId}/observability/${environment}/${region}`

**Examples:**
| Router route definition | HMAC path |
|---|---|
| `router.get('/deployments/:id', ...)` | `/deployments/${actualId}` |
| `router.get('/releases/:id', ...)` | `/releases/${actualId}` |
| `router.get('/applications/:id/services', ...)` | `/applications/${appId}/services` |
| `router.put('/applications/:id/observability/:env/:region', ...)` | `/applications/${appId}/observability/${env}/${region}` |
| `router.get('/deployments', ...)` (with query) | `/deployments` (NOT `/deployments?status=active`) |
| `router.get('/applications', ...)` (with query) | `/applications` (NOT `/applications?organizationId=xxx`) |

**Common mistake:** Including query parameters in the HMAC path causes "Invalid Authorization signature" (403). The query string is sent via the SDK `query` field but must NOT be part of the signed path.

**Signature computation:** `${method}\n${path}\n${body?}\n${timestamp}\n${nonce}` → HMAC-SHA256 → base64

## Feature Gating & Billing Surfacing (Backend)

**Rule:** never call `billingCacheService.getCachedFeatures(orgId)` or
`billingCacheService.getCachedSubscription(orgId)` directly from a controller.
Each service has its own per-service Redis DB; the raw cache returns `null`
on miss and silently treats paid orgs as free-tier / featureless. Always go
through the surfacing functions — they do cache-then-HMAC-fetch and populate
the local cache on miss.

### Canonical surfacing pattern

Surfacing functions live in **`registrations.ts`** inside
`createDependencyContainer`, and flow out through **`bootstrapper.ts`** as
plain exported consts — same place `ci`/`tokens` come from. There are **no
DI tokens for them, no util wrappers, no setFn plumbing, and no `as never`
casts at controller call sites**.

**`registrations.ts`** (make `createDependencyContainer` async, resolve
deps, await factories in parallel, return alongside `ci`/`tokens`):

```typescript
import {
  createSurfaceFeatures,
  createSurfaceSubscription
} from '@forklaunch-platform/billing';
import {
  createSurfacePermissions,
  createSurfaceRoles
} from '@forklaunch-platform/iam';

export const createDependencyContainer = async (envFilePath: string) => {
  const ci = serviceDependencies.validateConfigSingletons(envFilePath);
  const tokens = serviceDependencies.tokens();

  const authCacheService = ci.resolve(tokens.AuthCacheService);
  const billingCacheService = ci.resolve(tokens.BillingCacheService);
  const iamUrl = ci.resolve(tokens.IAM_URL);
  const billingUrl = ci.resolve(tokens.BILLING_URL);
  const hmacSecretKey = ci.resolve(tokens.HMAC_SECRET_KEY);

  const [surfaceRoles, surfacePermissions, surfaceSubscription, surfaceFeatures]
    = await Promise.all([
      createSurfaceRoles({ authCacheService, iamUrl, hmacSecretKey }),
      createSurfacePermissions({ authCacheService, iamUrl, hmacSecretKey }),
      createSurfaceSubscription({ billingCacheService, billingUrl, hmacSecretKey }),
      createSurfaceFeatures({ billingCacheService, billingUrl, hmacSecretKey })
    ]);

  return {
    ci, tokens,
    surfaceRoles, surfacePermissions, surfaceSubscription, surfaceFeatures
  };
};
```

**`bootstrapper.ts`** (top-level `await`, destructure, re-export):

```typescript
export const {
  ci,
  tokens,
  surfaceRoles,
  surfacePermissions,
  surfaceSubscription,
  surfaceFeatures
} = await createDependencyContainer(envFilePath);
```

**`server.ts`** (import from bootstrapper, hand directly to the framework):

```typescript
import {
  ci,
  surfaceFeatures,
  surfacePermissions,
  surfaceRoles,
  surfaceSubscription,
  tokens
} from './bootstrapper';

const app = forklaunchExpress(SchemaValidator(), openTelemetryCollector, {
  auth: { surfaceRoles, surfacePermissions, surfaceSubscription, surfaceFeatures }
});
```

**Controller usage** (import from bootstrapper — no util, no cast):

```typescript
import { FEATURE_FLAGS } from "@{{app-name}}/core";
import { surfaceFeatures, surfaceSubscription } from "../../bootstrapper";

// Feature flag check (cache-then-HMAC, never silently null)
const features = await surfaceFeatures({ organizationId });
if (!features.has(FEATURE_FLAGS.CUSTOM_DOMAINS)) {
  return res.status(403).send("Custom domains require Pro. Please upgrade.");
}

// Plan-limit check
const subscription = await surfaceSubscription({
  organizationId,
  sub: req.session.sub
});
const limits = getLimitsForPlan(subscription?.planName || "free");
if (limits.maxServices > 0 && count >= limits.maxServices) {
  return res.status(403).send(`Service limit reached (${limits.maxServices}).`);
}
```

### Why no `as never` is needed

The factory-returned type is `(payload: JWTPayload & { organizationId?: string; sub?: string }) => ...`. `JWTPayload` (from `jose`) has all-optional claims. Combined with optional `organizationId`/`sub`, **every field is optional**, so TypeScript accepts a partial object like `{ organizationId }` via structural subtyping. No cast required. If you see `as never` at a call site, delete it.

### Why no DI token, no util wrapper

Earlier the code had both (DI tokens + `surface-features.util.ts` with `surfaceOrgFeatures(orgId)` + `setSurfaceFeaturesFn`-style indirection). All of that was deleted. Reasons:

- **DI token + `ci.resolve(tokens.X)`** forces consumers to `await` a promise per call (or per module) just to unwrap the factory. Module-level top-level await in `bootstrapper.ts` does it once; everyone imports a ready fn.
- **Util wrapper + arity helpers** (`surfaceOrgFeatures(orgId)` wrapping `surfaceFeatures({ organizationId: orgId })`) add a file per surfacing fn that exists only to hide two characters. Not worth the indirection.
- **`setFn`-style wiring** (`setSurfaceFeaturesFn(fn)` called from `server.ts`, then `surfaceOrgFeatures` reads a module-level singleton) was the worst version — adds action-at-a-distance plus a load-order footgun where calling the helper before `server.ts` runs returns an empty set.

### Adding a new surfacing function

1. Import its `createSurfaceX` factory in `registrations.ts`.
2. Add it to the `Promise.all([...])` in `createDependencyContainer`; return alongside `ci`/`tokens`.
3. Destructure the new name in `bootstrapper.ts`'s top-level `await`.
4. Import from `./bootstrapper` wherever you need it. Call with `{ organizationId }` directly.

### Applies across every module

platform-management, observability-api, developer-tools, deployment-agent-worker, resource-management, and billing all use this pattern. If you find factory calls inline in a `server.ts`, that's a regression — move them into `createDependencyContainer`.

### Don't make surfacing lazy to appease scripts

Tempting fix: "scripts/enforce-retention imports bootstrapper but doesn't need IAM, so let's defer `createSurfaceRoles`/`createSurfacePermissions` until first call." **Don't.**

- The blocking is a one-time ~100–500ms OpenAPI fetch at module load, not a real startup cost.
- Every other module awaits surfacing fns in `createDependencyContainer` the same way — diverging here breaks pattern consistency for a theoretical benefit.
- Lazy wrapping (cached-promise-of-factory-of-fn) adds a promise unwrap on every auth request and makes the code harder to reason about.
- If a script genuinely can't tolerate IAM being down at import, the correct fix is a **script-specific bootstrap** that imports `ci`/`tokens` without surfacing fns — not making the server path lazy.

## Mapper Pattern (Brief — Controllers Only)

Mappers use `requestMapper`/`responseMapper` from `@forklaunch/core/mappers`:

```typescript
// domain/mappers/service.mappers.ts
import { responseMapper } from "@forklaunch/core/mappers";
import { schemaValidator } from "@{{app-name}}/core";
import { ServiceSchemas } from "../schemas/service.schema";
import { Service } from "../../persistence/entities";

export const ServiceMapper = responseMapper({
  schemaValidator,
  schema: ServiceSchemas.ServiceSchema,
  entity: Service,
  mapperDefinition: {
    toDto: async (entity: Service) => ({
      id: entity.id,
      name: entity.name,
      description: entity.description,
      status: entity.status,
      version: entity.version,
      applicationId: entity.application.id,
      createdAt: entity.createdAt,
      updatedAt: entity.updatedAt,
    }),
  },
});
```

**Rules:** Used in controllers only. Services never import mappers. `ServiceMapper.schema` is used in handler response schemas.

## Docker Build Secrets

ForkLaunch supports BuildKit secrets for private package registries during Docker builds. Users can securely access private npm/pnpm/bun packages without baking credentials into image layers.

**How to use:**

- Configure an npm token as an application secret or environment variable in the ForkLaunch dashboard
- In the Dockerfile, mount the secret at build time:
  ```dockerfile
  RUN --mount=type=secret,id=npmrc,target=/root/.npmrc \
      pnpm install --frozen-lockfile
  ```
- Secrets are passed via BuildKit's `--mount=type=secret` mechanism and never appear in image layers or `docker history`

See `docs/docker-build-secrets.md` for full examples (pnpm, npm, bun) and security details.

## Migrations & Scripts

Always use pnpm scripts from the module's `package.json`:

```bash
pnpm migrate:create     # create new migration
pnpm migrate:up          # run pending migrations
pnpm migrate:down        # rollback last migration
pnpm dev                 # start service in dev mode
pnpm test                # run tests
pnpm build               # build
pnpm lint                # lint
```

Never run raw migration CLI commands.

## Module Structure

```
src/modules/<module>/
├── api/
│   ├── controllers/          # handlers.get/post/put/patch/delete
│   │   ├── service.controller.ts
│   │   └── index.ts          # re-exports all (for SDK)
│   ├── routes/               # forklaunchRouter definitions
│   └── middleware/
├── domain/
│   ├── services/             # business logic (NO mappers)
│   ├── schemas/              # natural object notation
│   ├── types/                # TypeScript interfaces
│   ├── mappers/              # requestMapper/responseMapper
│   ├── enum/                 # const-as-const enums
│   ├── constants/
│   ├── guards/
│   └── utils/
├── persistence/
│   ├── entities/             # MikroORM @Entity (SqlBaseEntity)
│   │   └── index.ts          # re-exports all
│   └── seeders/
├── migrations-postgresql/
├── websocket/
├── registrations.ts          # createConfigInjector + chain
├── bootstrapper.ts           # env loading, DI container
├── server.ts                 # forklaunchExpress, routes, listen
└── package.json              # pnpm scripts for migrate, dev, test
```

## Replacing Scaffolded Stub Entities

Each scaffolded service includes a working stub entity (`<Name>Record`) with test data, seeders, and test cases. When adding real domain entities, update these files to use your new entities instead of the stub:

1. **`__test__/test-utils.ts`** -- Change the `setupTestData()` import and `em.create()` call to use your entity and realistic test data
2. **`*.test.ts` files** -- Update SDK method references (e.g., `sdk.nameRecord.list` becomes `sdk.restaurant.list`)
3. **`persistence/seeders/<name>Record.seeder.ts`** -- Replace the stub entity import and `em.create()` with your entity and seed data
4. **`persistence/seed.data.ts`** -- Replace the stub entity import and data object with your entity's required fields

Don't delete these files. Replace the stub entity references with your real entities so the test and seed infrastructure keeps working.

## Seeder Wiring

Seeders must be wired through the `DatabaseSeeder` in `persistence/seeder.ts`. The `mikro-orm.config.ts` has `glob: 'seeder.js'` (singular) pointing to `persistence/seeder.ts`. New seeder classes go in `persistence/seeders/` and must be imported and called via `this.call(em, Object.values(seeders))` in the `DatabaseSeeder` class.

```typescript
// persistence/seeder.ts
import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import * as seeders from './seeders';

export class DatabaseSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    // Write organizationId directly into each entity.
    // The tenant filter is only registered in server.ts and does not run during seeding.
    return this.call(em, Object.values(seeders));
  }
}
```
