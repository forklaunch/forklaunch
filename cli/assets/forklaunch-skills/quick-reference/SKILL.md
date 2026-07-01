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
| Enums        | `const X = { A: 'a' } as const; type X = ...` | `enum X { A = 'a' }`                                     |
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

// Auth
generateHmacAuthHeaders;

// Shared schemas
(IdSchema, IdsSchema, SHARED_SESSION_SCHEMA);

// Entity base
SqlBaseEntity;

// Types
(Request, Response, NextFunction, ExpressApplicationOptions);
```

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
