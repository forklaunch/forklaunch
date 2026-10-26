# {{{name}}}

PostgreSQL IAM with tenant isolation, encrypted directory CRUD, Argon2id credentials, and persistent opaque or explicitly configured RS256 bearer sessions.

The crate follows the generated service layout. `config.rs` reads trusted settings, `registrations.rs` builds the stores and `AppState`, and `api/routes/<resource>.rs` binds each IAM contract to the controller in `api/controllers/<resource>.rs`. Controllers are the module's own code: each extracts its request, calls the audited session or directory service and maps the result to the contract's responses, so edit one to customize that operation. `forklaunch-iam-base` keeps storage, sessions, audit and authorization. Routers added with `forklaunch init router` live in `api/routes/extensions`.

Apply migrations using a schema-owner connection:

```sh
DATABASE_URL="$DATABASE_OWNER_URL" cargo run -p {{{name}}} -- --migrate
```

Grant the runtime role USAGE on the schema and SELECT/INSERT/UPDATE/DELETE on `fl_iam_users`, `fl_iam_organizations`, `fl_iam_roles`, `fl_iam_sessions`, `fl_iam_directory`, `fl_iam_directory_unique_claims`, and `fl_iam_directory_audit`, and (when browser sessions are enabled) `fl_iam_browser_sessions`. Grant USAGE on `fl_iam_directory_sequence_seq`. It must have neither SUPERUSER nor BYPASSRLS. Configure `DATABASE_URL`, a trusted `TENANT_ID`, `ENCRYPTION_KEY`, `HMAC_SECRET_KEY`, and `JWKS_PUBLIC_KEY_URL`, then run `cargo run -p {{{name}}}`. The service rejects privileged database roles. Environment files are examples; export variables before running.

`{{{name}}} --seed` creates or updates the roles, organizations and login users listed in `seed.toml` (or the file named by `IAM_SEED_FILE`) with the restricted runtime `DATABASE_URL`, `TENANT_ID` and `ENCRYPTION_KEY`. Each seeded user also gets the directory records `POST /user` creates (permissions, roles, its organization and a directory user under the login id), so surfacing its roles and the JWT-protected directory routes work for it; with `IAM_DIRECTORY_AUTHORIZATION=stored-session` the seed also links the login to that directory user. It is idempotent: roles and profiles are rewritten and passwords reset on each run, and nothing is deleted. Each user names the variable holding its password (`IAM_SEED_ADMIN_PASSWORD` for the default admin, which `forklaunch init` writes to the git-ignored root `.env`); the file itself holds no secrets. Docker Compose runs it as the one-shot `{{{name}}}-seed` service after migrations. The default roles match the TypeScript IAM migration: `viewer` (`platform:read`) and `editor`, `admin`, `system` (`platform:read`, `platform:write`). HTTP `createUser` writes a directory profile and ignores its password, exactly like the TypeScript iam-base mapper, so login users come from the seed or `PostgresIamService::create_user`.

`POST /auth/login` accepts `{ "email": "...", "password": "..." }` and returns a session whose `id` is the bearer credential. Send `Authorization: Bearer <id>` for the legacy `/users`, `/organizations`, permission lookup, and logout routes. `GET /users/:id` and `/users/:id/permissions` return the caller's own record; reading another user, `GET /users` and `GET /organizations` need the `platform:read` permission (403 otherwise). The legacy `/users/:id` SDK/MCP operation is `getSessionUser`. `POST /auth/logout` revokes the current session. `/health`, `/openapi.json`, `/docs` and `/api/auth/jwks` are public. Login responses disable caching.

Sessions are RS256 JWTs by default: `forklaunch init` writes a development signing key (`SESSION_JWT_PRIVATE_KEY_PEM`, base64 of the PEM on one line, git-ignored), `SESSION_JWT_KEY_ID`, and a shared `JWT_ISSUER`/`JWT_AUDIENCE` to the root `.env`, and Compose passes them to this service. The service publishes the public half at `GET /api/auth/jwks`, which is what every generated `*_JWKS_PUBLIC_KEY_URL` points at, so other services accept the same token. Generate a separate production key (for example `openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 | base64`) and keep it in your secret store. Omitting all four `SESSION_JWT_*` values selects opaque sessions that only this service can verify; partial or invalid signing configuration rejects startup. New tokens contain a unique `jti`, subject, tenant ID, issuance/expiry times, issuer/audience and a snapshot of server-resolved roles/permissions. Tokens expire after one hour and must fit the existing 4096-byte bearer limit. Email/password fields are omitted.

One token serves both route families: the legacy bearer routes look the token up as a stored session (so logout and disabling apply at once), and the directory routes verify its signature through JWKS and use its role/permission claims.

Browsers on other origins can call IAM only when listed in `CORS_ORIGINS` (comma-separated exact origins). Responses then echo the origin with credentials allowed; `*` is never sent and unlisted origins get no CORS headers.

Local session routes always check the full token's stored hash and current user
state, preserving logout, password-change revocation, disabled users and tenant
isolation. A valid signature alone cannot authenticate an unstored token there.
Changing signing configuration affects future issuance; existing stored sessions
retain their normal expiry/revocation lifecycle. Downstream stateless JWT consumers
must separately enforce current grants/revocation and tenant policy when required.
Directory identities remain separate from credential/session identities.

The `/user`, `/role`, `/permission`, and `/organization` families expose the configured TypeScript module's thirty CRUD and grant-surfacing operations through HTTP and MCP. Internal routes verify request HMAC signatures; protected routes verify asymmetric JWKS JWTs and their declared roles or permissions. User reads/updates/deletes resolve the JWT subject's organization from storage and enforce it inside each directory transaction. Sensitive fields are encrypted with tenant- and record-bound envelopes; uniqueness claims support retained encryption keys, and successful reads/writes commit actor audits atomically. Directory users are separate from credential/session users; CRUD passwords are ignored as in the configured TypeScript mapper. Provision authentication identities through the repository API.

`GET /discovery/auth-methods` is public and returns 204 for base IAM. The system-role JWT routes `/compliance/export/:userId` and `/compliance/erase/:userId` return 404 because the configured TypeScript IAM entities explicitly classify every scalar as `none`. Field encryption does not change that classification. Application-specific compliance classifications and entity extensions require a corresponding native compliance implementation.

The tenant comes from server configuration, never client headers. This scaffold serves one configured tenant. Review the configured role/access policies for your application. The optional OIDC browser flow is described below. Verification/reset flows, account-linking UI and Better Auth client compatibility remain separate work.

HTTP health and Streamable HTTP MCP share the configured listener. MCP is served at `/mcp` by default, and tools retain HTTP route authentication. `MCP_ENABLED` (true or false) is required; optionally configure `MCP_PATH`, `MCP_ALLOWED_HOSTS` and `MCP_ALLOWED_ORIGINS` for deployment.


Enable browser sessions with `SESSION_COOKIE_ORIGIN=https://app.example.com`, an
exact HTTPS origin with no path. Migrations add `fl_iam_browser_sessions`; grant
the restricted runtime role CRUD on that table. The service uses the existing
encryption key ring and tenant, with its service name as the store namespace.
Changing that name requires a fresh browser login. Omit the variable to leave
browser routes disabled; empty, non-HTTPS or malformed values reject startup.

GET `/auth/browser/csrf` establishes a Secure/HttpOnly/SameSite=Strict `__Host-iam`
cookie and returns `csrfToken`. POST `/auth/browser/login` accepts email/password
JSON plus that cookie, the `x-csrf-token` header and the exact trusted Origin.
It rotates cookie and CSRF identifiers and returns only the replacement CSRF
token. GET `/auth/browser/session` returns current subject/grants/expiry.
POST `/auth/browser/logout` requires the current CSRF proof, revokes the IAM
credential and clears the cookie. All responses are no-store. Browser endpoints
are excluded from SDK/MCP contracts; existing bearer routes still require their
Authorization header. Applications own TLS termination; cross-origin browsers need `CORS_ORIGINS`.

Cookie IDs are hashed in PostgreSQL; server-side records, including the backing
IAM credential, are encrypted and bound to tenant, namespace, record ID and
expiry. Sessions survive process restarts and support retained encryption keys.
Updates cannot resurrect deleted or expired records. Records expire after thirty
minutes without writes; the backing IAM credential keeps its one-hour maximum.
Invoke `PostgresCookieStore::purge_expired` and IAM expiry maintenance from a
trusted scheduled task. Cookie persistence and IAM writes are separate transactions;
failed persistence can leave an unreachable credential until normal expiry.


To enable one OIDC browser provider, set `SESSION_OIDC_ISSUER` and
`SESSION_OIDC_CLIENT_ID`, plus `SESSION_OIDC_CLIENT_SECRET` for a confidential
client. `SESSION_COOKIE_ORIGIN` is required. Register the exact callback
`<SESSION_COOKIE_ORIGIN>/auth/browser/oidc/callback` with the provider. Discovery
and configuration validation complete before the service listens. Apply migrations
and grant the runtime role CRUD on `fl_iam_provider_identities` and
`fl_iam_oidc_pending` as well as the browser-session table.

Provision each exact issuer/subject binding to an existing local credential user
using `PostgresIamService::bind_provider_identity` in a trusted administrative
path. Emails never auto-link accounts; provider roles do not grant local access.
Unbinding prevents new sign-ins; revoke existing local sessions separately when
required. Provider access/refresh tokens are not retained by this login flow.

After GET `/auth/browser/csrf`, POST `/auth/browser/oidc/start` with the cookie,
exact Origin and CSRF header. Navigate to the returned `authorizationUrl`. The
fixed callback serves a static page that removes code/state from the visible URL,
obtains the browser's existing CSRF proof and POSTs the callback on the same origin.
This retains the Strict cookie policy across the provider redirect. Success rotates
the cookie/CSRF proof and returns the browser to `/`; no user-selected redirect is
accepted. The callback requires JavaScript. No provider token enters browser JSON.

Pending PKCE/nonce/state data is encrypted, bound to tenant/namespace/browser,
expires within ten minutes and is atomically consumed before provider exchange.
A newer start replaces the prior pending flow for that browser. Pending flows
survive process restarts; removing the cookie row also removes its pending flow.
A rejected or interrupted exchange requires starting again.

Directory and login identities can be associated explicitly through the administrative `PostgresDirectoryStore::{bind_auth_identity,unbind_auth_identity,delete_linked_identity}` APIs. These require a trusted nonblank audit actor, never infer links from email/password/provider claims, and do not change default CRUD linking behavior. Grant the restricted runtime role SELECT/INSERT/UPDATE/DELETE on `fl_iam_directory_identities` when using them. `PostgresIamService::directory_user_for_session` resolves only complete, current persisted session tokens. See the [`forklaunch-iam-base` identity integration guide](https://crates.io/crates/forklaunch-iam-base) for atomic deletion, unlink semantics and migration details.

Set `IAM_DIRECTORY_AUTHORIZATION=stored-session` to use current linked directory grants for directory JWT routes. This requires complete `SESSION_JWT_*` signing configuration and a trusted `JWKS_PUBLIC_KEY_URL` publishing the matching public key. Provision the explicit binding and bridge-table grant first. JWT signature/issuer/audience validation still runs, then the complete token must match a current persisted session; current directory grants replace token grants before authorization. The same request snapshot supplies organization scope. Unbinding, disabling, revocation and deletion deny directory access, while grant edits take effect on the next request without reissuing the token. Default `jwt-claims` behavior, legacy credential bearer endpoints and internal HMAC routes are unchanged. Unlike the TypeScript user-keyed surfacing cache, this opt-in mode always reads current grants.

Application service extensions can use `BaseIamDirectory::unit_of_work()` and `directory::unit_of_work::DirectoryMapper` for typed custom inputs/outputs and atomic related directory operations. Explicit commit is required; errors or canceled mapper futures prevent committing partial work. Existing tenant, organization, audit and password-ignoring rules apply. These adapters target the built-in directory entities; custom SQL tables and arbitrary filters are not provided. See the bundled `forklaunch-iam-base` README for the extension boundary.

Typed directory unit-of-work filters support declared field equality/membership/null checks, logical composition and related IDs. They preserve tenant/organization restrictions and existing insertion order. Filtering decrypts and audits up to 4096 candidate rows; overflow is an explicit error, not pagination. Predicates and evaluation have fixed budgets. Arbitrary ORM filters, SQL, JSON paths and custom sorting remain unsupported; see the bundled base implementation README for exact admitted semantics.
