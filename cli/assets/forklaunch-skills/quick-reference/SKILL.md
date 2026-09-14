---
name: quick-reference
description: "Cheat sheet: critical rules, templates, commands, patterns at a glance."
user-invokable: true
---

# ForkLaunch Quick Reference

## Critical Rules

| Rule         | Do                                            | Don't                                                    |
| ------------ | --------------------------------------------- | -------------------------------------------------------- |
| Imports      | `from '@{{app-name}}/core'`                   | `from '@forklaunch/validator/*'`, `from '@modules/core'` |
| Schemas      | `{ name: string, age: optional(number) }`     | `z.object({ name: z.string() })`                         |
| Nullable     | `optional(string.nullable())`                 | `nullish(string)` unless local exports prove it exists   |
| Enums        | `const X = { A: 'a' } as const; type X = ...` | `enum X { A = 'a' }`                                     |
| enum_ input  | `enum_(StatusEnum)`                           | `enum_(['a', 'b'] as const)`                             |
| Entities     | `defineComplianceEntity` + `fp`               | `defineEntity` from `@forklaunch/core/persistence`       |
| Numbers      | `fp.integer()` / `fp.double()`                | `fp.number()`                                            |
| Responses    | Return JSON matching `responses[code]`        | `{ message }` against a domain response schema           |
| Controller body | Match sibling handler body config shape    | Bare schema object that fails `Body<...>` typing         |
| Service calls | Pass exact command object from controller    | A primitive when service expects `{ ... }`               |
| Entity create | Add required server-owned fields (`userId`, `organizationId`, `tokenHash`) | Persist a partial request DTO |
| Seeders      | Update `seed.data`, seeders, and test-utils with required domain fields | Leave starter `{ message }` rows |
| Worker events | Keep event schema/record/service/renderer/controller exports aligned | Mix `channel`/`channels`, `message`/`summary`, `fileMimeType`/`mimeType`, or string/object renderer outputs |
| Controller exports | Export route handler names from controller + index + SDK | Import handlers that do not exist |
| HMAC tests    | `TEST_TOKENS.HMAC` for internal route SDK tests | `Bearer test-token` for HMAC/internal routes         |
| Union records | Shared base or discriminated union           | Extending partial variants with conflicting `kind`       |
| Studio DTOs  | Follow sibling schema-typed services          | Force mappers/entity returns into generated DTO services |
| Repair logs  | Fix scoped `TS...` build errors               | Repair `tsx watch` restart / force-kill noise            |
| Mappers      | In controllers only                           | In services                                              |
| Handler name | `'Create Service'`                            | `'service/create'`                                       |
| Manifest     | `forklaunch change ...`                       | `vim manifest.toml`                                      |
| Migrations   | `pnpm migrate:up`                             | raw CLI commands                                         |
| Frontend API | `platformApi.service.get(...)`                | `fetch('/api/...')`                                      |
| Features     | `useFeatureAccess()` hook                     | endpoint probes                                          |

## @{{app-name}}/core Exports

```typescript
// Schema primitives
(string,
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
  binary);

// Express
(forklaunchExpress,
  forklaunchRouter,
  handlers,
  schemaValidator,
  SchemaValidator);

// Shared schemas
(IdSchema, IdsSchema, SHARED_SESSION_SCHEMA);

// Entity base
SqlBaseEntity;

// Types
(Request, Response, NextFunction, ExpressApplicationOptions);
```

## HMAC Auth

For route SDK tests, import `TEST_TOKENS` from local `test-utils` or `@forklaunch/testing` and use `TEST_TOKENS.HMAC` for internal routes. For real service-to-service calls, import `generateHmacAuthHeaders` from `@forklaunch/core/http` unless the local app core explicitly re-exports it.

## Import Order (7 Layers)

1. Node built-ins (`node:crypto`)
2. External deps (`@mikro-orm/core`)
3. `@{{app-name}}/core` + other `@forklaunch/*`
4. Cross-module (`@{{app-name}}/<module>`)
5. Local persistence (`../../persistence/entities`)
6. Local domain (`../enum/...`, `../types/...`)
7. Same directory (`./service`)

## Backend Cheat Sheet

### New Endpoint Checklist

1. **Schema** — `domain/schemas/<resource>.schema.ts`

   ```typescript
   import { string, optional, array } from "@{{app-name}}/core";
   export const MySchemas = { CreateSchema: { name: string } };
   ```

2. **Service** — `domain/services/<resource>.service.ts`

   ```typescript
   async create(params: { data: {...}; organizationId: string; em: EntityManager }): Promise<Entity> { ... }
   ```

3. **Controller** — `api/controllers/<resource>.controller.ts`

   ```typescript
   export const create = handlers.post(schemaValidator, '/', { name: 'Create', access: 'protected', body: Schema, auth: { sessionSchema: SHARED_SESSION_SCHEMA, jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL }, allowedRoles: PLATFORM_EDITOR_ROLES }, responses: { 201: Mapper.schema } }, async (req, res) => { ... });
   ```

4. **Route** — `api/routes/<resource>.routes.ts`

   ```typescript
   const router = forklaunchRouter("/resources", schemaValidator, otel);
   export const createRoute = router.post("/", create);
   ```

5. **Export** — add to `api/controllers/index.ts`

6. **Register** — mount route in `server.ts`

7. **Test** — `pnpm test`

### Entity Template

```typescript
import { Entity, Property, ManyToOne, Enum } from "@mikro-orm/core";
import { SqlBaseEntity } from "@{{app-name}}/core";

@Entity()
export class MyEntity extends SqlBaseEntity {
  @Property({ index: true }) name!: string;
  @Property({ type: "text", nullable: true }) description?: string;
  @Enum({ items: () => MyStatusEnum }) status!: MyStatusEnum;
  @ManyToOne("ParentEntity") parent!: ParentEntity;
  @Property({ type: "json", nullable: true }) config?: Record<string, unknown>;
}
```

### Enum Template

```typescript
export const MyStatusEnum = { ACTIVE: "active", INACTIVE: "inactive" } as const;
export type MyStatusEnum = (typeof MyStatusEnum)[keyof typeof MyStatusEnum];
```

### Handler Auth & Access Options

```typescript
// JWT with roles (user-facing, role-gated)
access: 'protected',
auth: { sessionSchema: SHARED_SESSION_SCHEMA, jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL }, allowedRoles: PLATFORM_EDITOR_ROLES }

// JWT (any logged-in user)
access: 'authenticated',
auth: { sessionSchema: SHARED_SESSION_SCHEMA, jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL } }

// HMAC (service-to-service)
access: 'internal',
auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } }

// Public (no auth)
access: 'public'
```

## Frontend Cheat Sheet

### Page Template

```typescript
"use client";
import { useApi } from "@/lib/hooks/use-api";
import { platformApi } from "@/lib/api";
import { useAuth } from "@/contexts/auth-context";

export default function MyPage() {
  const { getToken } = useAuth();
  const { data, loading, refetch } = useApi(
    async () => {
      const token = await getToken();
      if (!token) return null;
      const res = await platformApi.resource.list({
        headers: { authorization: `Bearer ${token}` },
      });
      return res.code === 200 ? res.response : null;
    },
    { deps: [] },
  );
  // ...
}
```

### API Call Pattern

```typescript
const token = await getToken()
const res = await platformApi.service.method({
  params: { id: "..." },     // URL params
  body: { ... },              // POST/PUT/PATCH body
  query: { ... },             // query params
  headers: { authorization: `Bearer ${token}` }
})
if (res.code === 200) { /* res.response is typed */ }
```

### Feature Check

```typescript
const { checkFeature } = useFeatureAccess();
if (!checkFeature("CUSTOM_DOMAINS").hasAccess) {
  showUpgradeModal();
}
```

## pnpm Scripts

```bash
pnpm dev            # start all services
pnpm test           # run tests
pnpm build          # build all
pnpm migrate:create # new migration
pnpm migrate:up     # run migrations
pnpm migrate:down   # rollback
pnpm lint           # lint
```

## CLI Commands

```bash
forklaunch init service <name> --path ./src/modules --database postgresql
forklaunch init worker <name> --path ./src/modules --type bullmq
forklaunch init library <name> --path ./src/modules
forklaunch change application --runtime bun --dryrun
forklaunch delete service <name>
forklaunch sync all
forklaunch environment validate
forklaunch openapi export
forklaunch sdk mode [lean|full]
forklaunch deploy create --environment staging --region us-east-1
forklaunch release create --version 1.2.3
```

## File Naming

| Type       | Pattern                    |
| ---------- | -------------------------- |
| Controller | `<resource>.controller.ts` |
| Service    | `<resource>.service.ts`    |
| Entity     | `<resource>.entity.ts`     |
| Schema     | `<resource>.schema.ts`     |
| Mapper     | `<resource>.mappers.ts`    |
| Routes     | `<resource>.routes.ts`     |
| Enum       | `<name>.enum.ts`           |
| Types      | `<resource>.types.ts`      |

## Known Gotchas

| # | Issue | What to do |
|---|-------|------------|
| 2 | `access` field missing from handlers | Every handler MUST include `access: 'public' \| 'authenticated' \| 'protected' \| 'internal'`. |
| 3 | `--modules` flag required for non-interactive CLI | CLI/Blueprint fix needed. `forklaunch init application` requires `--modules iam-better-auth billing-stripe` (or similar) to avoid interactive mode. |
| 7 | `em.setFilterParams('tenant', ...)` in seeders | The tenant filter is only registered in `server.ts`. In seeders, write `organizationId` directly into each entity. |
| 8 | `type<X>()` resolves to `unknown` at runtime | You need `as X` casts when passing validated values to typed functions. Consider `array()` with flat schemas instead. |
| 9 | Client-SDK compliance namespace scaffold bug | CLI/Blueprint fix needed. Correct path is `config.iam.compliance`, NOT `config.iam.core.compliance`. |
| 10 | Stub entity coupling in tests | CLI/Blueprint fix needed. After replacing scaffolded stub entities, update `__test__/test-utils.ts`, `*.test.ts`, and seeder files. |
| 11 | Stub entity coupling in seeders | CLI/Blueprint fix needed. Update `persistence/seeders/index.ts` and `persistence/seed.data.ts` when replacing stub entities. |
| 12 | Seeder glob wiring | New seeders go in `persistence/seeders/` and must be imported in `DatabaseSeeder` (`persistence/seeder.ts`). Config glob is `seeder.js` (singular). |
| 13 | Hydrating tenant-encrypted PII/PHI/PCI before tenant context is known | First raw-select the unencrypted owner FK or query a lookup hash, then fork `EntityMgr` with `{ context: { tenantId } }`. For auth surfaces, use tenant-aware snapshot/helpers; never `findOne(UserEntity/InvitationEntity)` from an unscoped EM just to discover ownership. |
| 14 | Starter `{ message }` response drift | If a response schema was changed to domain fields, update/remove the starter GET handler too, or keep a dedicated starter schema for legacy routes/tests. |
| 15 | Service signature drift | If a service now expects `{ reportId, userId, text }`, controllers must pass that object (`req.body` or explicit merge), not one string field. |
| 16 | Controller body config drift | If `body: SomeSchema` fails `Body<...>`, mirror the sibling generated controller's handler config shape instead of changing service logic. |
| 17 | Mapper architecture drift | Current Studio scaffolds often use schema-derived DTOs directly in service interfaces. Follow the local sibling shape; do not force older mapper/entity-return examples. |
| 18 | Watch-mode log noise | `tsx watch` restart, force-kill, shutdown spam, and MaxListeners warnings are not build blockers when the service restarts successfully. |
| 19 | Worker renderer drift | If renderer returns an object, add matching `responses` schema/tests; if it returns `string`, assert a string. Keep `message` vs `summary` consistent across schema, record, service, and fixtures. |
| 20 | Required seed/test fields | When replacing starter records, seed/test fixtures must include all required entity fields such as `reportId`, `sourceType`, `userId`, and `organizationId`. |
| 21 | Worker payload field drift | Use exact generated event fields (`mimeType`, not `fileMimeType`) and narrow optional strings before assigning to required payload fields. |
| 22 | Notification flag drift | Map/filter wider flagged biomarker rows so emitted items include required `biomarkerCode`, `severity`, and `shortLabel`. |
| 23 | Registration factory drift | Do not cast injector functions to `Record<string, unknown>`; restore scaffold registrations and fix constructor/factory signatures. |
| 24 | Mutating shell edits | Use write tools for edits; do not run `sed -i` through `run_command`. |
| 25 | `setFilterParams('tenant', ...)` without `wrapEmWithTenantContext` | The filter scopes **rows**; it does **not** bind the encryption key. A factory doing only the first looks correct -- queries return the right rows, and every test that skips encrypted columns passes -- while encrypted columns keep using the no-tenant key and fail only once a real tenant exists, i.e. in production. Always build the EM with `wrapEmWithTenantContext(Orm.em.fork(opts), context?.tenantId)`. `forklaunch audit` reports this as `tenant-context-half-wired`. |
| 26 | `fork()` / `getSuperAdminContext()` drops the encryption context | `wrapEmWithTenantContext` returns a proxy, so forking off it yields a plain EM carrying no tenant. To read across tenants under one known key, re-wrap: `wrapEmWithTenantContext(getSuperAdminContext(em), organizationId)`. There is no single key for genuinely cross-tenant encrypted data -- resolve each row's tenant, or select around the columns. |
| 27 | Hydrating a whole entity to read one FK | `find`/`findOne` decrypts **every** encrypted column on the row, so one bad or foreign-tenant value takes down an endpoint that never reads it. Pass `fields: [...]`: omitted columns are never hydrated and stay out of later UPDATEs, so ciphertext is untouched. |
