---
name: compliance
description: "Compliance: fp property builder, defineComplianceEntity, access levels, audit CLI, encryption, tenant isolation, DPIA."
user-invokable: true
---

# ForkLaunch Compliance Framework

## When to Use This Skill

Use when the user asks about:

- Compliance field classification (PII, PHI, PCI)
- Entity property builders (`fp` vs `p`)
- `defineComplianceEntity` vs `defineEntity`
- Access levels on routes (public, authenticated, protected, internal)
- Field encryption (AES-256-GCM)
- Tenant isolation (MikroORM filters, PostgreSQL RLS)
- Audit logging
- The `forklaunch compliance audit` CLI command
- Compliance reports in the portal
- GDPR erasure/export endpoints
- Data residency enforcement
- Secrets management

## Architecture Overview

The compliance framework is enforced at the framework level — architecturally impossible to skip:

1. **`fp` property builder** — Proxy over MikroORM's `p` that requires `.compliance()` classification on every scalar field
2. **`defineComplianceEntity`** — Validates all properties have compliance classification at compile time
3. **Access levels** — Required on every route handler (`public`, `authenticated`, `protected`, `internal`)
4. **Field encryption** — AES-256-GCM with per-tenant HKDF key derivation for PII/PHI/PCI fields where configured
5. **Tenant isolation** — MikroORM global filter (mandatory) + optional PostgreSQL RLS
6. **Audit logging** — Automatic for every HTTP/WS request via OpenTelemetry
7. **Rate limiting** — Per-tenant, per-route, per-user Redis-backed
8. **Secrets management** — Boot-time validation with typed `getSecret()`

## Entity Compliance Classification

### The `fp` Property Builder

Every entity must use `fp` (not `p`) and call `.compliance()` on every scalar property:

```typescript
import { defineComplianceEntity, fp } from "@forklaunch/core/persistence";

export const UserEntity = defineComplianceEntity({
  name: "User",
  tableName: "users",
  properties: {
    id: fp.uuid().primary().compliance("none"),
    email: fp.string().unique().compliance("pii"),
    name: fp.string().compliance("pii"),
    ssn: fp.string().nullable().compliance("phi"),
    cardNumber: fp.string().nullable().compliance("pci"),
    role: fp.enum(() => RoleEnum).compliance("none"),
    metadata: fp.json<Metadata>().nullable().compliance("none"),

    // Relations are auto-classified — do NOT call .compliance() on them
    organization: fp.manyToOne(() => OrganizationEntity),
    posts: fp.oneToMany(() => PostEntity, { mappedBy: "author" }),
  },
});
```

### Classification Levels

| Level  | Meaning                             | Encryption     | Examples             |
| ------ | ----------------------------------- | -------------- | -------------------- |
| `none` | No sensitive data                   | No             | IDs, slugs, statuses |
| `pii`  | Personally Identifiable Information | Tenant field encryption where configured | Email, name, address |
| `phi`  | Protected Health Information        | AES-256-GCM    | SSN, medical records |
| `pci`  | Payment Card Industry               | AES-256-GCM    | Card numbers, CVVs   |

### Key Rules

- **`.compliance()` must be last** in the chain: `fp.string().nullable().compliance('pii')` (not `fp.string().compliance('pii').nullable()`)
- **Relations skip `.compliance()`** — they are auto-classified as `'none'`
- **Nullable FK relations** — Use `fp.manyToOne(Entity).nullable()` for nullable foreign keys. The chaining works correctly. The standard non-nullable relation pattern is `() => fp.manyToOne(Entity)`.
- **Base properties** (`createdAt`, `updatedAt`, etc.) in `defineBaseProperties` must also use `fp` with `.compliance('none')`
- Omitting `.compliance()` on a scalar is a **compile-time error**

### `defineComplianceEntity` vs `defineEntity`

- `defineComplianceEntity` — Validates all properties have compliance classification. **Use this for all entities.**
- `defineEntity` — No compliance validation. Only for framework internals or third-party entities.

The return type of `defineComplianceEntity` is identical to `defineEntity` — the `ClassifiedProperty` wrapper is stripped at the type level. MikroORM sees the same entity structure.

## Route Access Levels

Every handler requires an `access` field:

```typescript
export const getUser = handlers.get(
  schemaValidator,
  '/users/:id',
  {
    name: 'Get User',
    summary: 'Get user by ID',
    access: 'protected',     // <-- REQUIRED
    params: { id: string },
    auth: {
      sessionSchema: SHARED_SESSION_SCHEMA,
      jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
      allowedRoles: PLATFORM_VIEWER_ROLES,
    },
    responses: {
      200: UserSchemas.UserResponseSchema,
      404: string,
    },
  },
  async (req, res) => { ... }
);
```

### Access Level Matrix

| Level           | Auth Required | Session                 | Use Case                            |
| --------------- | ------------- | ----------------------- | ----------------------------------- |
| `public`        | No            | None                    | Health checks, docs, public APIs    |
| `authenticated` | Yes           | Basic session           | Any logged-in user                  |
| `protected`     | Yes           | Full session with roles | Role-gated endpoints                |
| `internal`      | HMAC only     | None                    | Service-to-service, CLI-to-platform |

The `access` field is type-narrowed: setting `access: 'public'` makes `auth` optional; `access: 'protected'` requires `auth` with `allowedRoles`.

## Field Encryption

PII/PHI/PCI fields may be automatically encrypted/decrypted by
`ComplianceEventSubscriber` or `EncryptedType` depending on the entity/module:

- **Algorithm**: AES-256-GCM
- **Key derivation**: HKDF-SHA256 with per-tenant salt
- **Format**: `v1:base64(iv):base64(authTag):base64(ciphertext)`
- **Triggered automatically** on persist/load — no manual encryption needed
- **Blocks `nativeInsert`** to prevent bypassing encryption

Configuration:

```typescript
// In registrations.ts
EncryptionService: {
  lifetime: Lifetime.Singleton,
  type: EncryptionService,
  factory: ({ ENCRYPTION_KEY }) => new EncryptionService(ENCRYPTION_KEY),
},
```

## Tenant Isolation

### EntityMgr Factory Pattern

`EntityMgr` in DI returns a factory `(tenantId?: string | null) => EntityManager`. This sets the tenant filter on the forked EM for per-tenant encryption key derivation.

**In registrations.ts:**

```typescript
EntityMgr: {
  lifetime: Lifetime.Scoped,
  type: (tenantId?: string | null) => EntityManager,
  factory: ({ Orm }, _resolve, context) =>
    (tenantId?: string | null) => {
      const em = Orm.em.fork(context?.entityManagerOptions as ForkOptions | undefined);
      if (tenantId) {
        em.setFilterParams('tenant', { tenantId });
      }
      return em;
    },
},
```

**In controllers** — create the tenant-scoped EM via DI context:

```typescript
// Authenticated handlers (have req.session)
const em = emFactory({ context: { tenantId: req.session.organizationId } });
```

**In internal HMAC handlers (NO session, but reading encrypted columns)** —
**DO NOT** call `emFactory()` with no context. There is no JWT, so
`getCurrentTenantId()` returns `''`, MikroORM's `EncryptedType` decrypts with
the wrong key, fails _silently_ in the published framework versions, and
returns the raw `v2:` ciphertext. That ciphertext propagates into ECS task
definitions, deployment manifests, S3 log objects, etc. — extremely hard to
diagnose because the failure is invisible at the read site.

Always resolve the tenant from an entity ID first, then fork a tenant-scoped
EM. Use the helpers in `domain/utils/tenant-em.util.ts`:

```typescript
import {
  getApplicationScopedEm,
  getDeploymentScopedEm,
  getReleaseScopedEm,
  getResourceScopedEm
} from '../../domain/utils/tenant-em.util';

// In an internal handler keyed by application
const scoped = await getApplicationScopedEm(req.params.id);
if (!scoped) {
  res.status(404).send('Application not found');
  return;
}
const { em, application } = scoped;

// All subsequent reads/writes via `em` use the correct tenant key
const envVars = await em.find(EnvironmentVariableEntity, { ... });
```

The helper does an unscoped lookup just to read the unencrypted FK column
(`organizationId`), then forks an EM with that as the tenant context. All
subsequent reads through that EM auto-decrypt encrypted columns correctly,
and all writes encrypt with the same key (so future reads stay symmetric).

**The only valid uses of `emFactory()` with no context** are operations that:

1. Don't touch entities with encrypted columns (no PII/PCI/PHI), AND
2. Don't write to a global filter–scoped table without bypassing the filter via `getSuperAdminContext`.

### Decrypt-Safe Lookup Pattern

When the tenant is not known yet, never hydrate an encrypted entity just to
learn who owns it. This is the failure mode that causes errors like:

- `Failed to decrypt encrypted column value`
- `ciphertext is corrupted or the wrong key was used`

Typical bad pattern:

```typescript
// Wrong: invitation.email / invitation.token may decrypt before authorization
const invitation = await em.findOne(InvitationEntity, { id: invitationId });
```

Preferred pattern:

```typescript
const rows = (await em.getConnection().execute(
  `
    select organization_id as "organizationId"
    from invitation
    where id = ?
    limit 1
  `,
  [invitationId]
)) as Array<{ organizationId: string }>;

const organizationId = rows[0]?.organizationId;
if (!organizationId) return null;

const scopedEm = emFactory({ context: { tenantId: organizationId } });
const invitation = await withEncryptionContext(organizationId, () =>
  scopedEm.findOne(InvitationEntity, { id: invitationId })
);
```

Apply the same rule to `UserEntity`, `OrganizationEntity`, GitHub
installation-like records, and any other entity that mixes:

- unencrypted ownership columns (`organization_id`, `application_id`)
- encrypted payload columns (`email`, `token`, `accountLogin`, etc.)

If the read is only needed for authorization or routing, prefer raw FK lookup or
lookup-hash columns over entity hydration.

If in doubt, scope it.

### Tenant-Encrypted IAM/Auth Surface Pattern

IAM is the easiest place to accidentally hydrate encrypted PII too early:
login, invite, resend, `/me`, organization switching, and internal HMAC
endpoints all start with only a user ID, email, token, or invitation ID. In
these flows, treat encrypted entities as **two-phase loads**:

1. Resolve ownership using only unencrypted columns or lookup hashes.
2. Create an EM with the resolved tenant context.
3. Hydrate encrypted entities only inside that tenant context, or return a
   narrow raw snapshot if hydration is not needed.

Do not do this:

```typescript
// Wrong: hydration decrypts email/token/name before tenant context is known.
const user = await em.findOne(UserEntity, { id: userId }, {
  populate: ['organization', 'roles', 'roles.permissions']
});
const invitation = await em.findOne(InvitationEntity, { email });
```

Do this instead:

```typescript
// 1. Raw ownership lookup: select only unencrypted FK/status columns.
const rows = (await em.getConnection().execute(
  `
    select organization_id as "organizationId"
    from "user"
    where id = ?
    limit 1
  `,
  [userId]
)) as Array<{ organizationId: string | null }>;

const organizationId = rows[0]?.organizationId;
if (!organizationId) return null;

// 2. Scope both tenant filter and encryption context before hydration.
const scopedEm = emFactory({ context: { tenantId: organizationId } });
const user = await withEncryptionContext(organizationId, () =>
  scopedEm.findOne(UserEntity, { id: userId })
);
```

For lookup by encrypted value (`email`, `token`, account login, invite token),
do **not** query the encrypted column. Store and query a parallel
`.compliance('none')` lookup column, usually a normalized hash:

```typescript
const normalizedEmail = normalizeInvitationEmail(email);
const rows = await em.getConnection().execute(
  `
    select id, organization_id as "organizationId"
    from "invitation"
    where email_lookup_hash = ?
      and organization_id = ?
      and status in (?, ?)
  `,
  [
    invitationEmailLookupHash(normalizedEmail),
    organizationId,
    InvitationStatus.PENDING,
    InvitationStatus.ACCEPTED
  ]
);
```

When implementing `/me`, JWT payload construction, organization switching, or
other auth-surface reads, prefer helper functions that return a narrow snapshot
or authorization surface (`roles`, `permissions`, `organizationId`) without
hydrating unrelated encrypted fields. In IAM, that means using helpers like:

- `resolveUserOrganizationIdById` for raw user → organization lookup
- `findUserByIdWithTenantContext` when full user hydration is required
- `getUserAuthorizationSurfaceWithTenantContext` for roles/permissions
- `findOrganizationByIdWithTenantContext` for encrypted organization fields

Behavior flags must not depend solely on legacy encrypted or metadata state. For
example, invited-user onboarding should derive `wasInvited` from multiple
non-PII signals:

- `user.metadata.wasInvited === true`
- active `OrganizationUser.invitedBy`
- pending/accepted `Invitation` matched by `email_lookup_hash` + organization

This prevents legacy users and accepted invites from being treated as brand-new
organizations just because one metadata write was missed.

For legacy mixed-encryption data, repair in a dedicated helper or migration:
try explicit candidate tenant contexts (`organizationId`, legacy `''`) with
`FieldEncryptor`, then rewrite under the canonical tenant context. Do not
scatter "try any key and continue" logic through controllers, and never return
raw `v1:`/`v2:` ciphertext to clients as a fallback.

**Symmetry rule for encryption**: Whatever tenant ID was used at write time
must be used at read time. Asymmetric tenants → silent decryption failure →
ciphertext returned as-is → corrupted data downstream. This applies to:

- DB columns (`EncryptedType` via `getCurrentTenantId()`)
- Redis cache values (`TtlCache` `compliance` arg)
- S3 object bodies (`ObjectStore` `compliance` arg)

### Raw SQL (Kysely) bypasses `EncryptedType`

`EncryptedType.convertToJSValue` is wired up by MikroORM's entity hydration
pipeline. **It does not run when you reach past MikroORM to query via raw
SQL** — e.g. `(em as unknown as SqlEntityManager).getKysely<Database>()`.
A Kysely query that selects a `.compliance('pii' | 'phi' | 'pci')` column
returns the raw `v2:` ciphertext straight through to the caller, and the
failure is completely silent: the frontend will render `v2:<iv>:<tag>:…`
instead of the plaintext.

The same bypass applies to **WHERE clauses**: `LOWER(email) = 'alice@x.com'`
compares the plaintext to ciphertext, never matches, and silently returns
zero rows. This quietly breaks rate-limiting, duplicate checks, search, etc.
built against encrypted columns.

**Rule**: anywhere you call `.getKysely()` and touch an encrypted column,
either:

1. **Decrypt after SELECT** — apply a helper to each PII column in the
   row-mapping step:

   ```typescript
   // services/internal/kyselyEncryption.ts
   import { FieldEncryptor } from '@forklaunch/core/persistence';

   const encryptor = new FieldEncryptor(
     process.env.ENCRYPTION_KEY
   );

   export function decryptEncryptedColumn(
     value: string | null | undefined,
     tenantId: string
   ): string | null {
     if (value == null) return null;
     if (!/^v[12]:/.test(value)) return value;
     try { return encryptor.decrypt(value, tenantId); }
     catch { return value; }
   }

   // In the service: always thread the request's tenant ID through —
   // ciphertext was produced under HKDF(masterKey, info=tenantId), so reads
   // must use the same tenantId or decryption silently no-ops.
   const tenantId = req.session.organizationId;
   const rows = await db.selectFrom('policies').select([...]).execute();
   return rows.map(r => ({
     ...r,
     namedInsured: decryptEncryptedColumn(r.named_insured, tenantId),
     insuredPropertyAddress: decryptEncryptedColumn(r.insured_property_address, tenantId),
   }));
   ```

2. **Encrypt before WHERE** — for equality lookups (`FieldEncryptor` uses a
   deterministic IV, so same plaintext → same ciphertext):

   ```typescript
   const tenantId = req.session.organizationId;
   const encEmail = encryptor.encrypt(email, tenantId);
   db.selectFrom('external_analysis_requests')
     .where('email', '=', encEmail)
     .select(...)
   ```

3. **Don't filter on encrypted columns with `ILIKE` / `LIKE` / `LOWER()`** —
   ciphertext is opaque. If you need partial-match search or case-insensitive
   equality, store a parallel `.compliance('none')` token (hashed prefix,
   domain extract, canonical lowercased copy) alongside the encrypted column
   and query that instead. Backfill the token in a migration.

**Tenant ID must match writes.** The helper above takes `tenantId` as a
required parameter — never default it to `''`. Ciphertext is produced under
HKDF(masterKey, info=tenantId); reading with the wrong (or empty) tenant
silently fails the AES-GCM auth-tag check, the `catch` branch returns the
raw ciphertext, and the caller hands PII-shaped strings back to the client.
Always thread the request's `organizationId` through to every
`decryptEncryptedColumn` and `encryptor.encrypt` call in the same code path
that the EM-scoped writes used.

The framework cannot statically detect Kysely access to encrypted columns —
this is a manual-vigilance rule. When adding `.compliance('pii')` to a
field, grep for `.getKysely()` and `selectFrom('<table>')` to find call
sites that need updating.

**In service DI factories** — services receive plain `EntityManager`:

```typescript
MyService: {
  factory: ({ EntityMgr, OtelCollector }) =>
    new MyService(EntityMgr, OtelCollector)
},
```

**Services receive plain `EntityManager`** — no factories, no tenant awareness. The tenant context is set on the EM by the DI framework when `scopedResolver` is called with `{ context: { tenantId } }`. For service methods that need a tenant-scoped EM passed from the controller, take it as a method parameter.

**In server.ts** — register the tenant filter at bootstrap:

```typescript
import { setupTenantFilter } from "@forklaunch/core/persistence";
const orm = ci.resolve(tokens.Orm);
setupTenantFilter(orm);
```

**In seeders:**

In seeders, write `organizationId` directly into each entity. The tenant filter is only registered in `server.ts` and does not run during seeding. Do NOT call `em.setFilterParams('tenant', ...)` in seeder contexts.

### MikroORM Global Filter

The `setupTenantFilter(orm)` call registers a global filter that adds `WHERE organizationId = :tenantId` to all queries on entities that have an `organizationId` property. The filter is set per-request via `em.setFilterParams('tenant', { tenantId })`.

To bypass for super-admin operations:

```typescript
import { getSuperAdminContext } from "@forklaunch/core/persistence";
const em = getSuperAdminContext(baseEm);
```

### PostgreSQL RLS (optional, configurable)

```typescript
// At bootstrap after setupTenantFilter:
import { setupRls } from "@forklaunch/core/persistence";
setupRls(orm); // Sets SET LOCAL app.tenant_id per transaction
```

Opt out by passing `{ enabled: false }` to `setupRls`.

## Data Retention (Framework)

Declare a retention policy on any compliance entity:

```typescript
export const Account = defineComplianceEntity({
  name: 'Account',
  retention: {
    duration: RetentionDuration.years(7), // ISO 8601 → 'P7Y'
    action: 'anonymize'                    // or 'delete'
  },
  properties: { ... },
});
```

### Duration Helpers

- `RetentionDuration.years(3)` → `'P3Y'`
- `RetentionDuration.months(6)` → `'P6M'`
- `RetentionDuration.days(90)` → `'P90D'`

Minimum duration is 1 day. Invalid or sub-day durations are rejected at boot.

### Actions

- `'delete'` — hard-deletes expired records
- `'anonymize'` — nulls PII/PHI/PCI fields, keeps 'none' fields, sets `retentionAnonymizedAt`

### Enforcement

`RetentionService` runs as a daily scheduled ECS RunTask via `scripts/enforce-retention.ts`:

- Batches of 1000 with fresh EM per batch
- Calendar-aware cutoff computation (handles leap years, month-ends)
- Idempotent — skips already-anonymized records
- Checks field nullability before anonymizing — skips non-nullable PII fields with warning
- Per-entity error isolation
- Dry-run mode: `pnpm retention:enforce -- --dry-run`
- Registered in DI as Singleton: `new RetentionService(Orm, OtelCollector)`

### Audit

Entities with PII but no retention policy are flagged as medium-severity findings in the CLI audit and platform risk scoring.

## CLI Audit Command

Generate a compliance audit report:

```bash
# Local-only (pretty terminal output)
forklaunch compliance audit

# Upload to platform for risk scoring, data flow diagrams, and DPIA
forklaunch compliance audit -e production

# Save to file
forklaunch compliance audit -e production -o report.json

# Selective output
forklaunch compliance audit -e staging --risk-score
forklaunch compliance audit -e staging --data-flow
forklaunch compliance audit -e staging --dpia

# Raw JSON
forklaunch compliance audit --json
```

### Requirements for Platform Upload

1. `platform_application_id` must be set in `manifest.toml`
2. `FORKLAUNCH_HMAC_SECRET` environment variable must be set
3. `--environment` / `-e` flag must specify the target environment

### What the Audit Collects

- **Entities**: Field names, compliance classifications, encryption status (from `manifest.toml [compliance]` section)
- **Routes**: Paths, methods, access levels (from OpenAPI specs)
- **Secrets**: Declared secret names and count
- **Data residency**: Allowed deployment regions

### Platform Processing

When uploaded, the platform computes:

- **Risk score** (0-100, normalized over total items)
- **Findings** (severity: critical/high/medium/low with point values)
- **PCI Data Flow Diagram** (Mermaid format)
- **DPIA** (GDPR Data Protection Impact Assessment with data inventory, mitigations, cross-border analysis)

## Portal Compliance Page

The compliance reports page is at `/dashboard/applications/:id/compliance`:

- Environment selector
- Risk score overview (score, level, findings count)
- Findings table (sorted by severity)
- Data Flow tab (Mermaid diagram for PCI fields)
- DPIA tab (data inventory, risk assessment, active mitigations, cross-border transfers)
- History tab (previous reports with scores)

Requires the `COMPLIANCE` feature flag (gated by billing plan).

## GDPR Erasure and Export

### ComplianceDataService (Framework)

Every service with a database gets a `ComplianceDataService` that walks all compliance-registered entities and erases or exports PII data for a user:

```typescript
// In registrations.ts
ComplianceDataService: {
  lifetime: Lifetime.Singleton,
  type: ComplianceDataService,
  factory: ({ Orm, OtelCollector }) =>
    new ComplianceDataService(Orm, OtelCollector, {
      // Optional: override userIdField per entity
      User: 'id',              // User entity links via primary key
      Subscription: 'partyId', // billing uses partyId
    })
},
```

### userIdField Resolution

When no override is specified for an entity, the service **optimistically searches** the entity's properties for common user-linking field names in order:

1. **Constructor override** — `{ Subscription: 'partyId' }` — exact, skips search
2. **Entity declaration** — `defineComplianceEntity({ userIdField: 'id' })` — exact
3. **Optimistic search** — tries: `userId`, `user`, `id`, `partyId`, `customerId`, `ownerId`, `createdBy`, `email` — first match wins
4. **Skip** — no candidate found, entity is skipped with a warning

If you pass overrides in the constructor, those entities use the exact field. Entities NOT in the overrides map still fall through to optimistic search.

### Per-Service Compliance Endpoints

Every service with a database gets HMAC-authenticated compliance endpoints:

```typescript
DELETE /compliance/erase/:userId  // erases PII from this service's entities
GET    /compliance/export/:userId // exports PII from this service's entities
```

These are auto-generated by the CLI template. Each service handles only its own entities — no cross-service coupling.

### Client-SDK Fan-Out

The `client-sdk` provides `createComplianceClient` that fans out to all services in parallel:

```typescript
import { createComplianceClient } from "@myapp/client-sdk";

const compliance = createComplianceClient({
  hmacSecretKey: HMAC_SECRET_KEY,
  services: {
    iam: iamSdk,
    billing: billingSdk,
    myService: myServiceSdk,
  },
});

// From frontend or CLI:
const result = await compliance.erase("user-123");
// {
//   iam: { status: 'fulfilled', result: { entitiesAffected: ['User'], recordsDeleted: 1 } },
//   billing: { status: 'fulfilled', result: { entitiesAffected: ['Subscription'], recordsDeleted: 2 } },
//   myService: { status: 'rejected', error: 'Connection refused' }
// }

const exported = await compliance.export("user-123");
```

- Uses `Promise.allSettled` — partial failures don't block other services
- Per-service status: `'fulfilled'` with result or `'rejected'` with error
- Any SDK with `compliance.eraseUserData` / `compliance.exportUserData` qualifies
- Lives in **client-sdk** (not framework) because it knows what services exist

## Secrets Management

```typescript
// In forklaunch.manifest.toml
[compliance];
secrets = ["DATABASE_URL", "ENCRYPTION_KEY", "STRIPE_SECRET_KEY"];

// In code — boot-time validated, typed access
import { getSecret } from "@forklaunch/core/secrets";
const dbUrl = getSecret("DATABASE_URL"); // throws at boot if missing
```

## Data Residency

```toml
# In forklaunch.manifest.toml
[compliance]
data_residency = ["us-east-1", "eu-west-1"]
```

Enforced at deploy time by the platform — deployments to unauthorized regions are rejected.

## Platform API Endpoints

All under the `/compliance` router prefix:

| Method | Path                                                        | Auth | Description                          |
| ------ | ----------------------------------------------------------- | ---- | ------------------------------------ |
| GET    | `/features`                                                 | JWT  | List compliance features             |
| GET    | `/standards`                                                | JWT  | List compliance standards            |
| GET    | `/applications/:appId/environments/:envName`                | JWT  | Get environment compliance config    |
| PUT    | `/applications/:appId/environments/:envName`                | JWT  | Update environment compliance config |
| GET    | `/applications/:appId/environments/:envName/audit/internal` | JWT  | Get internal audit logs              |
| GET    | `/applications/:appId/environments/:envName/audit/aws`      | JWT  | Get AWS CloudTrail logs              |
| POST   | `/applications/:appId/environments/:envName/audit/report`   | HMAC | Submit audit report from CLI         |
| GET    | `/applications/:appId/environments/:envName/audit/reports`  | JWT  | List historical audit reports        |
| POST   | `/retention/enforce`                                        | HMAC | Trigger retention enforcement        |

### Per-Service Compliance Endpoints (generated on every service)

| Method | Path                         | Auth | Description                             |
| ------ | ---------------------------- | ---- | --------------------------------------- |
| DELETE | `/compliance/erase/:userId`  | HMAC | Erase PII for a user from this service  |
| GET    | `/compliance/export/:userId` | HMAC | Export PII for a user from this service |

## Supply Chain Monitoring

The CLI generates Dependabot configuration in the user's target repo:

```bash
forklaunch init myapp  # generates .github/dependabot.yml
```

This is a warning-only default. The configuration is also updated by any CLI command that touches GitHub configuration paths (sync, change).

## Change Management

The CLI generates GitHub branch protection documentation and CI workflows:

```bash
forklaunch init myapp  # generates .github/workflows/ci.yml and .github/BRANCH_PROTECTION.md
```

The CI workflow runs lint, type-check, and tests on PRs. Branch protection rules must be configured manually via the setup guide.
