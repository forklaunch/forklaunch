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
import { generateHmacAuthHeaders, OpenTelemetryCollector } from "@forklaunch/core/http";
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

### Common Studio Repair Gotchas

- Treat `tsx watch` runtime churn as informational unless the latest block contains a real stack trace and the service does not return to "Server is running". Lines such as `change in ... Restarting`, repeated `Shutting down application`, `Process didn't exit in 5s. Force killing`, and `MaxListenersExceededWarning` are watch-mode noise during live edits, not repair targets.
- Repair build-blocking output first. For `TS...` errors, open the named file at the reported line plus the directly referenced schema/service/interface. Patch that contract mismatch and rerun the scoped build. Do not spend iterations listing or reading broad scaffold directories to rediscover the generated layout.
- Studio parallelizes backend work by product server group, not by every package that happens to have a `server.ts`. Worker packages can include `server.ts` for health/control endpoints; that does not make them independent product servers. If a task owns an API service plus file-parser/analysis/insight/notification workers, treat them as one capability group with one canonical domain contract.
- In a server-scoped backend group, update the HTTP service and workers together: request/response schemas, queue/job payload schemas, event records, service interfaces, controller responses, route exports, SDK exports, tests, seed data, and worker processors must agree in the same patch. Do not let each worker invent its own `reportId`, `jobId`, `message`, `status`, or biomarker shape.
- Contract-first passes are not complete until starter residue is gone from the assigned group. Scan for generic `{ message: string }`, `response.message`, seed rows with `message`, test fixtures with `message`, and worker logs reading `event.message`; either remove them or isolate them behind a dedicated starter-only schema that is not reused by domain routes.
- Validator `number` is a schema primitive value; entity `fp` builders are different. Do not write `fp.number()` unless the generated package exports it. For persisted numeric fields, use the sibling scaffold pattern: usually `fp.integer()` for counts/limits/retries and `fp.double()` for currency, percentages, scores, and measured values.
- Do not write `nullish(...)` in schemas. It is not exported by the current generated validator surface. Use `optional(field.nullable())` or the closest sibling nullable pattern.
- Do not pass readonly arrays to `enum_`. It expects a generated enum object/record. For literal choices, define a local enum-like object or use `union([literal("a"), literal("b")])` when sibling schemas use that pattern.
- Do not import `defineEntity` from `@forklaunch/core/persistence`; recent generated packages do not export it there. Prefer `defineComplianceEntity` for Studio-generated entities, with `.compliance("none")` on non-sensitive scalar fields.
- Keep schemas, services, controllers, tests, and mappers on the same contract. If a schema was changed from starter `{ message: string }` to domain fields, remove `data.message` from service/controller/test code in the same pass.
- Handler response schemas must match the JSON actually sent for that status. If `responses: { 200: FooResponseSchema }` now describes a domain entity, the handler must return that DTO/entity shape; do not keep `res.json({ message: "Foo" })` against a domain response schema. If retaining scaffold starter routes/tests, isolate them behind dedicated starter request/response schemas.
- When a service method changes from a primitive parameter to a command object, update all controller call sites in the same patch. For body-shaped commands, pass `req.body`; for mixed route/body commands, pass `{ ...req.params, ...req.body }` or an explicit object with the service's exact keys. Do not pass `req.body.message` or `req.params.id` to a method that expects an object.
- Controller body schemas are handler config fields. If a request body is an object schema, wrap/pass it exactly as sibling controllers do. When a generated schema type is wider than the command, cast validated `req.body` only at the controller/service boundary; do not reshape it into a primitive.
- Current Studio-generated services often type methods with `Schema<typeof RequestSchema, SchemaValidator>` and return schema DTOs directly when no mapper exists. Follow the sibling generated service shape; do not introduce a mapper/entity-return architecture just to satisfy older examples.
- Mapper DTOs should normalize nullable entity fields to optional API fields with `value ?? undefined` when the response schema uses `optional(...)`. Do not return `null` to an optional-only schema.
- Entity creation must satisfy required persistence fields, not just request fields. If entities require `userId`, `organizationId`, `tokenHash`, `jobId`, or timestamps, derive them in the controller/service from `req.session`, params, generated IDs, or hash helpers before `em.create(...)`. Never pass a partial request DTO directly to persistence when required entity fields are missing.
- **Multi-tenant create args:** if an entity has a `tenantId` field, every service method that creates or queries that entity must accept `tenantId` in its argument object and pass it through to `em.create({ ..., tenantId })` or the `where` clause. Controllers source `tenantId` from `req.session.organizationId`. Do not drop `tenantId` because the schema validator did not flag it — the entity contract requires it and `tsgo -b` will fail with `Property 'tenantId' is missing in type ...`.
- **Date vs ISO string at the API boundary:** entity timestamp/date fields (`collectedAt`, `dateOfBirth`, `createdAt`, etc.) are `Date` in MikroORM but response schemas typed with the validator expect `string` (ISO). On the way in, parse: `collectedAt: new Date(input.collectedAt)`. On the way out (or when assigning into a typed response object), serialize: `collectedAt: entity.collectedAt.toISOString()`. Never assign a raw `Date` to a field typed `string | RawQueryFragment<string>` — that error means the schema is string-shaped and the value is still a `Date`.
- **null → undefined for optional fields:** MikroORM nullable columns are typed `T | null | undefined` but response/optional schemas use `T | undefined`. Normalize with `?? undefined` (e.g. `rawValue: entity.rawValue ?? undefined`, `minValue: entity.minValue ?? undefined`) when building the response DTO. The error `Type 'null' is not assignable to type 'string | undefined'` is this exact mismatch.
- **MikroORM EntityManager has no `createQueryBuilder`.** Use `em.qb(EntityName)` (or `em.find/findOne/findOneOrFail` with a `where` clause) for queries. `em.getKnex()` is only available when you need raw SQL. Do not call `em.createQueryBuilder(...)` — it does not exist on the current `EntityManager<IDatabaseDriver<Connection>>` type and will fail typecheck.
- **MikroORM v7 removed `persistAndFlush` / `removeAndFlush`.** Use `em.persist(entity); await em.flush();` (or `em.remove(entity); await em.flush();`) — two calls, not one. The error `Property 'persistAndFlush' does not exist on type 'EntityManager<...>'` is this exact mismatch.
- **Handler response must be an OBJECT, not a string.** `res.status(N).json(...)` must receive an object matching the response schema for that status. Even if the schema looks like it could accept a primitive (e.g. just `{ jobId }`), return `{ jobId: '...' }`, not `res.json("...")`. The error `Type 'string' is not assignable to type 'ResponseBody<ZodSchemaValidator>'` is this exact mismatch. If you genuinely want a string body, the schema field type itself must be a string-typed schema (e.g. literal/string primitive at the body root).
- **TEST_TOKENS only exports what the local `__test__/test-utils.ts` actually contains.** Most generated scaffolds expose `AUTH` and `HMAC`, NOT `JWT`. Open the file and grep before using `TEST_TOKENS.X` — `TEST_TOKENS.JWT` is the most common false reference. Same caveat applies to `SHARED_SESSION_SCHEMA` and `PLATFORM_*` role constants.
- **Controller export / SDK export / test invocation MUST share the exact identifier.** If you rename a controller from `ingestionGet` to `uploadGetJob`, you must update `routes/*.routes.ts`, `sdk.ts`, AND every `__test__/*.test.ts` invocation in the same patch. The error `Property 'X' does not exist on type '{ Y: ... }'` from a test file means the controller was renamed but the test wasn't.
- **MikroORM v7 entity field wrapper types.** Fields read from `em.create/em.findOne/em.find` carry `Opt<T>` and `NonNullable<string | number>` wrapper types that do NOT auto-narrow to a plain DTO. When a service method declares return `Promise<XResponseSchema>` and you build the return object from an entity, you MUST coerce each non-plain field:
  - `id: entity.id` → `id: entity.id as string`
  - `minValue: entity.minValue` (numeric column) → `minValue: Number(entity.minValue)` or `as number`
  - `createdAt: entity.createdAt` → `createdAt: entity.createdAt as Date`
  - Or end the return with `as Schema<typeof XResponseSchema, SchemaValidator>`.
  The error signature is `Type '{ id: Opt<string>; ... minValue: NonNullable<string | number>; ... }' is not assignable to type '{ id: string; ... minValue: number; ... }'`. NEVER `return entity` directly when the function return type is the schema DTO.
- **Return-type completeness:** if a service method's declared return type lists fields (`{ id, patientId, tenantId, collectedAt, labName, status, ... }`), every property in that list must be present on the returned object. Don't return a partial entity and rely on caller behavior — `tsgo -b` will surface `Property 'X' is missing in type ...`. If a field is optional in the schema, mark it optional in the return type too; otherwise populate it.
- Avoid over-typing mapper callback parameters with MikroORM inferred entity types from a specific package path. Studio workspaces can contain duplicate `.pnpm/@mikro-orm/core` paths, making structurally identical entity types incompatible. Let mapper callbacks infer from the local entity/schema, or map to a plain DTO object with the response schema as the contract.
- When replacing starter data, update every seed/test fixture in the same pass: `persistence/seed.data.ts`, `persistence/seeders/*`, `__test__/test-utils.ts`, and tests that still query `{ message: ... }`. Do not leave scaffold `{ message: string }` fixture rows after domain fields replace the starter schema; include required entity fields such as `reportId`, `sourceType`, `userId`, and `organizationId`.
- Do not define a persisted event/record interface by extending multiple `Partial<...Payload>` variants when the payloads have conflicting discriminants such as `kind: "analysis-complete"` and `kind: "critical-flags"`. Use a shared base plus optional non-discriminant fields, or a discriminated union.
- Worker event records, worker services, renderers, controllers, routes, and SDK exports must use one canonical event contract. If implementation code needs `organizationId`, `requestId`, `recipientEmail`, `pushToken`, `channels`, `severity`, or `flaggedBiomarkers`, add those fields to the event schema/record and fixtures; otherwise remove those reads. Do not mix `channel` and `channels`, `fileMimeType` and `mimeType`, or event kinds not present in the enum/schema.
- If the worker/event schema uses `message`, use `message` everywhere; do not invent `summary` unless it is added to schema, record, service, fixtures, and renderer together. If a renderer returns `{ subject, text, html, pushTitle, pushBody }`, controller `responses`, service return types, and tests must use that object shape; if it returns `string`, tests must assert a string.
- Before assigning generated event payloads or DTOs, narrow optional strings to required strings with explicit guards/defaults. For arrays like `flaggedBiomarkers`, map/filter wider domain rows so every emitted item has required fields such as `biomarkerCode: string`, `severity`, and `shortLabel: string`.
- If a test imports a fixture helper such as `buildEventRecord`, export it from `__test__/test-utils.ts` in the same patch or update the test to the existing helper name.
- Route files and SDK entrypoints import exported controller names. If you add `enqueueNotification`, `previewNotification`, or `getQueueInfo` routes, those exact functions must be exported from the controller file and `api/controllers/index.ts`; otherwise update routes and SDK to the actual generated handler names.
- Do not edit generated runtime registrations to satisfy a service constructor mismatch. Prefer matching the service constructor to the generated factory signature, unless the local scaffold already wires the dependency.
- Never cast a typed injector function directly to `Record<string, unknown>` in generated registrations. If registration typing fails, restore the scaffold registration shape and fix the service constructor/factory contract instead.
- Read local exports before reporting missing framework exports. `@<app-name>/core` and `@forklaunch/core/persistence` are scaffold/version-specific.
- Generated client-sdk methods are named from actual handlers/routes. Tests and wiring must use the exported SDK names; do not invent generic helpers like `apiGet`, `servicePost`, `workerPost`, or `insightWorkerPost`.
- Use write tools for edits. Do not run mutating shell commands such as `sed -i` through `run_command`.

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

**Required env vars:** a service scaffolded with `--infrastructure redis` does not automatically get `REDIS_URL` added to its `.env.local` — add it manually (`REDIS_URL=redis://localhost:6379/0`, matching whatever port Docker Compose actually exposes). Any service declaring `auth.jwt` needs `JWKS_PUBLIC_KEY_URL` in its `.env.local` — it is present in iam's own template but is **not** added automatically to other services' templates (inventory, billing, custom services). Add it manually, pointing at the iam service: `JWKS_PUBLIC_KEY_URL=http://localhost:<iam-port>/api/auth/jwks`. Cross-service auth (a non-iam service calling iam's surfacing endpoints) also needs `IAM_URL` and an `HMAC_SECRET_KEY` shared with iam's — both should already be scaffolded as resolvable tokens, but the `.env.local` values themselves may still be empty until you fill them.

```typescript
// JWT with roles (user-facing, role-gated endpoints)
access: 'protected',
auth: {
  sessionSchema: SHARED_SESSION_SCHEMA,
  jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
  allowedRoles: PLATFORM_EDITOR_ROLES  // Set<string>
}

// HMAC (service-to-service — receiving end)
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

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

**Mandatory internal route rule:** `access: 'internal'` and `auth.hmac` are a pair. If a controller uses `access: 'internal'`, the same handler config must include `auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } }`, and the controller must resolve `HMAC_SECRET_KEY` from `ci.resolve(tokens.HMAC_SECRET_KEY)`. Do not use JWT/session/roles for internal routes, and do not omit `auth`; the handler type will reject it.

**A route-level `auth.surfaceRoles` is silently never invoked once the app already wires a global one** via `forklaunchExpress`'s `auth` option above (or via `createAuthOptions()` in a CLI-scaffolded app's `server.ts` — see below). Don't hand-roll a per-route `surfaceRoles`/`surfacePermissions` callback expecting it to run; only the app/router-level one is called. If you need route-specific role logic, filter inside the app-level surfacing function instead.

**CLI-scaffolded apps (v1.2.6) wire cross-service RBAC inline in `server.ts`**, not via the `registrations.ts` → `bootstrapper.ts` pattern below: `server.ts` defines a local `createAuthOptions()` that calls `createSurfaceRoles()` / `createSurfacePermissions()` (imported from the app's own generated `@{{app-name}}/iam` package, exported from a `surfacing.ts` file for other services to consume) and passes the result straight to `forklaunchExpress`. This works, but diverges from the canonical pattern this repo's own modules follow — check `server.ts` first before adding a new surfacing wire-up, and consider migrating it into `registrations.ts` for consistency if you're touching that file anyway.

### Making HMAC Calls (Service-to-Service)

Use `generateHmacAuthHeaders` to call HMAC-protected endpoints from other services.
For route SDK tests, prefer `TEST_TOKENS.HMAC` from local `test-utils`/`@forklaunch/testing` when available. Do not use `Bearer test-token` for internal/HMAC routes; their SDK header type is `HMAC keyId=...`.

```typescript
import { generateHmacAuthHeaders } from "@forklaunch/core/http";

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
| Router mounted at `/products`, handler at `/internal/count` | `/internal/count` (NOT `/products/internal/count`) |
| Calling iam's `/user/:id/surface-roles` from another service | sign `/{id}/surface-roles`, but `fetch` the full `${IAM_URL}/user/{id}/surface-roles` — the mount prefix is stripped for signing purposes only, not from the URL you actually call |

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

> **Exception:** `deployment-agent-worker/domain/services/pulumi-executor.service.ts` must use lazy dynamic imports for `ci`/`tokens` to avoid a circular dependency with `registrations.ts`:
> ```typescript
> const { ci, tokens } = await import('../../bootstrapper');
> ```
> Do NOT add a top-level `import { ci, tokens }` in that file.

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

Seeders must be wired through the `DatabaseSeeder` in `persistence/seeder.ts`. The `mikro-orm.config.ts` has `glob: 'seeder.js'` (singular) pointing to `persistence/seeder.ts`. New seeder classes go in `persistence/seeders/`.

**Do NOT seed via plain `this.call(em, Object.values(seeders))` if any seeder's entity has a foreign key to another seeder's entity.** `Object.values()` on an ES module namespace enumerates export names **alphabetically** (this is a spec-mandated behavior of module namespace exotic objects — see ECMA-262 §9.4.6.6 `[[OwnPropertyKeys]]`), not in declaration or dependency order — e.g. an `AccountSeeder` runs before a `UserSeeder` even though `account.user_id` references `user.id`, causing a foreign-key-constraint violation. It's deterministic, not intermittent, so it will reproduce every time the alphabetical order disagrees with the FK order.

Keep `Object.values(seeders)` for auto-discovery (so a new seeder file is picked up without editing `seeder.ts`), but topologically sort it against an explicit dependency map so FK-dependent seeders still run in the right order:

```typescript
// persistence/seeder.ts
import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import * as seeders from './seeders';

// Map each seeder to the seeders it depends on (FK references). Only
// entries with a real FK dependency need to be listed — everything else
// is still auto-discovered and sorted in.
const SEEDER_DEPENDENCIES: Partial<Record<keyof typeof seeders, (keyof typeof seeders)[]>> = {
  AccountSeeder: ['UserSeeder'],
  SessionSeeder: ['UserSeeder']
};

function topologicalSort(
  all: typeof seeders,
  deps: typeof SEEDER_DEPENDENCIES
): (typeof seeders)[keyof typeof seeders][] {
  const ordered: (typeof seeders)[keyof typeof seeders][] = [];
  const visited = new Set<string>();

  function visit(name: keyof typeof seeders) {
    if (visited.has(name)) return;
    visited.add(name);
    for (const dep of deps[name] ?? []) {
      visit(dep);
    }
    ordered.push(all[name]);
  }

  for (const name of Object.keys(all) as (keyof typeof seeders)[]) {
    visit(name);
  }
  return ordered;
}

export class DatabaseSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    // Write organizationId directly into each entity.
    // The tenant filter is only registered in server.ts and does not run during seeding.
    return this.call(em, topologicalSort(seeders, SEEDER_DEPENDENCIES));
  }
}
```

Update `SEEDER_DEPENDENCIES` whenever a new seeder's entity has a foreign key to another seeded entity. Seeders with no FK relationships need no entry — they're auto-discovered and sorted in wherever `Object.keys` happens to place them.

**Seeders are not idempotent by default** — re-running `pnpm seed` against an already-seeded database throws unique-constraint violations (they always `em.create` + insert, never upsert). Only run seed on a fresh database, or clear the relevant tables first.
