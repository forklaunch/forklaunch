---
name: development-guidelines
description: "Toolchain: runtimes (node/bun), validators (zod/typebox), databases, formatters, linters, tests, workers."
user-invokable: true
---

# Development Guidelines

## Build & Install Commands

Use the correct package manager based on runtime:

### Bun projects

```bash
bun install          # install dependencies
bun run <script>     # run a package.json script
bun build            # build
bun <file>           # run a file directly
```

### Node projects (pnpm)

```bash
pnpm install         # install dependencies
pnpm run <script>    # run a package.json script
pnpm build           # build
pnpm dev             # local dev server
pnpm tsgo --noEmit   # type-check (frontend)
```

> Never mix package managers. If a project has a `bun.lockb`, use bun. If it has `pnpm-lock.yaml`, use pnpm.

---

## Type Safety — Never Use `any`

**NEVER use `any` in TypeScript code.** Always use proper types, generics, `unknown`, or specific interfaces. If a type is unclear, investigate and define it correctly rather than falling back to `any`. Conform strictly to the patterns laid out in the other skill files (backend-patterns, etc.) — they define the canonical way to write code in this codebase.

---

## Code Style — No Obvious Comments

Do not add comments that restate what the code already clearly says.

**Wrong:**

```typescript
// Set the user to null
setUser(null);

// Check if token exists
if (!token) return;

// Loop through features
featureArray.forEach((slug) => {
  features[slug.toUpperCase()] = true;
});
```

**Right — only comment non-obvious intent:**

```typescript
// Deduplicate concurrent fetches so a burst of 401s doesn't hammer the auth server
if (pendingTokenFetch) {
  return pendingTokenFetch;
}

// 30s buffer gives better-auth time to issue a refreshed token before the old one expires
const TOKEN_EXPIRY_BUFFER_MS = 30_000;
```

The rule: if the comment just says what the next line does, delete it. Only comment _why_, not _what_.

---

## Vite Frontend — Backend Types Without Compilation

If the frontend uses **Vite** (with `@vitejs/plugin-react-swc` or similar), it does **not** need the backend modules to be compiled (`dist/`) to build successfully — even if it imports types from backend packages.

This works because:

- All backend imports in the frontend should be `import type { ... }` — type-only imports
- Vite uses **SWC** for transpilation, which strips `import type` statements entirely before bundling
- The backend types flow into the frontend for editor type safety, but never appear in the bundle
- The only runtime values needed are from published npm packages (e.g. `@forklaunch/universal-sdk`)

**Practical consequence:** In a deployment pipeline (e.g. Vercel), you can scope the build command to just the frontend (`pnpm build` from the `web/` directory) without running the full recursive backend build. This means backend TypeScript errors cannot block frontend deployments.

```json
// web/vercel.json
{
  "buildCommand": "pnpm build"
}
```

The install step still needs to run from the monorepo root (to install workspace deps), but the build step is frontend-only.

---

## ForkLaunch CLI — Worker Generation

When the CLI generates a **worker**, it produces a **service/worker pair** — both a service and a worker module are created together. This means:

- If an app needs **1 worker and 1 service** to operate, generating a single worker is sufficient. You do not need to separately generate a service.
- The generated service acts as the HTTP interface while the worker handles background/async processing.

---

## ForkLaunch Toolchain Choices

All choices are made at `forklaunch init application` time. The CLI manages configurations, dependencies, and build scripts automatically.

### Runtimes

| Runtime | Package Manager | Notes                                                                      |
| ------- | --------------- | -------------------------------------------------------------------------- |
| `node`  | `pnpm`          | Standard Node.js. Broader compatibility.                                   |
| `bun`   | `bun`           | Faster startup, native TypeScript. No `better-sqlite`, no `hyper-express`. |

When using Bun, ForkLaunch includes `@forklaunch/bunrun` — a Bun-native workspace script runner that executes package.json scripts across the monorepo in topological order (respecting inter-package dependencies). It replaces `pnpm -r` recursive commands in Bun projects.

### HTTP Frameworks

| Framework       | Description            | Notes                                                         |
| --------------- | ---------------------- | ------------------------------------------------------------- |
| `express`       | Express.js adapter     | Default. Broad ecosystem, 30+ HTTP methods.                   |
| `hyper-express` | uWebSockets.js wrapper | High throughput, HTTP/2, clustering. Not compatible with Bun. |

Handlers, routers, and schemas are **identical** between both — only `server.ts` imports differ.

### Validators

| Validator | Library           | Notes                                               |
| --------- | ----------------- | --------------------------------------------------- |
| `zod`     | Zod               | Default. Schema-first. Required for MCP generation. |
| `typebox` | @sinclair/typebox | JSON Schema-based. Faster runtime. No MCP support.  |

Both use the same natural object notation (`{ name: string }`). Switching changes only the `SchemaValidator` import.

### Databases (MikroORM)

| Database        | Type  | Notes                                         |
| --------------- | ----- | --------------------------------------------- |
| `postgresql`    | SQL   | Production standard. Full feature support.    |
| `mysql`         | SQL   | Widely available.                             |
| `mariadb`       | SQL   | MySQL-compatible.                             |
| `mssql`         | SQL   | Microsoft SQL Server.                         |
| `mongodb`       | NoSQL | Document-oriented. Different entity patterns. |
| `libsql`        | SQL   | SQLite-compatible, serverless-friendly.       |
| `sqlite`        | SQL   | Embedded, zero-config. Good for dev/testing.  |
| `better-sqlite` | SQL   | Synchronous SQLite. Not supported with Bun.   |

### Formatters

| Formatter  | Notes                                  |
| ---------- | -------------------------------------- |
| `prettier` | Established standard. Configurable.    |
| `biome`    | Faster. Combines formatting + linting. |

### Linters

| Linter   | Notes                                       |
| -------- | ------------------------------------------- |
| `eslint` | Mature ecosystem, extensive plugin support. |
| `oxlint` | Rust-based, significantly faster.           |

### Test Frameworks

| Framework | Notes                         |
| --------- | ----------------------------- |
| `vitest`  | Vite-native, fast, ESM-first. |
| `jest`    | Established, large ecosystem. |

### Worker Types

| Type       | Backing       | Use Case                                         |
| ---------- | ------------- | ------------------------------------------------ |
| `bullmq`   | Redis         | Job queues with retries, scheduling, priorities. |
| `kafka`    | Kafka         | High-throughput event streaming, multi-consumer. |
| `database` | DB polling    | When Redis/Kafka unavailable.                    |
| `redis`    | Redis pub/sub | Simple real-time messaging.                      |

### Infrastructure Add-Ons (per service)

| Add-On  | Token Registered | Use Case                    |
| ------- | ---------------- | --------------------------- |
| `redis` | `TtlCache`       | Caching, sessions, pub/sub. |
| `s3`    | `S3ObjectStore`  | File/object storage.        |

### Preconfigured Modules

| Module            | Description                               |
| ----------------- | ----------------------------------------- |
| `billing-base`    | Billing hooks, no payment provider        |
| `billing-stripe`  | Full Stripe billing integration           |
| `iam-base`        | Authorization hooks, no auth provider     |
| `iam-better-auth` | Better Auth authentication implementation |
