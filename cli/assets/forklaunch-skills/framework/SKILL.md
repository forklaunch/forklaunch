---
name: framework
description: "HTTP framework: handler defs, route config, validation, auth, OpenAPI, MCP, SDK gen, streaming."
user-invokable: true
---

# ForkLaunch Framework Patterns

## When to Use This Skill

Use when working with the ForkLaunch framework's HTTP layer — routes, handlers, validation, auth, and OpenAPI.

## Core Concepts

ForkLaunch wraps Express (or Hyper-Express) with type-safe routing, automatic OpenAPI generation, schema validation (Zod or TypeBox), MCP server generation, and observability. Everything imports from `@{{app-name}}/core`. The framework abstracts away HTTP server and validator differences — handlers, routers, and schemas are identical regardless of which alternatives you choose.

## Handler/Route Definition

### handlers.METHOD(schemaValidator, path, config, handler)

```typescript
import {
  handlers,
  schemaValidator,
  string,
  number,
  optional,
  array,
  enum_,
  date,
  SHARED_SESSION_SCHEMA,
} from "@{{app-name}}/core";

// GET - no body, optional query/params
export const listItems = handlers.get(
  schemaValidator,
  "/",
  {
    name: "List Items", // NO forward slashes
    summary: "Get all items", // OpenAPI description
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_VIEWER_ROLES,
    },
    query: {
      page: optional(number),
      limit: optional(number),
      status: optional(string),
    },
    responses: {
      200: array({ id: string, name: string, status: string }),
      401: string,
      500: string,
    },
  },
  async (req, res) => {
    // req.query is typed: { page?: number, limit?: number, status?: string }
    // req.session is typed from sessionSchema
    res.status(200).json(results);
  },
);

// POST - with body
export const createItem = handlers.post(
  schemaValidator,
  "/",
  {
    name: "Create Item",
    summary: "Create a new item",
    auth: {
      /* ... */
    },
    body: {
      name: string,
      description: optional(string),
      config: optional({ key: string, value: string }),
    },
    responses: {
      201: { id: string, name: string, createdAt: date },
      400: string,
      401: string,
      403: string,
    },
  },
  async (req, res) => {
    // req.body is typed from body schema
    res.status(201).json(result);
  },
);

// PATCH - with params + body
export const updateItem = handlers.patch(
  schemaValidator,
  "/:id",
  {
    name: "Update Item",
    params: { id: string },
    body: { name: optional(string), status: optional(string) },
    auth: {
      /* ... */
    },
    responses: { 200: { id: string, name: string }, 404: string },
  },
  async (req, res) => {
    // req.params.id is typed as string
    // req.body is typed from body schema
  },
);

// DELETE
export const deleteItem = handlers.delete(
  schemaValidator,
  "/:id",
  {
    name: "Delete Item",
    params: { id: string },
    auth: {
      /* ... */
    },
    responses: { 200: string, 404: string },
  },
  async (req, res) => {
    res.status(200).send("Deleted");
  },
);
```

A `204` response must never declare or send a body — use `200: string` (as above) for a
confirmation message, or `responses: { 204: ... }` paired with `res.status(204).send()` (no
argument) if the contract genuinely has no body.

### Response Contract Rules

- `responses[status]` is the compile-time contract for `res.status(status).json(...)`. If the schema describes a domain DTO/entity, return that shape exactly; do not leave scaffold starter responses such as `{ message: "Service" }` wired to a domain response schema.
- When replacing generated starter `{ message: string }` schemas with real fields, update the GET/POST starter handlers, tests, and services in the same pass. If the starter route must remain for scaffold tests, give it a dedicated starter response schema instead of reusing the domain schema.
- For service/worker capability groups, the HTTP contract and worker contract are one system contract. If a controller enqueues or previews worker work, its response schema, route export, SDK export, worker job payload, event record, and tests must use the same field names and required/optional status.
- If a service signature expects a command object, the controller must pass the matching object. Prefer `req.body` when the body schema is the command and an explicit object when combining `req.params`, `req.query`, `req.session`, and `req.body`.
- Controller `body` config must follow the local generated handler shape. If TypeScript says the body config is not assignable to `Body<...>`, compare a sibling controller and wrap/pass the schema the same way; do not change service contracts to primitives to silence the handler type.
- Controller, route, and SDK exports must stay in lockstep. Every function imported by `api/routes/*.routes.ts`, `api/controllers/index.ts`, or `sdk.ts` must be exported by the controller file under the exact same name. When renaming handlers, update all three layers in the same patch.
- For protected routes, enrich command objects with server-owned context before calling services when persistence requires it. Common examples are `userId` and `organizationId` from `req.session`, `tokenHash` from a generated token, and worker `jobId`/`requestId` generated by the service.

## Router Definition

```typescript
import { forklaunchRouter, schemaValidator } from "@{{app-name}}/core";

const itemRouter = forklaunchRouter(
  "/items",
  schemaValidator,
  openTelemetryCollector,
);

// Mount handlers — export each route individually
export const listItemsRoute = itemRouter.get("/", listItems);
export const createItemRoute = itemRouter.post("/", createItem);
export const getItemRoute = itemRouter.get("/:id", getItem);
export const updateItemRoute = itemRouter.patch("/:id", updateItem);
export const deleteItemRoute = itemRouter.delete("/:id", deleteItem);
```

**Registration order matters when a router mixes parameterized and specific/static paths.** A `GET /:id` registered before a more specific path like `/secure/:id` or `/internal/count` on the same router swallows requests to those paths — they get matched (and typically rejected, e.g. on `:id`'s uuid validation) by `/:id` first and never reach the intended handler, usually surfacing as a confusing 404. Always register specific/static paths **before** parameterized catch-alls:

```typescript
export const listItemsRoute = itemRouter.get("/", listItems);
export const secureGetRoute = itemRouter.get("/secure/:id", secureGetItem); // specific — first
export const internalCountRoute = itemRouter.get("/internal/count", internalCount); // specific — first
export const getItemRoute = itemRouter.get("/:id", getItem); // catch-all — last
```

## Application Setup (server.ts)

```typescript
import { forklaunchExpress, SchemaValidator } from "@{{app-name}}/core";
import { OpenTelemetryCollector } from "@forklaunch/core/http";

const app = forklaunchExpress(SchemaValidator(), openTelemetryCollector, {
  auth: {
    surfaceRoles: async (orgId, req) => {
      /* return roles Set */
    },
    surfacePermissions: async (orgId, req) => {
      /* return perms Set */
    },
    surfaceFeatures: async (orgId, req) => {
      /* return features Set */
    },
    surfaceSubscription: async (orgId, req) => {
      /* return subscription */
    },
  },
});

// Mount routers
app.use(serviceRouter);
app.use(applicationRouter);

// Start
app.listen(Number(getEnvVar("PORT")), () => {
  console.log(`Server running on port ${getEnvVar("PORT")}`);
});
```

## Schema Validation

Schemas are **natural object notation** using primitives from `@{{app-name}}/core`:

Important separation:

- Validator schemas use exported primitive values such as `string`, `number`, `boolean`, `date`, `array(...)`, `optional(...)`. Do not call `number()` in schemas.
- The generated validator surface does not include `nullish(...)`. For nullable fields, use the sibling pattern, usually `optional(string.nullable())`, `optional(date.nullable())`, or `optional(number.nullable())`.
- `enum_` is for generated enum objects/records. Do not pass readonly arrays such as `["normal", "critical"] as const` to `enum_`; define an enum-like object or use the sibling literal-union pattern with `union([literal("a"), literal("b")])`.
- Persistence entities use the `fp` property builder from `@forklaunch/core/persistence`. Do not translate validator `number` into `fp.number()`: current generated entities use `fp.integer()` for counters/counts/whole-number config and `fp.double()` for decimal amounts, measurements, scores, and percentages.
- If a builder name is unclear, copy the closest sibling entity pattern or read the generated persistence exports before writing it.
- Build repair should use compiler output as the source of truth. Watch-mode lines like `Restarting`, `Shutting down application`, `Process didn't exit in 5s. Force killing`, and `MaxListenersExceededWarning` are informational when the service starts again.

```typescript
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
  email,
  uri,
  union,
  literal,
} from "@{{app-name}}/core";

// Simple schema
const CreateUserSchema = {
  name: string,
  email: email,
  age: optional(number),
};

// Nested objects — just inline
const DetailSchema = {
  user: {
    id: string,
    profile: {
      bio: optional(string),
      avatar: optional(uri),
    },
  },
  tags: array(string),
};

// Enum values
const StatusSchema = {
  status: enum_(ServiceStatusEnum),
};

// Key-value records
const ConfigSchema = {
  settings: record(string, string),
};

// Complex TS types
const ManifestSchema = {
  manifest: type<ReleaseManifest>(),
};

// Nullable
const NullableSchema = {
  deletedAt: optional(date.nullable()),
};

// Union types
const ContactSchema = {
  contact: union([
    { type: literal("email"), value: email },
    { type: literal("phone"), value: string },
  ]),
};

// Arrays of objects
const ListSchema = {
  items: array({
    id: string,
    name: string,
    nested: optional(array({ key: string, value: string })),
  }),
};
```

### Validating schemas programmatically (outside a handler)

To validate a payload against a natural-notation schema directly (e.g. in a unit test, or a script), use `schemaValidator.schemify(...)` to resolve it to the underlying Zod/TypeBox schema, then call that schema's native validation method:

```typescript
import { schemaValidator } from "@{{app-name}}/core";

const compiled = schemaValidator.schemify(MySchema);
const result = compiled.safeParse(payload); // Zod: { success: boolean, data? / error? }
if (!result.success) { /* handle result.error */ }
```

`schemaValidator.compile(...)` is a **different** function — it expects an already-shaped Zod/TypeBox object schema (`ZodObject`/`TObject`), not a natural-notation object literal, and will produce confusing type errors if you pass it a plain schema object. Use `schemify`, not `compile`, when you just want to validate a natural-notation schema. Also note Zod's `safeParse` result has a `success` field, not `ok`.

## File uploads / non-JSON request bodies

A `body` declared with a single non-JSON content type (`file`, `binary`, or `text`) collapses `req.body` **directly** to the transformed value — it does not wrap it in an object keyed by the field name:

```typescript
body: { file: file, contentType: 'application/octet-stream' as const }

// In the handler:
const blob = req.body;       // correct — req.body IS the Blob
const blob = req.body.file;  // WRONG — Blob has no .file property, this is a type error
```

This only applies when `body` has a single field with one of the non-JSON content types. A plain object body (the default, `application/json`) is accessed field-by-field as usual (`req.body.name`).

## Authentication & Authorization

### Per-handler auth

```typescript
// JWT (user-facing endpoints)
auth: {
  sessionSchema: SHARED_SESSION_SCHEMA,
  jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
  allowedRoles: PLATFORM_EDITOR_ROLES,           // Set<string>
  // Optional:
  forbiddenRoles: new Set(['guest']),
  allowedPermissions: new Set(['write:services']),
  requiredFeatures: ['CUSTOM_DOMAINS'],
  requireActiveSubscription: true
}

// HMAC (service-to-service)
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

access: 'internal',
auth: {
  hmac: { secretKeys: { default: HMAC_SECRET_KEY } }
}
```

`access: 'internal'` always requires HMAC auth in the same handler config. Do not omit `auth`, and do not use JWT/session/role options for internal service-to-service routes.

### Making HMAC Calls to Other Services

For route SDK tests, use `TEST_TOKENS.HMAC` from local `test-utils`/`@forklaunch/testing` when available. Use `generateHmacAuthHeaders` for real service-to-service calls, and import it from `@forklaunch/core/http` unless the local app core explicitly re-exports it.

```typescript
import { generateHmacAuthHeaders } from "@forklaunch/core/http";

// path = route path on the target router, with actual values (NOT full URL, NOT router base)
const headers = generateHmacAuthHeaders({
  secretKey: hmacSecretKey,
  method: "GET",
  path: `/organizations/${orgId}/surface-features`,
});

const response = await billingSdk.feature.surfaceFeatures({
  params: { id: orgId },
  headers,
});

// For mutations, include body in the signature
const headers = generateHmacAuthHeaders({
  secretKey: hmacSecretKey,
  method: "PATCH",
  path: `/deployments/${deploymentId}/status`,
  body: updatePayload,
});
```

**Path = route path as defined on the target router, with `:param` replaced by actual values.**
Do NOT include the router base path or full URL.

### Session data

With `sessionSchema: SHARED_SESSION_SCHEMA`, `req.session` is typed:

```typescript
req.session.sub; // user ID
req.session.organizationId; // org ID (from JWT)
req.session.email; // user email
req.session.roles; // user roles string
```

## Contract Config Fields

| Field             | Type                 | Used In          | Description                     |
| ----------------- | -------------------- | ---------------- | ------------------------------- |
| `name`            | `string`             | All              | Route name (NO slashes)         |
| `summary`         | `string`             | All              | OpenAPI description             |
| `auth`            | `object`             | All              | Auth configuration              |
| `body`            | `schema`             | POST/PUT/PATCH   | Request body schema             |
| `params`          | `schema`             | GET/PATCH/DELETE | URL params schema               |
| `query`           | `schema`             | GET              | Query params schema             |
| `responses`       | `{ [code]: schema }` | All              | Response schemas by status code |
| `requestHeaders`  | `schema`             | All              | Custom request header schema    |
| `responseHeaders` | `schema`             | All              | Custom response header schema   |

## SDK Generation

Controllers exported from `api/controllers/index.ts` are automatically included in SDK generation:

```typescript
// api/controllers/index.ts
export { listServices, createService, getService } from "./service.controller";
export { listApplications, createApplication } from "./application.controller";
```

The SDK client is typed and called on the frontend as:

```typescript
const response = await platformApi.service.getService({
  params: { id: "..." },
  headers: { authorization: `Bearer ${token}` },
});
// response.code === 200 => response.response is typed
```

## Streaming File Downloads (ZIP, binary)

ForkLaunch wraps `res.send()` and `res.json()` with response validation middleware (`enrichExpressLikeSend`). This is fine for JSON responses but can cause issues with binary streaming (e.g. ZIP archives) because:

- `deepCloneWithoutUndefined` causes stack overflows on Buffer/stream data
- `generateSchema` can recurse infinitely on complex response bodies
- Buffering a large response in memory before sending causes 504 gateway timeouts behind ALBs

### Solution: `responseValidation: 'none'` + `archive.pipe()` + early response start

```typescript
import { file } from "@{{app-name}}/core";
import archiver from "archiver";
import type { Readable } from "stream";

export const downloadZip = handlers.get(
  schemaValidator,
  "/download",
  {
    name: "Download ZIP",
    summary: "Stream files as a ZIP archive",
    auth: {
      /* ... */
    },
    params: { id: string },
    responseHeaders: {
      "Content-Type": string,
      "Content-Disposition": string,
      "Cache-Control": string,
    },
    responses: {
      200: file, // tells OpenAPI this returns a binary file
      404: string,
      500: string,
    },
    options: {
      responseValidation: "none", // CRITICAL: skip deepClone/generateSchema
    },
  },
  async (req, res) => {
    // 1. Do fast validation (DB lookups, S3 list) — can still send error status
    //    ...

    // 2. Start the response ASAP — gets first byte to the ALB/proxy
    //    This prevents idle-timeout (ALB default: 60s) from killing the connection.
    res.setHeader("Content-Type", "application/zip");
    res.setHeader("Content-Disposition", 'attachment; filename="archive.zip"');
    res.setHeader("Cache-Control", "no-cache");

    const archive = archiver("zip", { zlib: { level: 1 } });
    const nodeRes = res as unknown as import("stream").Writable;
    archive.pipe(nodeRes);

    // 3. Kick off slow work (metadata, DB queries) concurrently
    const metadataPromise = buildMetadata().catch(() => null);

    // 4. Stream files into the archive (bytes keep flowing to client)
    for (const key of fileKeys) {
      const body = await fetchFileStream(key); // e.g. S3 GetObject
      archive.append(body as Readable, { name: key });
    }

    // 5. Await slow work and append as last entry
    const metadata = await metadataPromise;
    if (metadata) {
      archive.append(JSON.stringify(metadata, null, 2), {
        name: "metadata.json",
      });
    }

    // 6. Finalize — flushes remaining data through the pipe
    await archive.finalize();
  },
);
```

### Why this pattern matters

| Problem                                       | Cause                                                               | Fix                                                                        |
| --------------------------------------------- | ------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| Stack overflow on `deepCloneWithoutUndefined` | ForkLaunch tries to deep-clone Buffer/stream in response validation | `responseValidation: 'none'`                                               |
| Stack overflow on `generateSchema`            | Recursive schema generation on complex response bodies              | `responseValidation: 'none'`                                               |
| 504 gateway timeout                           | ALB idle timeout hit before first byte sent                         | Start `archive.pipe(res)` before slow work; run metadata concurrently      |
| Response never arrives                        | ForkLaunch `enrichExpressLikeSend` wraps `res.send()`/`res.json()`  | `archive.pipe()` bypasses the wrapper — writes directly to the Node socket |

### Key points

- **`file` from `@{{app-name}}/core`** — use as the 200 response schema for binary downloads
- **`responseValidation: 'none'`** — disables ForkLaunch's response validation middleware entirely for this endpoint
- **`responseHeaders`** — declare custom headers so `res.setHeader()` calls type-check
- **Start streaming early** — do fast checks first (auth, DB exists, S3 list), then immediately start the response before any slow work (metadata building, large S3 fetches)
- **Run slow work concurrently** — kick off `buildMetadata()` as a promise, stream files, then `await` and append metadata last
- **`archive.pipe(res as unknown as Writable)`** — pipes directly to the Node response, bypassing ForkLaunch's `res.send()` wrapper

## HTTP Framework Alternatives

ForkLaunch supports two HTTP server implementations. The choice is made at `init application` time via `--http-framework`:

### Express (default)

Standard Express.js adapter. Broad ecosystem compatibility, battle-tested.

```typescript
import { forklaunchExpress } from "@{{app-name}}/core";
const app = forklaunchExpress(SchemaValidator(), otel, {
  auth: {
    /* ... */
  },
});
```

### Hyper-Express (high-performance)

Based on uWebSockets.js. Significantly higher throughput, HTTP/2 support, built-in clustering.

```typescript
import { forklaunchHyperExpress } from "@{{app-name}}/core";
const app = forklaunchHyperExpress(SchemaValidator(), otel, {
  auth: {
    /* ... */
  },
});
```

**Key differences:**

- Fewer HTTP methods (10 core vs 30+ in Express)
- Not compatible with Bun runtime (CLI forces Express when `--runtime bun`)
- Native WebSocket support via `.ws()` route method
- Clustering defaults to kernel-level routing

**Migration impact:** Handlers, routers, schemas, auth — all identical between Express and Hyper-Express. Only `server.ts` import changes. The `forklaunchRouter()` factory works with both.

## Validator Alternatives

ForkLaunch supports two schema validators. The choice is made at `init application` time via `--validator`:

### Zod (default)

Schema-first validation with wide ecosystem support.

### TypeBox

JSON Schema-based validation using `@sinclair/typebox`. Faster runtime performance, but smaller ecosystem.

**Switching validators:**

```typescript
// Only the core package import changes. ALL schema definitions stay identical.
// Zod:    import { SchemaValidator } from '@forklaunch/validator/zod';
// TypeBox: import { SchemaValidator } from '@forklaunch/validator/typebox';
```

Both validators support the same natural object notation (`{ name: string }`), the same primitives, and the same OpenAPI generation.

**Constraint:** MCP server generation only works with Zod (TypeBox is not supported for MCP).

## MCP Server Generation

ForkLaunch auto-generates a Model Context Protocol (MCP) server from your handlers when using the Zod validator. This enables AI agents (Claude, etc.) to discover and call your API.

- **Default port:** application port + 2000
- **Endpoint:** `/mcp`
- **Configuration** in `forklaunchExpress()`:
  ```typescript
  const app = forklaunchExpress(SchemaValidator(), otel, {
    auth: {
      /* ... */
    },
    mcp: true, // or false to disable, or { name, version } for custom config
  });
  ```
- Automatically exposes all registered handlers as MCP tools
- Only available with `ZodSchemaValidator`

## OpenAPI & API Documentation

ForkLaunch auto-generates OpenAPI 3.1.0 specs from handler definitions:

- **Spec endpoint:** `/api/{version}/openapi` (JSON)
- **Swagger UI:** Available at the docs path configured in the app
- **Scalar API Reference:** Alternative API documentation UI
- **Configuration:**
  ```typescript
  const app = forklaunchExpress(SchemaValidator(), otel, {
    auth: {
      /* ... */
    },
    openapi: true, // or { title, description, contact, discreteVersions }
  });
  ```

## Key Rules

1. **`schemaValidator` from `@{{app-name}}/core`** — pre-instantiated, just import and use
2. **Natural object notation for all schemas** — never `z.object()` or `Type.Object()`
3. **Handler `name` has NO forward slashes** — breaks OpenAPI generation
4. **Always define `responses` with error status codes** — for complete OpenAPI docs
5. **Export controllers from `index.ts`** — required for SDK auto-generation
6. **Use `em.flush()` after mutations** — MikroORM unit-of-work pattern
7. **MCP requires Zod** — TypeBox validator does not support MCP generation
