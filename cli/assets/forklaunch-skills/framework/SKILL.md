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
    responses: { 204: string, 404: string },
  },
  async (req, res) => {
    res.status(204).send("Deleted");
  },
);
```

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
auth: {
  hmac: { secretKeys: { default: HMAC_SECRET_KEY } }
}
```

### Making HMAC Calls to Other Services

```typescript
import { generateHmacAuthHeaders } from "@{{app-name}}/core";

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
