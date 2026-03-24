# Plan: ForkLaunch Compliance, Security & Realtime Enforcement Framework

## Overview

ForkLaunch is being upgraded from a typed API framework to a **compliance-enforced runtime** where security, audit, tenant isolation, and data classification are architecturally impossible to skip. This is a major version bump covering 10 features: automatic audit logging via OTEL→Loki, field-level encryption (PHI/PCI only) via AES-256-GCM with HKDF-derived tenant keys, tenant isolation via MikroORM global filter + configurable PostgreSQL RLS, mandatory route access declarations (`'public' | 'authenticated' | 'protected' | 'internal'`), tenant-scoped WebSocket channels with permission-checked broadcast, Redis-backed rate limiting, typed secrets accessor with boot-time validation, data residency constraints in the manifest (compiler-enforced), guided CLI field classification, and compliance audit report generation.

**Scope mode: HOLD SCOPE.** All 10 features ship in one major version. 8 are framework runtime features; 2 are CLI/compiler features (data residency, guided data model setup).

## Scope Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Audit log store | OTEL → Loki | Already have OTEL infra; Loki is append-only; no new DB |
| Encryption scope | PHI/PCI only, not PII | RDS encryption + TLS sufficient for PII; field encryption for highest-sensitivity only |
| Encryption keys | HKDF from master env key | One secret to manage; compiler provides master key |
| Tenant isolation | MikroORM filter (mandatory) + PostgreSQL RLS (configurable opt-out) | Defense in depth; RLS only on PG; filter works on all DBs |
| RBAC model | `access: 'public' \| 'authenticated' \| 'protected' \| 'internal'` | Type-narrows existing `auth` field; `'protected'` requires RBAC declaration |
| Auth config | Stays per-route; `access` type-narrows `auth` | Preserves full SDK type safety (AuthHeaders, AuthCollapse, LiveTypeFunctionRequestInit unchanged); shared constants for DRY |
| Entity properties | All fields use `fp`; `.compliance()` required on every field | Compile-time enforcement via phantom types; `defineComplianceEntity` only accepts `ClassifiedProperty` |
| WS channels | Tenant-scoped with per-broadcast permission check | Maximum granularity; every broadcast verifies recipient permissions |
| WS re-validation | Configurable interval (default 5min) | Balance between security and performance |
| Rate limit Redis failure | Fail open with warning | Redis outage shouldn't cascade to service outage |
| Native query blocking | Block nativeInsert/Update/Delete on compliance entities | Prevents encryption bypass; enforcement via code |
| RLS + transactions | Implicit transaction wrapper on em.flush() with SET LOCAL | Developer doesn't need explicit em.transactional() |
| Rollout | Major version bump, all enforced at once | Clean cut; no half-enforcement states |

## Architecture

```
+============================================================================+
|                         FORKLAUNCH FRAMEWORK v2                             |
+============================================================================+
|                                                                            |
|  +--- APPLICATION LAYER (framework/express, framework/hyper-express) ----+ |
|  |                                                                       | |
|  |  ForklaunchApplication                                                | |
|  |    - complianceConfig: { rls: boolean, encryption: { masterKey } }    | |
|  |    - rateLimitConfig: { read: N, write: N, windowMs: N }             | |
|  |                                                                       | |
|  +---+-------------------------------------------------------------------+ |
|      |                                                                     |
|      | registers routes; validates access field at startup                 |
|      v                                                                     |
|  +--- HTTP MIDDLEWARE PIPELINE (framework/core/src/http/middleware) ------+ |
|  |                                                                       | |
|  |  REQUEST -->                                                          | |
|  |    1. [RateLimiter]  <-- Redis (INCR + EXPIRE)                       | |
|  |    2. [Auth]         <-- derived from route's access + auth fields   | |
|  |    3. [TenantCtx]    <-- sets organizationId on EM, SET LOCAL (PG)   | |
|  |    4. [Handler]      <-- business logic                              | |
|  |    5. [AuditLogger]  --> OTEL (log.type: audit) --> Loki             | |
|  |  <-- RESPONSE                                                        | |
|  |                                                                       | |
|  +-----------------------------------------------------------------------+ |
|                                                                            |
|  +--- PERSISTENCE LAYER (framework/core/src/persistence) ----------------+ |
|  |                                                                       | |
|  |  fp (Proxy over MikroORM p)                                          | |
|  |    fp.string() --> UnclassifiedProperty (NOT usable in entity)       | |
|  |    fp.string().compliance('pii') --> ClassifiedProperty (usable)     | |
|  |                                                                       | |
|  |  defineComplianceEntity (wrapper over MikroORM defineEntity)         | |
|  |    properties field only accepts ClassifiedProperty                   | |
|  |    compile-time error if .compliance() missing                        | |
|  |                                                                       | |
|  |  ComplianceEventSubscriber                                            | |
|  |    onBeforeCreate/Update --> encrypt phi/pci fields (AES-256-GCM)    | |
|  |    onAfterLoad           --> decrypt phi/pci fields                   | |
|  |    blocks nativeInsert on compliance entities                         | |
|  |                                                                       | |
|  |  TenantFilter (MikroORM global filter)                               | |
|  |    WHERE organization_id = :tenantId  (always enabled)               | |
|  |                                                                       | |
|  |  RLS (PostgreSQL only, configurable opt-out)                         | |
|  |    SET LOCAL app.tenant_id = :tenantId  (per transaction)            | |
|  |                                                                       | |
|  +-----------------------------------------------------------------------+ |
|                                                                            |
|  +--- SECRETS LAYER (framework/core/src/secrets) ------------------------+ |
|  |                                                                       | |
|  |  SecretsAccessor                                                      | |
|  |    - validateAtBoot(manifest.secrets)                                 | |
|  |    - getSecret(key) --> env var (throws if undeclared)               | |
|  |    - .secrets.local for local dev                                    | |
|  |                                                                       | |
|  +-----------------------------------------------------------------------+ |
|                                                                            |
|  +--- WEBSOCKET LAYER (framework/ws) ------------------------------------+ |
|  |                                                                       | |
|  |  ForklaunchWebSocketServer                                            | |
|  |    - authenticate(req) --> session | reject (close 4001)             | |
|  |    - channels: tenant-scoped, permission-checked                     | |
|  |    - periodic re-validation (configurable interval)                  | |
|  |    - all events --> AuditLogger                                      | |
|  |                                                                       | |
|  +-----------------------------------------------------------------------+ |
|                                                                            |
|  +--- ENCRYPTION ENGINE (framework/core/src/encryption) -----------------+ |
|  |                                                                       | |
|  |  FieldEncryptor                                                       | |
|  |    - masterKey: from env var (provided by compiler)                  | |
|  |    - deriveKey(tenantId): HKDF(masterKey, tenantId)                  | |
|  |    - encrypt(plaintext, tenantId): AES-256-GCM                      | |
|  |    - decrypt(ciphertext, tenantId): AES-256-GCM                     | |
|  |    - ciphertext format: version:iv:authTag:encrypted                 | |
|  |                                                                       | |
|  +-----------------------------------------------------------------------+ |
|                                                                            |
+============================================================================+
|                      EXTERNAL SERVICES                                      |
|  +----------+  +-----------+  +--------+  +------+                         |
|  | PostgreSQL|  | Redis     |  | OTEL   |  | Loki |                         |
|  | (+ RLS)   |  | (rate     |  | Collec |  | (aud |                         |
|  |           |  |  limits)  |  | tor)   |  | logs)|                         |
|  +----------+  +-----------+  +--------+  +------+                         |
|                                                                            |
+============================================================================+
|                      CLI (Rust)                                             |
|  +-------------------------+  +-----------------------------+               |
|  | forklaunch init entity  |  | forklaunch compliance audit |               |
|  |   --> classify fields   |  |   --> JSON/PDF report       |               |
|  |   --> generate fp defs  |  |   --> routes, fields, keys  |               |
|  +-------------------------+  +-----------------------------+               |
|                                                                            |
|  +-------------------------+                                               |
|  | manifest.toml           |                                               |
|  |   data_residency: ...   |                                               |
|  |   secrets: [...]        |                                               |
|  |   compliance: {...}     |                                               |
|  +-------------------------+                                               |
+============================================================================+
```

## Implementation Steps

### Step 1: Compliance Types & `fp` Property Builder & `defineComplianceEntity`

**Files to create:**
- `framework/core/src/persistence/complianceTypes.ts`
- `framework/core/src/persistence/compliancePropertyBuilder.ts`
- `framework/core/src/persistence/defineComplianceEntity.ts`

**Files to modify:**
- `framework/core/src/persistence/index.ts` — re-export `fp`, `defineComplianceEntity`, compliance types

**What the code should do:**

1. Define the compliance classification type:
   ```typescript
   const ComplianceLevel = {
     pii: 'pii',
     phi: 'phi',
     pci: 'pci',
     none: 'none'
   } as const;
   type ComplianceLevel = (typeof ComplianceLevel)[keyof typeof ComplianceLevel];
   ```

2. Define phantom-typed property wrappers that enforce `.compliance()` at compile time:
   ```typescript
   declare const CLASSIFIED: unique symbol;

   // fp.string() returns this — NOT assignable to defineComplianceEntity properties
   type UnclassifiedProperty<T> = {
     compliance(level: ComplianceLevel): ClassifiedProperty<T>;
     nullable(): UnclassifiedProperty<T | null>;
     primary(): UnclassifiedProperty<T>;
     unique(): UnclassifiedProperty<T>;
     onCreate(fn: () => T): UnclassifiedProperty<T>;
     onUpdate(fn: () => T): UnclassifiedProperty<T>;
     // ... Proxy forwards all MikroORM PropertyBuilder methods
   };

   // fp.string().compliance('pii') returns this — IS assignable
   type ClassifiedProperty<T> = {
     readonly [CLASSIFIED]: true;
     nullable(): ClassifiedProperty<T | null>;
     primary(): ClassifiedProperty<T>;
     unique(): ClassifiedProperty<T>;
     // compliance() NOT available (already called)
   };
   ```

3. Implement `fp` as a Proxy over MikroORM's `p`. When any method on `p` is called (e.g., `fp.string()`), return a Proxy-wrapped `UnclassifiedProperty` that forwards all MikroORM PropertyBuilder methods and adds `.compliance()`. Calling `.compliance(level)` stores the level in a `WeakMap<PropertyBuilder, ComplianceLevel>` and returns a `ClassifiedProperty` Proxy that propagates the compliance level through subsequent chains (e.g., `.nullable()`, `.primary()`). The Proxy auto-forwards all future MikroORM property methods without maintenance.

4. Implement `defineComplianceEntity`:
   ```typescript
   type ComplianceProperties<T> = {
     [K in keyof T]: ClassifiedProperty<T[K]> | (() => ClassifiedProperty<T[K]>);
   };

   function defineComplianceEntity<T>(options: {
     name: string;
     properties: ComplianceProperties<T>;
   }): EntitySchema<T> {
     // 1. Extract compliance metadata from each ClassifiedProperty via WeakMap
     // 2. Store in module-level registry: Map<entityName, Map<fieldName, ComplianceLevel>>
     // 3. Strip compliance wrapper, pass raw MikroORM PropertyBuilder to real defineEntity()
     // 4. Return the EntitySchema
   }
   ```

5. Export a `getComplianceMetadata(entityName: string, fieldName: string): ComplianceLevel` function that reads from the registry. Used by EventSubscriber and AuditLogger.

**Dependencies:** None (foundational)

**Test requirements:**
- Compile-time: `fp.string()` is NOT assignable to `ClassifiedProperty` (`// @ts-expect-error` test)
- Compile-time: `fp.string().compliance('pii')` IS assignable to `ClassifiedProperty`
- Compile-time: `defineComplianceEntity({ properties: { x: fp.string() } })` errors
- Compile-time: `defineComplianceEntity({ properties: { x: fp.string().compliance('none') } })` succeeds
- Chaining order: `fp.string().nullable().compliance('phi')` and `fp.string().compliance('phi').nullable()` both work
- All `p` methods are proxied: string, uuid, enum, json, datetime, integer, boolean, oneToMany, manyToOne, manyToMany
- `getComplianceMetadata` returns correct classification after `defineComplianceEntity`
- Entity defined with `defineComplianceEntity` creates a valid MikroORM entity (can be used with `em.create`, `em.find`)

---

### Step 2: `access` Field — Type-Narrows Existing `auth`

**Files to modify:**
- `framework/core/src/http/types/contractDetails.types.ts` — Add `access` to base contract types; constrain `Auth` generic via discriminated union on `access`
- `framework/core/src/http/router/expressLikeRouter.ts` — Add startup validation

**Files NOT modified (SDK types unchanged):**
- `framework/core/src/http/types/apiDefinition.types.ts` — `AuthHeaders`, `AuthCollapse`, `LiveTypeFunctionRequestInit`, `LiveTypeFunction`, `LiveSdkFunction` all unchanged
- `framework/core/src/http/types/sdk.types.ts` — `MapHandlerToLiveSdk` reads `contractDetails.auth` as before
- `framework/core/src/http/types/expressLikeRouter.types.ts` — unchanged
- `framework/universal-sdk/` — unchanged

**What the code should do:**

1. Define access level type:
   ```typescript
   type AccessLevel = 'public' | 'authenticated' | 'protected' | 'internal';
   ```

2. Define constrained auth types based on access level, using EXISTING types (`JwtAuthMethods` line 318, `BasicAuthMethods` line 309, `HmacMethods` line 333, `PermissionSet` line 361, `RoleSet` line 367, `TokenOptions` line 343, `DecodeResource` line 348):

   ```typescript
   // Authenticated: JWT or Basic, RBAC optional
   type AuthenticatedAuth<SV, ...> = TokenOptions
     & { readonly decodeResource?: DecodeResource }
     & (BasicAuthMethods | JwtAuthMethods)
     & { readonly sessionSchema?: SessionObject<SV>;
         readonly requiredScope?: string;
         readonly scopeHeirarchy?: string[];
         readonly requiredFeatures?: string[];
         readonly requireActiveSubscription?: boolean;
         // surfacePermissions/surfaceRoles/surfaceScopes allowed
       };

   // Protected: JWT or Basic, RBAC REQUIRED
   type ProtectedAuth<SV, ...> = TokenOptions
     & { readonly decodeResource?: DecodeResource }
     & (BasicAuthMethods | JwtAuthMethods)
     & (PermissionSet | RoleSet | { readonly requiredScope: string })
     & { readonly sessionSchema?: SessionObject<SV>;
         readonly scopeHeirarchy?: string[];
         readonly requiredFeatures?: string[];
         readonly requireActiveSubscription?: boolean;
       };

   // Internal: HMAC only
   type InternalAuth = TokenOptions & HmacMethods;
   ```

3. Replace the `auth?: Auth` field on `PathParamHttpContractDetails` and `HttpContractDetails` with a discriminated union on `access`:
   ```typescript
   ... & (
     | { readonly access: 'public'; readonly auth?: never }
     | { readonly access: 'authenticated'; readonly auth: AuthenticatedAuth<SV, ...> }
     | { readonly access: 'protected'; readonly auth: ProtectedAuth<SV, ...> }
     | { readonly access: 'internal'; readonly auth: InternalAuth }
   );
   ```

4. Startup validation in `expressLikeRouter.ts` as runtime safety net, using existing guards (`hasPermissionChecks`, `hasRoleChecks`, `hasScopeChecks` from `framework/core/src/http/guards/`, `isHmacMethod` from `framework/core/src/http/guards/isHmacMethod.ts`):
   ```typescript
   if (contractDetails.access === 'protected') {
     if (!hasPermissionChecks(contractDetails.auth)
       && !hasRoleChecks(contractDetails.auth)
       && !hasScopeChecks(contractDetails.auth)) {
       throw new Error(`Route ${method} ${path}: 'protected' requires roles, permissions, or scope`);
     }
   }
   if (contractDetails.access === 'internal' && !isHmacMethod(contractDetails.auth)) {
     throw new Error(`Route ${method} ${path}: 'internal' requires HMAC auth`);
   }
   if (contractDetails.access === 'public' && contractDetails.auth) {
     throw new Error(`Route ${method} ${path}: 'public' cannot have auth`);
   }
   ```

**Why SDK types are unchanged:** `contractDetails.auth` still exists as a typed field. `MapHandlerToLiveSdk` reads it. `AuthHeaders<Auth>` computes the correct header format from `Auth.jwt`/`Auth.hmac`/`Auth.basic`. `AuthCollapse<Auth>` returns true when `auth` is `never` (public routes). The `access` field only constrains WHAT `auth` can be — it doesn't change the shape SDK types consume.

**SDK type flow (unchanged):**
```
contractDetails.auth (typed per-route)
  → MapHandlerToLiveSdk extracts Auth
    → LiveSdkFunction<..., Auth>
      → LiveTypeFunctionRequestInit<..., Auth>
        → AuthHeaders<Auth> computes header format
        → AuthCollapse<Auth> determines if optional
          → SDK function requires/omits auth headers correctly
```

**Dependencies:** None (parallel with Step 1)

**Test requirements:**
- Compile-time: `{ access: 'protected', auth: { jwt: {...} } }` errors (missing RBAC)
- Compile-time: `{ access: 'public', auth: { jwt: {...} } }` errors (auth not allowed)
- Compile-time: `{ access: 'internal', auth: { jwt: {...} } }` errors (must be HMAC)
- Compile-time: `{ access: 'protected', auth: { jwt: {...}, allowedRoles: new Set(['admin']) } }` succeeds
- Compile-time: `{ access: 'authenticated', auth: { jwt: {...} } }` succeeds (no RBAC needed)
- Compile-time: `{ access: 'internal', auth: { hmac: { secretKeys: {...} } } }` succeeds
- Runtime: startup rejects route without `access` field
- Runtime: startup rejects protected without RBAC
- SDK: existing SDK tests pass unchanged
- SDK: protected route SDK function requires `{ headers: { authorization: 'Bearer ...' } }`
- SDK: public route SDK function does NOT require headers
- SDK: internal route SDK function requires HMAC authorization header

---

### Step 3: Encryption Engine

**Files to create:**
- `framework/core/src/encryption/fieldEncryptor.ts`
- `framework/core/src/encryption/index.ts`

**What the code should do:**

1. `FieldEncryptor` class with constructor taking `masterKey: string`:
   - `deriveKey(tenantId: string): Buffer` — HKDF-SHA256 with masterKey as input key material and tenantId as info context. Returns 32-byte derived key. Uses Node.js `crypto.hkdfSync`.
   - `encrypt(plaintext: string, tenantId: string): string` — Generate random 12-byte IV via `crypto.randomBytes(12)`, derive key, AES-256-GCM encrypt, return `v1:${base64(iv)}:${base64(authTag)}:${base64(ciphertext)}`.
   - `decrypt(ciphertext: string, tenantId: string): string` — Parse version prefix, extract IV/authTag/ciphertext from base64 segments, derive key, AES-256-GCM decrypt. If version unknown or decryption fails, throw `DecryptionError`.
   - All crypto via Node.js `crypto` module (no external dependencies).

2. Error types:
   - `MissingEncryptionKeyError` — master key not in env
   - `DecryptionError` — ciphertext corrupted or wrong key
   - `EncryptionRequiredError` — attempt to persist compliance field without encryption

**Dependencies:** None

**Test requirements:**
- Encrypt/decrypt roundtrip produces original value
- Different tenants produce different ciphertext for same plaintext
- Same tenant + same plaintext produces different ciphertext (random IV)
- Tampered ciphertext throws `DecryptionError`
- Wrong tenant ID for decryption throws `DecryptionError`
- Missing master key throws `MissingEncryptionKeyError`
- Null input → null output; empty string → encrypted empty string
- Ciphertext format starts with `v1:` prefix

---

### Step 4: Compliance EventSubscriber

**Files to create:**
- `framework/core/src/persistence/complianceEventSubscriber.ts`

**Files to modify:**
- `framework/core/src/persistence/index.ts` — export subscriber

**What the code should do:**

1. MikroORM v7 `EventSubscriber` implementation:
   - `onBeforeCreate(args)` and `onBeforeUpdate(args)`: For each field on the entity, call `getComplianceMetadata(entityName, fieldName)`. If compliance is `'phi'` or `'pci'` and the field value is not null, encrypt using `FieldEncryptor.encrypt(value, tenantId)`. The `tenantId` comes from the EntityManager's filter parameters (set by tenant context middleware). If master key unavailable, throw `EncryptionRequiredError` (abort persist). If compliance is `'pii'` or `'none'`, do nothing.
   - `onAfterLoad(args)`: For each field with compliance `'phi'` or `'pci'`, check if the value starts with `v1:`. If so, decrypt using `FieldEncryptor.decrypt(value, tenantId)`. If no `v1:` prefix, treat as pre-migration plaintext (log warning, return as-is — supports rolling deployment). If decryption fails, throw `DecryptionError`.

2. Native query interception: Wrap `EntityManager.nativeInsert`, `nativeUpdate`, `nativeDelete` to check if the target entity has any compliance fields with level `'phi'` or `'pci'`. If so, throw `EncryptionRequiredError("nativeInsert blocked on compliance entity '{entityName}'. Use em.create() + em.flush() instead.")`.

**Dependencies:** Step 1 (compliance types + registry), Step 3 (encryption engine)

**Test requirements:**
- Entity with `compliance('phi')` field: value encrypted before DB insert, decrypted after load
- Entity with `compliance('pci')` field: same behavior
- Entity with `compliance('pii')` field: NOT encrypted (passed through)
- Entity with `compliance('none')` field: NOT encrypted
- `em.nativeInsert()` on entity with `phi`/`pci` field throws `EncryptionRequiredError`
- `em.nativeInsert()` on entity with only `pii`/`none` fields succeeds
- Missing encryption key aborts persist with clear error
- Corrupted ciphertext on load throws `DecryptionError`
- Pre-migration plaintext (no `v1:` prefix) loaded with warning (graceful degradation)
- Null field values are not encrypted (null → null roundtrip)

---

### Step 5: Tenant Isolation — MikroORM Global Filter

**Files to create:**
- `framework/core/src/persistence/tenantFilter.ts`

**Files to modify:**
- `framework/core/src/persistence/index.ts` — export filter setup

**What the code should do:**

1. Define a MikroORM global filter named `'tenant'` that adds `WHERE organization_id = :tenantId` to every query on entities that have an `organizationId` (or `organization`) relationship property. The filter is enabled by default.

2. `setupTenantFilter(orm: MikroORM)` function called at application bootstrap that registers the filter globally.

3. `getSuperAdminContext(em: EntityManager, auditLogger: AuditLogger): EntityManager` function that:
   - Forks the EM with the tenant filter disabled
   - Emits a `'super_admin_bypass'` audit event via AuditLogger
   - Should only be called from code paths that have verified super-admin permissions

4. The tenant filter parameter (`tenantId`) is set per-request by the tenant context middleware (Step 8) when forking the EM.

**Dependencies:** None (audit logger integration added later)

**Test requirements:**
- `em.find(Entity, {})` with tenant filter returns only current tenant's data
- `em.find(Entity, {})` on entity without `organizationId` is NOT filtered
- `getSuperAdminContext(em).find(Entity, {})` returns cross-tenant data
- Filter cannot be disabled on a regular EM (only via `getSuperAdminContext`)

---

### Step 6: Tenant Isolation — PostgreSQL RLS (Configurable)

**Files to create:**
- `framework/core/src/persistence/rls.ts`

**What the code should do:**

1. `setupRLS(orm: MikroORM, enabled: boolean)` function:
   - If `enabled` is false or database driver is not PostgreSQL, skip.
   - For each entity with an `organizationId` property, verify that an RLS policy exists on the table. If not, log a warning with the SQL needed to create it:
     ```sql
     ALTER TABLE "table_name" ENABLE ROW LEVEL SECURITY;
     CREATE POLICY tenant_isolation ON "table_name"
       USING (organization_id = current_setting('app.tenant_id'));
     ```
   - The framework does NOT auto-create RLS policies (that's a migration concern). It validates they exist.

2. Implicit transaction wrapper: use MikroORM's `EventSubscriber.beforeTransactionStart` to execute `SET LOCAL app.tenant_id = :tenantId` at the start of every transaction. Since `em.flush()` internally uses transactions, the SET LOCAL is automatically included. The `tenantId` is read from the EM's filter parameters.

3. Configuration: `complianceConfig.rls: boolean` on application options (default `true` on PostgreSQL, `false` on other DBs). User can set to `false` to opt out.

**Dependencies:** Step 5 (tenant filter)

**Test requirements:**
- On PostgreSQL: `SET LOCAL app.tenant_id` executed before each transaction
- On non-PostgreSQL: RLS setup skipped without error
- With `rls: false` on PostgreSQL: RLS setup skipped, only MikroORM filter active
- Integration test with real PG: cross-tenant raw query blocked by RLS policy

---

### Step 7: Audit Logger

**Files to create:**
- `framework/core/src/http/telemetry/auditLogger.ts`

**Files to modify:**
- `framework/core/src/http/middleware/response/enrichExpressLikeSend.middleware.ts` — Add audit log emission after response
- `framework/core/src/http/index.ts` — export AuditLogger

**What the code should do:**

1. `AuditLogger` class that wraps the existing `OpenTelemetryCollector`:
   ```typescript
   class AuditLogger {
     constructor(private otel: OpenTelemetryCollector<MetricsDefinition>) {}

     append(entry: AuditEntry): void {
       this.otel.info(entry, {
         'log.type': 'audit',
         'audit.timestamp': entry.timestamp,
         'audit.userId': entry.userId,
         'audit.tenantId': entry.tenantId,
         'audit.route': entry.route,
         'audit.method': entry.method,
         'audit.bodyHash': entry.bodyHash,
         'audit.status': entry.status,
         'audit.duration': entry.duration,
         'audit.redactedFields': entry.redactedFields
       });
     }
   }
   ```

2. `AuditEntry` type:
   ```typescript
   type AuditEntry = {
     timestamp: string;
     userId: string | null;
     tenantId: string | null;
     route: string;
     method: string;
     bodyHash: string;
     status: number;
     duration: number;
     redactedFields: string[];
     eventType: 'http' | 'ws' | 'auth_failure' | 'rate_limit'
              | 'rbac_deny' | 'super_admin_bypass';
   };
   ```

3. Response middleware (in enrichExpressLikeSend pipeline, runs AFTER response):
   - Compute SHA-256 hash of request body
   - Read compliance metadata from entity schemas involved in the response and list redacted field names
   - Capture duration from request start time (already tracked via OTEL correlation)
   - Call `auditLogger.append()`
   - Catch OTEL errors and log locally (non-fatal)

4. The AuditLogger is shared — used by:
   - HTTP response middleware (every request/response)
   - Auth middleware (auth failures → `eventType: 'auth_failure'`)
   - Rate limiter (rate limit hits → `eventType: 'rate_limit'`)
   - WS server (WS events → `eventType: 'ws'`)
   - Super-admin context (bypass → `eventType: 'super_admin_bypass'`)

**Dependencies:** Uses existing `OpenTelemetryCollector`

**Test requirements:**
- Every HTTP request produces an audit entry with all required fields
- Request body is hashed (SHA-256), never logged in plaintext
- Fields with compliance != 'none' are listed in `redactedFields`, values never in log
- Auth failures produce entries with `eventType: 'auth_failure'`
- OTEL transport failure doesn't crash the request (non-fatal)

---

### Step 8: Tenant Context Middleware

**Files to create:**
- `framework/core/src/http/middleware/request/tenantContext.middleware.ts`

**Files to modify:**
- `framework/core/src/http/router/expressLikeRouter.ts` — Insert tenant context middleware into pipeline after auth

**What the code should do:**

1. After the auth middleware extracts the session, the tenant context middleware:
   - For `'protected'`/`'authenticated'` routes: reads `req.session.organizationId` (or `activeOrganizationId` from better-auth). If missing on a protected/authenticated route, returns 403 + audit log.
   - For `'internal'` routes: reads tenant ID from the HMAC payload or a dedicated header (`X-Tenant-Id`).
   - For `'public'` routes: no tenant context set; tenant filter remains disabled for this request.

2. Forks the EntityManager with the tenant filter parameter set:
   ```typescript
   const scopedEm = em.fork();
   scopedEm.setFilterParams('tenant', { tenantId: organizationId });
   req.em = scopedEm;
   ```

3. For PostgreSQL with RLS enabled, the tenant ID is stored on the EM context so the `beforeTransactionStart` subscriber can execute `SET LOCAL` with it.

**Dependencies:** Step 5 (tenant filter), Step 7 (audit logger)

**Test requirements:**
- Protected route without organizationId in session returns 403 + audit log
- Scoped EM has tenant filter parameter set correctly
- Public route has no tenant context (filter disabled)
- Internal route reads tenant from HMAC payload/header
- EM fork is request-scoped (not shared across requests)

---

### Step 9: Rate Limiter Middleware

**Files to create:**
- `framework/core/src/http/middleware/request/rateLimit.middleware.ts`
- `framework/core/src/http/rateLimit/rateLimiter.ts`

**Files to modify:**
- `framework/core/src/http/router/expressLikeRouter.ts` — Insert rate limiter as first middleware in pipeline
- `framework/core/src/http/application/expressLikeApplication.ts` — Accept `rateLimitConfig` in application options

**What the code should do:**

1. `RateLimiter` class that takes a `TtlCache` (Redis) instance:
   - `check(key: string, limit: number, windowMs: number): Promise<{ allowed: boolean; remaining: number; resetAt: number }>`
   - Internally: `MULTI` → `INCR key` → `PEXPIRE key windowMs` (only if key is new) → `EXEC`. Single Redis round-trip.

2. Rate limit middleware:
   - Key format: `ratelimit:{tenantId}:{routePath}:{userId}:{read|write}`. For public routes without userId, use IP: `ratelimit:ip:{ip}:{routePath}:{read|write}`.
   - Read operations: GET, HEAD, OPTIONS → uses `rateLimitConfig.read` limit.
   - Write operations: POST, PUT, PATCH, DELETE → uses `rateLimitConfig.write` limit.
   - If exceeded: return 429 with `Retry-After` header + audit log entry (`eventType: 'rate_limit'`).
   - If Redis is unreachable: catch error, log warning via OTEL, allow request through (fail open).
   - Set response headers: `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`.

3. Rate limit config on application options:
   ```typescript
   type RateLimitConfig = {
     read: number;
     write: number;
     windowMs: number;
   };
   ```

**Dependencies:** Step 7 (audit logger), existing `TtlCache`/`RedisTtlCache` from `framework/infrastructure/redis`

**Test requirements:**
- Request under limit passes through with correct response headers
- Request at limit returns 429 with `Retry-After` header
- Redis failure allows request through (fail open) with warning logged
- Read and write operations use separate limits
- Different tenants have independent counters
- Different users within same tenant have independent counters
- Rate limit hit produces audit log entry

---

### Step 10: Secrets Accessor

**Files to create:**
- `framework/core/src/secrets/secretsAccessor.ts`
- `framework/core/src/secrets/index.ts`

**What the code should do:**

1. `SecretsAccessor` class:
   ```typescript
   class SecretsAccessor {
     constructor(
       private declaredSecrets: string[],
       private localSecretsFile?: string
     ) {}

     validateAtBoot(): void {
       const missing: string[] = [];
       for (const key of this.declaredSecrets) {
         if (!process.env[key]) missing.push(key);
       }
       if (missing.length > 0) {
         console.error(`Missing required secrets: ${missing.join(', ')}`);
         process.exit(1);
       }
     }

     getSecret(key: string): string {
       if (!this.declaredSecrets.includes(key)) {
         throw new UndeclaredSecretError(
           `Secret '${key}' not declared in manifest. Add to manifest.toml [secrets].`
         );
       }
       const value = process.env[key];
       if (!value) {
         throw new MissingSecretError(`Secret '${key}' declared but not in environment.`);
       }
       return value;
     }
   }
   ```

2. At application bootstrap:
   - Read declared secrets from config (manifest-derived)
   - If `NODE_ENV !== 'production'` and `.secrets.local` exists, load via dotenv
   - Call `validateAtBoot()` — missing secrets = `process.exit(1)` with clear listing
   - If any entity has `compliance('phi')` or `compliance('pci')`, validate `ENCRYPTION_MASTER_KEY` is present
   - Register as singleton in DI container

3. Manifest additions:
   ```toml
   [secrets]
   required = ["ENCRYPTION_MASTER_KEY", "HMAC_SECRET_KEY", "JWKS_PUBLIC_KEY_URL"]
   ```

**Dependencies:** None

**Test requirements:**
- `getSecret('DECLARED_KEY')` returns env var value
- `getSecret('UNDECLARED_KEY')` throws `UndeclaredSecretError`
- Boot with missing declared secret exits with clear error listing missing keys
- `.secrets.local` loaded in non-production env
- `.secrets.local` NOT loaded in production env

---

### Step 11: WebSocket Auth, Channels & Audit

**Files to modify:**
- `framework/ws/src/webSocketServer.ts` — Add auth handshake, channels, periodic re-validation, audit logging

**Files to create:**
- `framework/ws/src/channels.ts` — Channel authorization layer

**What the code should do:**

1. `ForklaunchWebSocketServer` constructor accepts additional options:
   ```typescript
   type WSSecurityOptions = {
     authenticate: (req: IncomingMessage) => Promise<WSSession | null>;
     revalidateIntervalMs?: number;  // default 300_000 (5 min)
     auditLogger: AuditLogger;
   };

   type WSSession = {
     userId: string;
     tenantId: string;
     roles: string[];
     permissions: string[];
   };
   ```

2. Override `handleUpgrade` to intercept the upgrade request:
   - Call `authenticate(req)`. If it returns `null`, destroy the socket and emit audit log with `eventType: 'auth_failure'`.
   - If authenticated, tag the WebSocket connection with the session.

3. Periodic re-validation:
   - After connection, set an interval that calls `authenticate(req)` again.
   - If re-validation fails, close connection with code 4001 + audit log.
   - Clear interval on connection close.

4. Channel system:
   ```typescript
   class WSChannelManager {
     subscribe(ws: ForklaunchWebSocket, channel: string, requiredPermissions: string[]): void;
     unsubscribe(ws: ForklaunchWebSocket, channel: string): void;
     broadcast(channel: string, event: unknown, senderTenantId: string): void {
       // For each connection in channel:
       //   if conn.session.tenantId !== senderTenantId → skip
       //   if !requiredPermissions.every(p => conn.session.permissions.includes(p)) → skip
       //   else → conn.send(event)
     }
   }
   ```

5. All WS events (connect, message, broadcast, disconnect) emit audit log entries.

**Dependencies:** Step 7 (audit logger)

**Test requirements:**
- Valid JWT at handshake → connection accepted, session tagged
- Invalid JWT → socket destroyed, audit log `auth_failure`
- Periodic re-validation closes connection on expired session
- Broadcast to channel only reaches authorized recipients in same tenant
- Cross-tenant broadcast is blocked (different tenantId → skipped)
- Recipient without required permission → skipped
- All WS events produce audit log entries
- Re-validation interval is configurable

---

### Step 12: Data Residency (Manifest + Compiler)

**Files to modify:**
- CLI: `cli/src/core/manifest/` — Add `data_residency` field to manifest TOML schema

**What the code should do:**

1. Add to `manifest.toml` schema:
   ```toml
   [compliance]
   data_residency = ["us-east-1", "eu-west-1"]
   ```

2. CLI validates `data_residency` is well-formed (valid AWS/GCP/Azure region identifiers).

3. The platform compiler reads this during deployment and validates target region. The framework carries the field in the manifest but does NOT enforce at runtime (deployment-time concern).

**Dependencies:** None

**Test requirements:**
- Manifest with `data_residency` parses correctly
- Manifest with invalid region format rejected by CLI validation

---

### Step 13: Guided Data Model Setup (CLI)

**Files to modify:**
- CLI: `cli/src/init/` — Add compliance classification prompts when creating entities

**What the code should do:**

1. When `forklaunch init entity` creates a new entity, for each field prompt:
   ```
   Field: email (string)
   Classification:
     1. Non-sensitive (none)
     2. Personal data / PII (pii)
     3. Health data / PHI (phi)
     4. Financial data / PCI (pci)
   > 2
   ```

2. Generate entity using `fp` and `defineComplianceEntity`:
   ```typescript
   import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

   export const User = defineComplianceEntity({
     name: 'User',
     properties: {
       ...sqlBaseProperties,  // these need compliance('none') too
       email: fp.string().compliance('pii'),
       name: fp.string().compliance('none')
     }
   });
   export type User = InferEntity<typeof User>;
   ```

3. Store classification in manifest:
   ```toml
   [compliance.entities.User]
   email = "pii"
   name = "none"
   ```

4. Skip defaults to `'none'` with a warning.

**Dependencies:** Step 1 (fp builder)

**Test requirements:**
- Entity generation with classification produces correct `fp`/`defineComplianceEntity` imports
- Classification stored in manifest
- Skip defaults to `'none'` with warning

---

### Step 14: Compliance Audit Report (CLI)

**Files to create:**
- CLI: `cli/src/compliance/audit_report.rs`

**What the code should do:**

1. `forklaunch compliance audit` command:
   - Read route definitions (parse from OpenAPI spec which is already generated)
   - Read entity classifications from manifest `[compliance.entities]`
   - Read secrets declarations from manifest `[secrets]`
   - Read data residency from manifest `[compliance.data_residency]`
   - Read rate limit config

2. Generate JSON report:
   ```json
   {
     "generatedAt": "2026-03-22T...",
     "routes": [
       { "path": "/user", "method": "GET", "access": "protected",
         "rbac": { "allowedRoles": ["admin"] }, "rateLimit": { "read": 100 } }
     ],
     "entities": [
       { "name": "User", "fields": [
         { "name": "email", "compliance": "pii", "encrypted": false },
         { "name": "medicalRecord", "compliance": "phi", "encrypted": true }
       ]}
     ],
     "secrets": { "declared": ["ENCRYPTION_MASTER_KEY"], "allPresent": true },
     "dataResidency": { "allowedRegions": ["us-east-1"] },
     "auditLog": { "store": "otel-loki" }
   }
   ```

3. Optionally generate PDF via Rust PDF library.

**Dependencies:** Steps 12, 13 (manifest fields)

**Test requirements:**
- Report includes all routes with access levels and RBAC config
- Report includes all entities with compliance classifications
- Report includes secrets status
- JSON output is valid
- Report succeeds on project with all `'none'` compliance fields

---

### Step 15: Blueprint Updates & Migration Guide

**Files to modify:**
- All blueprint entity files — switch from `p`/`defineEntity` to `fp`/`defineComplianceEntity`
- All blueprint route files — add `access` field
- `blueprint/core/persistence/` — update `sqlBaseProperties` and `nosqlBaseProperties` to use `fp` with `compliance('none')`

**What the code should do:**

1. Update base properties:
   ```typescript
   import { fp } from '@forklaunch/core/persistence';

   export const sqlBaseProperties = {
     id: fp.uuid().compliance('none').primary().onCreate(() => v4()),
     createdAt: fp.datetime().compliance('none').onCreate(() => new Date()),
     updatedAt: fp.datetime().compliance('none').onCreate(() => new Date()).onUpdate(() => new Date())
   };
   ```

2. Update all entity definitions:
   ```typescript
   import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

   export const User = defineComplianceEntity({
     name: 'User',
     properties: {
       id: fp.string().compliance('none').primary(),
       email: fp.string().compliance('pii'),
       name: fp.string().compliance('none'),
       organization: () => fp.manyToOne(Organization).compliance('none').nullable()
     }
   });
   export type User = InferEntity<typeof User>;
   ```

3. Update all route definitions with `access` (auth stays per-route, use shared constants):
   ```typescript
   // Shared auth constants (e.g., in blueprint/core/auth/authConstants.ts)
   export const PROTECTED_JWT = {
     jwt: { jwksPublicKeyUrl: getEnvVar('JWKS_PUBLIC_KEY_URL') },
     sessionSchema: SessionSchema,
   } as const;

   export const INTERNAL_HMAC = {
     hmac: { secretKeys: { default: getEnvVar('HMAC_SECRET_KEY') } },
   } as const;

   // Route usage
   router.get('/', {
     name: 'GetUser',
     summary: 'Gets a user',
     access: 'protected',
     auth: { ...PROTECTED_JWT, allowedRoles: new Set(['admin']) },
     responses: { 200: UserSchema }
   }, handler);

   router.get('/health', {
     name: 'HealthCheck',
     summary: 'Health check',
     access: 'public',
     responses: { 200: { status: string } }
   }, handler);

   router.post('/sync', {
     name: 'SyncData',
     summary: 'Inter-service sync',
     access: 'internal',
     auth: INTERNAL_HMAC,
     body: SyncSchema,
     responses: { 200: SyncResponseSchema }
   }, handler);
   ```

4. Update application bootstrap:
   ```typescript
   const app = new ForklaunchExpressApplication(schemaValidator, {
     complianceConfig: {
       rls: true,
       encryption: { masterKeyEnvVar: 'ENCRYPTION_MASTER_KEY' }
     },
     rateLimitConfig: { read: 100, write: 50, windowMs: 60_000 }
   });
   ```

**Dependencies:** Steps 1-11 (all framework features)

**Test requirements:**
- Blueprint builds successfully with all new required fields
- All existing blueprint tests pass
- All routes have `access` field (compile-time enforced)
- All entity fields have `.compliance()` (compile-time enforced)
- SDK generated from blueprint routes has correct auth header requirements

---

## Data Model Changes

**New entities:** None. Audit logs go to OTEL/Loki, not a database table.

**Modified entities:** All existing blueprint entities get `compliance()` metadata via `fp` + `defineComplianceEntity`. No database schema change — compliance is metadata stored in the framework's in-memory registry, not a column.

**Encryption data format:** Encrypted fields (PHI/PCI only) store `v1:base64(iv):base64(authTag):base64(ciphertext)` in the same column. Column type remains `string`/`text`. Existing plaintext data is handled gracefully on read (no `v1:` prefix → treated as pre-migration).

**Encryption data flow:**
```
  WRITE PATH (onBeforeCreate / onBeforeUpdate)

  Entity field value              Compliance metadata
       |                               |
       v                               v
  +----+----+                   +------+------+
  | "John   |                   | compliance: |
  |  Doe"   |                   |    'phi'    |
  +----+----+                   +------+------+
       |                               |
       |         +---------+           |
       +-------->| phi/pci?|<----------+
                 +----+----+
                      |
              +-------+-------+
              |               |
           [YES]           ['none'/'pii']
              |               |
              v               v
  +-----------+------+    (pass through)
  | HKDF derive key  |
  |  masterKey +      |
  |  tenantId         |
  +-----------+------+
              |
              v
  +-----------+------+
  | AES-256-GCM      |
  |  random IV        |
  |  encrypt(value)   |
  +-----------+------+
              |
              v
  +-----------+------+
  | v1:iv:tag:cipher  |  <-- stored in DB
  +-----------+------+


  READ PATH (onAfterLoad)

  DB column value             Compliance metadata
       |                            |
       v                            v
  +----+---------------+    +------+------+
  | "v1:iv:tag:cipher" |    | compliance: |
  +----+---------------+    |    'phi'    |
       |                     +------+------+
       v                            |
  starts with "v1:"? ---------------+
       |
    [YES] --> derive key(tenantId) --> AES-256-GCM decrypt --> "John Doe"
    [NO]  --> return as-is (pre-migration) + log warning
```

**New MikroORM infrastructure:**
- Global filter: `tenant` — adds `WHERE organization_id = :tenantId`
- EventSubscriber: `ComplianceEventSubscriber` — encrypt/decrypt on persist/load
- EventSubscriber: RLS transaction hook — `SET LOCAL app.tenant_id` (PostgreSQL only)
- RLS policies: Migration-generated SQL (not auto-created by framework)

---

## HTTP Request Data Flow

```
                         INCOMING HTTP REQUEST
                                |
                                v
+--[1. RATE LIMITER]----------------------------------------------------+
|  key = ratelimit:{tenantId}:{route}:{userId}:{read|write}             |
|                                                                        |
|  HAPPY:    Redis INCR < limit       --> pass through                  |
|  ERROR:    Redis unreachable         --> fail open, log warning        |
|  EXCEEDED: Redis INCR >= limit      --> 429 + audit log, STOP         |
|  NULL:     No userId (public route) --> key = ratelimit:ip:{ip}:...   |
+---+-------------------------------------------------------------------+
    |
    v
+--[2. AUTH MIDDLEWARE]-------------------------------------------------+
|  access = 'public' | 'authenticated' | 'protected' | 'internal'      |
|                                                                        |
|  PUBLIC:       skip auth entirely    --> pass through                  |
|  AUTHENTICATED: JWT/Basic from auth  --> validate, decode session     |
|  PROTECTED:    JWT/Basic from auth   --> validate, decode, check RBAC |
|  INTERNAL:     HMAC from auth        --> validate signature           |
|                                                                        |
|  FAIL: 401/403 + audit log (eventType: 'auth_failure'), STOP         |
+---+-------------------------------------------------------------------+
    |
    v
+--[3. TENANT CONTEXT]-------------------------------------------------+
|  PROTECTED/AUTHENTICATED: tenantId = req.session.organizationId       |
|    HAPPY:   set on forked EM, enable filter                           |
|    NULL:    403 + audit log, STOP                                     |
|  INTERNAL:  tenantId from HMAC payload or X-Tenant-Id header          |
|  PUBLIC:    no tenant context, filter disabled                        |
|                                                                        |
|  PostgreSQL (if RLS enabled):                                         |
|    tenantId stored on EM for SET LOCAL in next transaction             |
+---+-------------------------------------------------------------------+
    |
    v
+--[4. HANDLER]---------------------------------------------------------+
|  em.find(Entity, { ... })                                             |
|    --> TenantFilter auto-appends WHERE organization_id = :tenantId    |
|    --> RLS (PG): database also enforces row filtering                 |
|                                                                        |
|  em.create(Entity, { medicalRecord: 'x' })                           |
|    --> em.flush()                                                      |
|      --> BEGIN TRANSACTION                                             |
|      --> SET LOCAL app.tenant_id = :tenantId  (PG + RLS only)         |
|      --> ComplianceEventSubscriber.onBeforeCreate:                     |
|          phi/pci fields → encrypt(value, tenantId)                    |
|          pii/none fields → pass through                                |
|      --> INSERT                                                        |
|      --> COMMIT                                                        |
|                                                                        |
|  em.nativeInsert(ComplianceEntity, ...)                                |
|    --> THROW: "nativeInsert blocked on compliance entity"              |
+---+-------------------------------------------------------------------+
    |
    v
+--[5. RESPONSE + AUDIT LOG]-------------------------------------------+
|  AuditLogger.append({                                                 |
|    timestamp, userId, tenantId, route, method,                        |
|    bodyHash: SHA256(requestBody),                                     |
|    status, duration,                                                   |
|    redactedFields: [fields where compliance != 'none']                |
|  }) --> OTEL (log.type: 'audit') --> Loki                            |
+-----------------------------------------------------------------------+
```

---

## WebSocket Lifecycle

```
  CLIENT                           SERVER
    |                                |
    |--- WS Upgrade Request -------->|
    |    (Authorization: Bearer JWT) |
    |                                |
    |                     +----------+-----------+
    |                     | authenticate(req)    |
    |                     |   validate JWT       |
    |                     |   extract session    |
    |                     |   check tenantId     |
    |                     +----------+-----------+
    |                                |
    |                        [valid?]|
    |              +---------+-------+---------+
    |              |                           |
    |           [YES]                       [NO]
    |              |                           |
    |              v                           v
    |    tag connection with           socket.destroy()
    |    { tenantId, userId,           + audit log (auth_failure)
    |      roles, permissions }        |
    |              |                   |<-- STOP
    |              v
    |    +--[PERIODIC RE-VALIDATION TIMER]--+
    |    | every N ms (configurable):       |
    |    |   if expired/revoked:            |
    |    |     close(4001) + audit log      |
    |    +----------------------------------+
    |              |
    |    +--[BROADCAST to channel]----------+
    |    | for each connection:             |
    |    |   if conn.tenantId != sender:    |
    |    |       SKIP                       |
    |    |   if !hasPermission(conn, chan): |
    |    |       SKIP                       |
    |    |   else: send(event)             |
    |    +----------------------------------+
```

---

## Tenant Isolation — Defense in Depth

```
  APPLICATION CODE
       |
       | em.find(User, { name: 'John' })
       v
  +----+-----------------------------------------+
  |         LAYER 1: MikroORM Global Filter       |
  |                 (always enabled)               |
  |                                                |
  |  Transforms query to:                         |
  |    SELECT * FROM "User"                        |
  |    WHERE name = 'John'                         |
  |    AND organization_id = :tenantId             |
  +----+-----------------------------------------+
       |
       v
  +----+-----------------------------------------+
  |         LAYER 2: PostgreSQL RLS                |
  |         (configurable, default on for PG)      |
  |                                                |
  |  Even if Layer 1 bypassed, PG enforces:       |
  |  USING (organization_id =                      |
  |    current_setting('app.tenant_id'))           |
  +----+-----------------------------------------+
       |
       v
  +----+-----------------------------------------+
  |         LAYER 3: Native Query Block            |
  |         (phi/pci entities only)                |
  |                                                |
  |  em.nativeInsert(ComplianceEntity)             |
  |    --> THROW (bypasses Layers 1 & 2)           |
  +----+-----------------------------------------+
       |
       v
  QUERY RESULT (tenant-scoped, guaranteed)


  SUPER-ADMIN BYPASS:
  +------------------------------------------------+
  |  const superEm = getSuperAdminContext(em, ...); |
  |    --> disables TenantFilter                    |
  |    --> audit logged as super_admin_bypass       |
  +------------------------------------------------+
```

---

## Startup Validation Sequence

```
  APPLICATION BOOT
       |
       v
  +--[1. SECRETS VALIDATION]-------------------------------+
  |  Read declared secrets from manifest config             |
  |  For each: check process.env[key] (or .secrets.local)  |
  |  MISSING? --> list missing secrets, EXIT(1)            |
  +---+-----------------------------------------------------+
      |
      v
  +--[2. ENCRYPTION KEY VALIDATION]------------------------+
  |  If any entity has compliance('phi') or ('pci'):       |
  |    check ENCRYPTION_MASTER_KEY in env                   |
  |    MISSING? --> EXIT(1)                                |
  +---+-----------------------------------------------------+
      |
      v
  +--[3. ROUTE ACCESS VALIDATION]---------------------------+
  |  For each registered route:                            |
  |    access field must exist                              |
  |    'protected' must have RBAC declaration               |
  |    'internal' must have HMAC auth                       |
  |    'public' must not have auth                          |
  |  FAIL? --> name offending route, EXIT(1)               |
  +---+-----------------------------------------------------+
      |
      v
  +--[4. REDIS CONNECTION]---------------------------------+
  |  Attempt Redis connection for rate limiting             |
  |  FAILED? --> WARN: rate limiting degraded, continue    |
  +---+-----------------------------------------------------+
      |
      v
  +--[5. RLS SETUP (PG only, if enabled)]------------------+
  |  Verify RLS policies exist per tenant-scoped entity    |
  |  MISSING? --> WARN with SQL to create policy           |
  +---+-----------------------------------------------------+
      |
      v
  APPLICATION READY (listen)
```

---

## Error Handling

```
ERROR TYPE                  | CAUGHT | HANDLER ACTION                    | USER SEES                    | TESTED
----------------------------|--------|-----------------------------------|------------------------------|-------
OtelTransportError          | Y      | Log locally, continue             | Nothing (degraded audit)     | Y
MissingEncryptionKeyError   | Y      | Refuse write, throw               | 500 "Encryption unavailable" | Y
DecryptionError             | Y      | Log error, throw                  | 500 "Data integrity error"   | Y
EncryptionRequiredError     | Y      | Block persist/native query        | 500 "Cannot store unencrypted"| Y
MissingTenantContextError   | Y      | 403 + audit log                   | 403 "Tenant context required"| Y
RedisConnectionError        | Y      | Fail open, warn                   | Nothing (degraded rate limit)| Y
RateLimitExceededError      | Y      | 429 + audit log                   | 429 + Retry-After header     | Y
UndeclaredSecretError       | Y      | Throw at call site                | 500                          | Y
MissingSecretError          | Y      | Refuse to start                   | App won't boot               | Y
AuthenticationError (WS)    | Y      | Destroy socket, close 4001        | WS close code 4001           | Y
```

---

## Test Plan

Each feature requires three test categories:

**Unit tests** (no DB/Redis):
- `fp` proxy: wraps all `p` methods, stores compliance, phantom types enforce `.compliance()`
- `FieldEncryptor`: encrypt/decrypt roundtrip, HKDF, error cases
- `AuditLogger`: entry format, redaction, body hashing
- `RateLimiter`: counter logic (mock Redis)
- `SecretsAccessor`: validation logic
- `WSChannelManager`: tenant scoping, permission checking
- Access type narrowing: compile-time tests via `// @ts-expect-error`

**Integration tests** (real DB/Redis):
- Full HTTP request through pipeline → audit log emitted with correct fields
- Entity with `compliance('phi')` persisted → DB column contains ciphertext, load returns plaintext
- Tenant A's query → only tenant A's data (both filter and RLS)
- 100 requests → 101st returns 429
- WS handshake with valid JWT → accepted; invalid → rejected
- Boot with missing secret → exit(1)
- SDK generated from routes has correct auth header requirements

**Bypass tests** (enforcement cannot be skipped):
- `em.nativeInsert()` on phi/pci entity → `EncryptionRequiredError`
- `em.find()` without tenant context on protected route → 403
- Route without `access` → startup error (also compile-time error)
- `access: 'protected'` without RBAC → startup error (also compile-time error)
- `getSecret('undeclared')` → `UndeclaredSecretError`
- WS broadcast across tenants → recipient doesn't receive
- Direct filter disable without `getSuperAdminContext` → blocked

---

## Performance Considerations

| Concern | Impact | Mitigation |
|---------|--------|------------|
| AES-256-GCM per field | < 0.05ms/field (AES-NI) | Only phi/pci fields encrypted |
| Redis round-trip (rate limit) | ~1ms/request | Single MULTI/EXEC; fail open |
| SET LOCAL per transaction (RLS) | < 0.1ms | PG only; configurable opt-out |
| OTEL log emission | Async, non-blocking | Batched by OTEL SDK |
| MikroORM global filter | Adds WHERE clause | Index on organization_id required |
| WS periodic re-validation | 1 check/connection/interval | Configurable; negligible at scale |

**Required indexes:** `organization_id` on every tenant-scoped entity table. Framework should warn at startup if missing.

---

## Deployment Plan

**Migration order:**
1. Database: add RLS policies to PostgreSQL tables (if RLS enabled)
2. Database: ensure `organization_id` indexed on all tenant-scoped tables
3. Deploy framework v2 with all features
4. Run one-time encryption migration for existing PHI/PCI plaintext data

**Rolling deployment support:** `onAfterLoad` checks for `v1:` prefix — plaintext values (from v1 code) are returned as-is with a warning. Encrypted values (from v2 code) are decrypted. This allows v1 and v2 to coexist temporarily.

**Rollback plan:**
1. Revert to framework v1
2. Encrypted data needs decryption migration to fully rollback
3. RLS policies: `DROP POLICY tenant_isolation ON "table"; ALTER TABLE "table" DISABLE ROW LEVEL SECURITY;`
4. Redis rate limit keys expire naturally

---

## Observability

**New OTEL attributes:**
- `log.type: 'audit'` — identifies audit log entries for Loki routing
- `audit.*` — userId, tenantId, route, method, bodyHash, status, duration, eventType, redactedFields

**New metrics:**
- `forklaunch.encryption.operations` (counter) — encrypt/decrypt
- `forklaunch.rate_limit.hits` (counter) — by tenant, route
- `forklaunch.rate_limit.exceeded` (counter) — by tenant, route
- `forklaunch.tenant_filter.bypasses` (counter) — super-admin usage
- `forklaunch.ws.connections` (gauge) — by tenant
- `forklaunch.ws.auth_failures` (counter)

---

## NOT in Scope

| Item | Rationale |
|------|-----------|
| Regulation-specific modes (HIPAA/PCI/SOC2 toggles) | Phase 2 — requires regulatory mapping |
| Data lineage tracking | Phase 2 — track data flow beyond system boundary |
| Automated compliance certification | Phase 2 — auto-generate audit artifacts |
| Multi-cloud residency routing | Phase 2 — route requests to region-local instances |
| Key rotation migration tooling | Phase 2 — CLI to re-encrypt with new master key |
| PII field encryption | Dropped — RDS encryption + TLS sufficient |
| Secrets manager SDK integration | Compiler responsibility |
| App/router-level auth config | Dropped — auth stays per-route; shared constants for DRY |
| SDK type changes | Not needed — `access` type-narrows `auth`; SDK reads `auth` unchanged |

---

## Failure Modes Registry

```
CODEPATH               | FAILURE MODE              | CAUGHT | TEST | USER SEES       | LOGGED
-----------------------|---------------------------|--------|------|-----------------|-------
AuditLogger#append     | OTEL collector down       | Y      | Y    | Nothing(degrade)| Y
FieldEncryptor#encrypt | Master key missing        | Y      | Y    | 500 + clear msg | Y
FieldEncryptor#decrypt | Corrupted ciphertext      | Y      | Y    | 500 + error     | Y
FieldEncryptor#decrypt | Pre-migration plaintext   | Y      | Y    | Transparent*    | Y
TenantFilter           | No tenant in context      | Y      | Y    | 403             | Y
TenantFilter           | em.nativeInsert bypass    | Y      | Y    | Throw           | Y
RateLimiter            | Redis down                | Y      | Y    | Nothing(degrade)| Y
SecretsAccessor        | Missing env var at boot   | Y      | Y    | Boot failure    | Y
WS handshake           | JWT expired               | Y      | Y    | WS close 4001   | Y
WS broadcast           | Recipient disconnected    | Y      | Y    | Silent skip     | Y
RLS SET LOCAL          | Non-Postgres DB           | Y      | Y    | ORM filter only | Y

* Pre-migration: no v1: prefix → return as-is + log warning
```

**Critical gaps: 0.**

---

## Unresolved Decisions

None. All decisions resolved during review phases.
