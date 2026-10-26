# Rust authentication candidate evaluation

Reviewed 2026-10-12 America/Los_Angeles (2026-10-13 04:55 UTC). This closes the
plan's ten **evaluation criteria**, not cookie, OAuth, passkey or provider
implementation tasks. Registry metadata and hashes of inspected release sources
are recorded in [the evidence receipt](rust-auth-candidate-evidence.json).
No candidate was added to production dependencies during this review.

Keep the existing ForkLaunch IAM contracts and domain services. Select narrow
protocol/session components behind adapters: `tower-sessions` for browser-session
transport, `openidconnect`/`oauth2` for relying-party flows, and `webauthn-rs` for
a later passkey adapter. This is a selection decision; each integration still
needs its own locked dependency audit, persistence wiring and adversarial tests.
Do not embed an entire competing application framework or replace ForkLaunch's
authorization rules with a candidate's default policy.

## 1. Maintenance and release cadence

These are observed non-yanked releases, not inferred maintenance guarantees.
Versioned package source was downloaded from the official crates.io API and
inspected without executing package code. For WebAuthn, the stable 0.5.5 source
was selected rather than the 0.6.1-dev prerelease.

| Candidate | Observed release / UTC date | Cadence and integration decision |
| --- | --- | --- |
| [loco-rs](https://crates.io/api/v1/crates/loco-rs) | 1.2.0 / 2026-09-24 | 1.0–1.2 releases July–September; reference implementation, not an embedded dependency. |
| [rs-auth](https://crates.io/api/v1/crates/rs-auth) | 0.1.2 / 2026-04-15 | Three April releases; small early API, defer adoption. |
| [auth-framework](https://crates.io/api/v1/crates/auth-framework) | 0.6.0-rc48 / 2026-10-13 | Rapid RC churn and multiple release lines; defer a wholesale integration. |
| [tower-sessions](https://crates.io/api/v1/crates/tower-sessions) | 0.15.0 / 2026-02-01 | 0.14 dated January 2025; selected narrowly with maintenance caveat below. |
| [axum-login](https://crates.io/api/v1/crates/axum-login) | 0.18.0 / 2025-07-20 | No newer registry release in this receipt; optional reference, not needed for existing IAM. |
| [axum_session_auth](https://crates.io/api/v1/crates/axum_session_auth) | 0.21.0 / 2026-08-10 | Several 2026 releases; alternative session stack, avoid two competing stacks. |
| [openidconnect](https://crates.io/api/v1/crates/openidconnect) | 4.0.1 / 2025-07-06 | Narrow protocol library; selected for OIDC relying-party support. |
| [oauth2](https://crates.io/api/v1/crates/oauth2) | 5.0.0 / 2025-01-21 | Selected lower-level OAuth client and OIDC dependency. |
| [oauth-axum](https://crates.io/api/v1/crates/oauth-axum) | 0.1.4 / 2024-12-18 | Wrapper with limited release activity; use underlying protocol crate directly. |
| [webgates](https://crates.io/api/v1/crates/webgates) | 1.2.0 / 2026-08-15 | 1.0–1.2 released May–August; optional reference, no default adoption. |
| [authkestra](https://crates.io/api/v1/crates/authkestra) | 0.13.1 / 2026-09-26 | Several September minor releases; promising modular design, defer integration. |
| [webauthn-rs](https://crates.io/api/v1/crates/webauthn-rs) | 0.5.5 stable / 2026-04-30 | Stable and development lines coexist; selected stable line for later review. |

The tower-sessions/axum-login maintainer requested help taking over maintenance
on July 12, explicitly saying the projects were not abandoned. Treat this as a
continuity risk, not proof of abandonment. Keep our session-store adapter small
and replaceable. [Maintainer announcement](https://github.com/maxcountryman/axum-login/discussions/330)

## 2. Security posture and dependency risk

The existing [ForkLaunch security gate](RUST-SECURITY-REVIEW.md) audits our actual
three lockfiles. It does not certify unused candidates or every optional feature.
Before adding a selected component, resolve only required features in our
workspace, inspect the resulting dependency tree, run the same advisory gate,
and review cryptographic/provider configuration. An upstream claim of an audit
or production readiness is not independently established by this assessment.

Concrete source findings supporting the selection:

- rs-auth 0.1.2 defaults to implicit account linking and does not require email
  verification for password login. Its Google user-info model does not retain
  `email_verified`, while the service marks OAuth users verified. Its callback
  loads state and deletes it in separate store operations. These are blockers
  to reuse without tests for verified-provider identity, browser binding and
  atomic consumption; this review does not claim an exercised exploit.
  [Config](https://docs.rs/crate/rs-auth-core/0.1.2/source/src/config.rs),
  [Google mapping](https://docs.rs/crate/rs-auth-core/0.1.2/source/src/oauth/google.rs),
  [Service](https://docs.rs/crate/rs-auth-core/0.1.2/source/src/service.rs),
  [Callback](https://docs.rs/crate/rs-auth-axum/0.1.2/source/src/handlers/oauth.rs)
- auth-framework's reviewed RC defaults enable enhanced RBAC and PostgreSQL;
  enhanced RBAC enables its API server. That expands the integration boundary
  beyond the missing ForkLaunch components. Broad feature and compliance claims
  remain upstream claims; we have not validated its full default/optional graph.
  [Published manifest](https://docs.rs/crate/auth-framework/0.6.0-rc48/source/Cargo.toml)
- tower-sessions documents lost-update risks for concurrent session mutation.
  Do not put replay counters, one-time OAuth consumption or authoritative
  revocation state in a load/modify/save JSON session map. Use atomic store
  operations for those invariants. [Session documentation](https://docs.rs/tower-sessions/0.15.0/tower_sessions/)
- oauth-axum describes adding tests as future work in its published README.
  A provider preset wrapper supplies no reason to replace the protocol library
  and explicit ForkLaunch flow tests. [Published README](https://docs.rs/crate/oauth-axum/0.1.4/source/README.md)

## 3. Password hashing defaults and upgrades

Loco 1.2.0 explicitly selects Argon2id/version 19 with library-default parameters;
rs-auth 0.1.2 uses Argon2 defaults. Both create random salts and parse PHC strings
for verification. Their inspected hash helpers do not perform persistent
rehashing. auth-framework's inspected helper uses Argon2id with 46 MiB, one
iteration/one lane and a 128-byte password bound. These observations describe
code, not a recommendation to copy one fixed work factor.
[Loco helper](https://docs.rs/crate/loco-rs/1.2.0/source/src/hash.rs),
[rs-auth helper](https://docs.rs/crate/rs-auth-core/0.1.2/source/src/crypto/hash.rs),
[auth-framework helper](https://docs.rs/crate/auth-framework/0.6.0-rc48/source/src/utils/password.rs)

Keep ForkLaunch's salted Argon2id/PHC domain boundary. Hashing costs must be
benchmarked under concurrent login load. A future upgrade policy should verify
an old PHC hash, compare its algorithm/work factors with the server policy, then
replace it atomically only after successful authentication. Neither importing a
session crate nor selecting a password helper supplies that persistence policy.
Session/protocol libraries do not choose our password or account-recovery rules.

## 4. Session storage and CSRF

`tower-sessions` supplies a pluggable `SessionStore`, expiry and ID lifecycle.
Its published default cookie configuration is Secure, HttpOnly and SameSite
Strict. Keep a server-side store and a cookie containing only an opaque ID;
rotate on login/privilege changes and invalidate on logout/reset. Configure the
cross-site OIDC callback cookie separately rather than weakening all session
cookies. [Source](https://docs.rs/crate/tower-sessions/0.15.0/source/src/service.rs)

SameSite is not our complete CSRF policy. Unsafe browser-authenticated methods
need validated Origin/Referer policy and a session-bound CSRF token, with no
state change through GET. Prove rotation, expiry, revocation, cookie deletion,
concurrent logout and multi-replica behavior through real HTTP and MCP adapters.
These are integration acceptance conditions, still open in the implementation
plan. Browser-only sessions must not silently become bearer credentials.

`axum-login` permits custom user/authentication/authorization backends and binds
sessions to `session_auth_hash`; changing a password hash can invalidate a
session. It still requires application-owned credential verification and policy.
`axum_session_auth` uses a different session stack with cached user data and
cookie/header modes; that would add a second revocation/cache model.
[axum-login](https://docs.rs/axum-login/0.18.0/axum_login/),
[axum_session_auth](https://docs.rs/axum_session_auth/0.21.0/axum_session_auth/)

## 5. OAuth/OIDC provider coverage

Choose openidconnect for provider discovery, verified ID tokens and relying-party
flows; use oauth2 for OAuth-only providers and token exchange. The official
examples include PKCE, state and nonce verification, and warn against following
HTTP redirects because of SSRF. OAuth2 also documents timing-resistant secret
comparison. The ForkLaunch adapter must bind single-use state to the initiating
browser/session and tenant, enforce expiry, constrain issuer/redirect URLs and
verify audience/issuer/nonce before creating an identity.
[OIDC API](https://docs.rs/openidconnect/4.0.1/openidconnect/),
[OAuth2 API](https://docs.rs/oauth2/5.0.0/oauth2/)

Provider configurations are not interchangeable: test Google OIDC and GitHub
OAuth separately, including refresh, linking/unlinking and provider errors.
The rs-auth release supplies Google/GitHub presets; oauth-axum supplies several
presets but leaves transient state storage to callers. Neither makes provider
HTTP results equivalent to trusted local role or tenant assignments.
[rs-auth](https://github.com/rs-auth/rs-auth),
[oauth-axum](https://docs.rs/oauth-axum/0.1.4/oauth_axum/)

This selects client/relying-party functionality. Running a general OAuth/OIDC
identity-provider server is a separate domain; neither the name `openidconnect`
nor an OAuth client integration completes that server role.

## 6. Multi-tenant organizations

None of the selected narrow libraries supplies the complete ForkLaunch directory,
organization membership, tenant isolation and encrypted persistence contract.
Keep those domain/store traits. Key external identities by trusted issuer/provider
and subject within a tenant; do not infer membership from a matching email.
Organization membership must be checked again at the HTTP/MCP authorization
boundary, including after session refresh and membership revocation.

The higher-level candidates expose accounts, groups, permissions or storage
traits; that is not evidence of our tenant-filter semantics. In particular,
a group label in webgates must not be treated as a database tenant isolation
mechanism. Its documented policy alternatives use OR semantics.
[Webgates policy API](https://docs.rs/webgates/1.0.0/webgates/authz/access_policy/struct.AccessPolicy.html)

## 7. Roles, permissions, scopes and feature entitlements

Use candidate backends to establish identity, then map verified claims into
`forklaunch_auth::Principal` and run our existing `authorize` function. ForkLaunch
combines separate role, permission, scope, feature and subscription conditions;
forbidden roles/permissions also deny access. A generic role hierarchy or OR
policy is not equivalent. Do not map arbitrary provider claims into entitlements
without an explicit server-side mapping. Preserve role/tenant revocation and
subscription changes across cached sessions.
[Local authorization implementation](../framework-rs/crates/forklaunch-auth/src/lib.rs)

axum-login's user/group permission trait is adaptable, but its route macros
would duplicate our contract authorization middleware. Webgates is a useful
policy-design reference, not a replacement policy engine for this port.

## 8. WebAuthn/passkeys

Select webauthn-rs's stable API for a future optional adapter. The project warns
that registration/authentication challenge state must be held server-side; its
serialization escape hatch can enable replay if used as client-controlled state.
Use an expiring, atomically consumed server record, fixed RP ID/origin policy,
credential ownership checks and counter updates. Test replay, wrong origin/RP,
wrong user, missing user verification and recovery/enrollment authorization.
[WebAuthn documentation](https://docs.rs/webauthn-rs/0.5.5/webauthn_rs/)

Authkestra advertises WebAuthn/TOTP and fresh-proof requirements, while
its published facade delegates implementation to feature-gated crates. That is
a reason to revisit its engine later, not evidence that ForkLaunch passkey
persistence/recovery/client contracts are implemented.
[Published facade README](https://docs.rs/crate/authkestra/0.13.1/source/README.md)

## 9. Independence from Axum

Keep OIDC/OAuth verification and IAM policy in framework-independent domain
services. openidconnect/oauth2 expose HTTP-client abstractions rather than Axum
handlers. tower-sessions belongs at the Tower transport boundary. axum-login and
axum_session_auth are explicitly Axum-oriented. Loco brings its application and
ORM conventions; embedding it would duplicate our framework lifecycle.
[Loco project](https://github.com/loco-rs/loco)

rs-auth separates core/store/Axum crates; webgates separates domain and adapters;
authkestra separates engine, Axum and Actix adapters and leaves account ownership
to applications. These are promising boundaries, but importing their facades
would still require translating stores, identity semantics and errors.
[rs-auth core](https://docs.rs/rs-auth-core/0.1.2/rs_auth_core/),
[Webgates manifest](https://docs.rs/crate/webgates/1.2.0/source/Cargo.toml),
[Authkestra facade](https://docs.rs/crate/authkestra/0.13.1/source/README.md)

## 10. OpenAPI, SDK and MCP boundaries

Adapters must register ForkLaunch route contracts and retain the shared verified
session/principal extension. Document cookies/headers as security schemes,
expose typed public DTOs, and ensure generated clients never embed server secrets.
Keep issuer keys, provider refresh tokens, password hashes and CSRF storage out
of SDK artifacts and logs. HTTP, MCP tool dispatch and WebSocket upgrades must
run the same authorization decision after authentication.

No reviewed candidate demonstrated a drop-in replacement for our contract IR,
OpenAPI/AsyncAPI generators, both SDK languages and MCP forwarding semantics.
That is an integration assessment against our local architecture, not a claim
that those projects cannot support adapters. Acceptance requires generated-module
init/change/sync/delete, actual HTTP/MCP startup, auth failure paths and client
roundtrips. The corresponding implementation checklist items remain open.

## Decision record

Proceed with narrow, optional session and OIDC adapters; preserve the existing
Argon2id/JWT/HMAC and IAM domain. Retain webauthn-rs as the passkey candidate.
Defer rs-auth, auth-framework, webgates and authkestra adoption, and avoid layering
axum-login/axum_session_auth over an already-owned IAM session model by default.
Revisit a candidate only for a concrete missing capability, with a pinned feature
graph and evidence that it preserves the ten criteria above.


## Selected adapter integration receipt — 2026-10-13

The selected narrow adapters are now implemented behind explicit optional
features: `forklaunch-axum/cookie-session` uses tower-sessions 0.15, and
`forklaunch-auth/oidc` uses openidconnect 4.0.1/oauth2. Their locked dependency
graphs passed the source-specific advisory review with no unreviewed findings.
The RSA exception is limited to public OIDC verification; it does not authorize
private-key operations.

Encrypted PostgreSQL cookie state and browser-bound, atomically consumed OIDC
pending state pass real provider, rotation/revocation, process-restart, CSRF and
HTTP/MCP fixtures. Chromium proves Strict-cookie cross-site exclusion and the
same-origin callback relay. Generated base IAM run `5zxkME` passes end-to-end
provider code/PKCE exchange and local grants. Detailed receipts and limitations
are in the parity inventory and security review.

This completes the recommendation to integrate selected optional adapters only
after review. It does not approve deferred whole-framework dependencies, implement
passkeys, or close broader provider/linking/client options. Those remain distinct
IAM parity work.

## Better Auth capability and evidence matrix — 2026-10-14

This additive audit is anchored at rotation checkpoint `1f2f65f5a`. It does not
close gate 918 or replace the broader user request for Better Auth parity. The
candidate-library assessment above and the native Better Auth implementation
are distinct workstreams. Historical statements above about future integrations
must be read with their dated receipts, not as the current implementation list.

The actual ForkLaunch configuration is
[`betterAuthConfig`](../blueprint/iam-better-auth/auth.ts). It returns a concrete
configuration checked with `satisfies BetterAuthOptions`; it is not a factory
accepting every upstream option. `BetterAuthConfig` is its exported return type.
[`index.ts`](../blueprint/iam-better-auth/index.ts) exports that type, the IAM SDK,
and surfacing symbols. The service injection registration is named **BetterAuth**
in [`registrations.ts`](../blueprint/iam-better-auth/registrations.ts), which
constructs `betterAuth(betterAuthConfig(...))`. No separate exported `AuthService`
symbol was found in this module; the registered instance exposes the upstream
configured `.api` helpers. This distinction matters when enumerating a finite
public compatibility contract.

The matrix distinguishes three scopes: **configured** behavior used by this
ForkLaunch preset, **public extension** behavior represented by exported
configuration/types or native trusted builders, and **broader library** features
not installed by the preset. An existing symbol alone is not evidence of parity.
The linked fixtures identify the exercised subset; unlisted combinations and
failure paths are not implicitly covered. This audit performs no new runtime or
Cargo validation. Prior execution receipts remain in the parity inventory and
security review, including working-tree versus immutable-archive provenance.

| Capability and scope | Actual source/API | Native implementation and exercised evidence | Concrete remaining boundary |
| --- | --- | --- | --- |
| Configuration, DI and environment — configured | `betterAuthConfig`, registered `BetterAuth`; base URL/path, origins, Google environment credentials in [auth.ts](../blueprint/iam-better-auth/auth.ts) and [registrations.ts](../blueprint/iam-better-auth/registrations.ts) | [AuthHttp](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/http.rs) plus [generated preset](../cli/src/templates/rust/iam-better-auth/main.rs); [runtime harness](../cli/tests/rust_better_auth_runtime.sh) executes generated startup, HTTP/MCP, persistence and lifecycle | General `BetterAuthOptions` object replacement is not a Rust object-shape compatibility promise. Each supported native builder needs a declared behavioral contract. |
| Credentials, user fields and sessions — configured | Email/password enabled; required first/last name, optional phone; 24-hour session and one-hour update age in [auth.ts](../blueprint/iam-better-auth/auth.ts) | Native [accounts](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/accounts.rs), [sessions](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/sessions.rs); actual [credential](../cli/tests/rust_better_auth_credential_validation.mjs), [profile](../cli/tests/rust_better_auth_profiles.mjs), [session](../cli/tests/rust_better_auth_sessions.mjs), [account](../cli/tests/rust_better_auth_accounts.mjs) and [fault](../cli/tests/rust_better_auth_fault_boundaries.mjs) fixtures | Tested routes do not establish arbitrary custom user schemas, every database failure, or unrestricted lifecycle hooks. Signout cookie clearing under storage failure does not establish persisted revocation. |
| Origin, cookie and rate policy — configured defaults plus typed extensions | Upstream guards reached by configured auth handlers; client supplies its API origin outside browsers | [Origins](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/origins.rs), [cookies](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/cookies.rs), [rate options](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/http/rate.rs); actual [origin](../cli/tests/rust_better_auth_origins.mjs), [rate](../cli/tests/rust_better_auth_rate_limits.mjs) and [redirect](../cli/tests/rust_better_auth_redirects.mjs) cases | These are bounded supported policies, not all upstream proxy/cookie/cache/storage configurations. |
| Google OAuth and provider-account operations — configured | Google credentials, redirect URI and first/last-name mapping in [auth.ts](../blueprint/iam-better-auth/auth.ts) | [OAuth module](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/oauth/mod.rs); actual [authorize](../cli/tests/rust_better_auth_google_authorize.mjs), [callback](../cli/tests/rust_better_auth_google_callback.mjs), [verification](../cli/tests/rust_better_auth_google_verify.mjs), [tokens](../cli/tests/rust_better_auth_google_tokens.mjs) and [HTTP](../cli/tests/rust_better_auth_google_http.mjs) fixtures | Owned provider fixtures prove protocol/handler behavior, not a live Google deployment. Other providers and arbitrary provider profile callbacks are not inferred from Google support. |
| Organizations, teams, memberships, invitations and dynamic roles — configured plugin | Organization creation enabled, owner creator, membership limit 100, 48-hour invitations, teams/dynamic access enabled, declared role statements and organization fields in [auth.ts](../blueprint/iam-better-auth/auth.ts) | [Organizations](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/organizations.rs); actual [organizations](../cli/tests/rust_better_auth_organizations.mjs), [members](../cli/tests/rust_better_auth_members.mjs), [teams](../cli/tests/rust_better_auth_teams.mjs), [invitations](../cli/tests/rust_better_auth_invitations.mjs), [role creation](../cli/tests/rust_better_auth_role_creation.mjs), [role updates](../cli/tests/rust_better_auth_role_updates.mjs), [permissions](../cli/tests/rust_better_auth_organization_permissions.mjs) fixtures | Configured operations have individual fixtures; arbitrary additional organization/team models, custom policy callbacks and every plugin option remain unproven. |
| Current-session role/permission surfacing — configured ForkLaunch extension | Session/user/organization scope derived by [authorizationScope.util.ts](../blueprint/iam-better-auth/domain/utils/authorizationScope.util.ts); current-grant services registered in [registrations.ts](../blueprint/iam-better-auth/registrations.ts) | [Surfacing](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/surfacing.rs); actual [surfacing](../cli/tests/rust_better_auth_surfacing.mjs), [application contracts](../cli/tests/rust_better_auth_application_contracts.mjs), real [ORM](../cli/tests/rust_better_auth_orm.mjs) and generated runtime | Exact session scope and UUID-versus-text identity cases are exercised. This is not arbitrary upstream access-control plugin equivalence. |
| Encrypted ORM and indexed membership queries — configured | [Adapter wrapper](../blueprint/iam-better-auth/domain/utils/betterAuthAdapter.util.ts), [encryption policy](../blueprint/iam-better-auth/domain/utils/authEncryptionPolicy.util.ts), service encryption registration | Native [store contract](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/store/mod.rs), [PostgreSQL store](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/store/postgres.rs), [member query profile](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/store/postgres/member_query.rs); real [ORM](../cli/tests/rust_better_auth_orm.mjs) and [member ORM](../cli/tests/rust_better_auth_member_orm.mjs) comparisons | Ordinary member pages use indexed count/window reads. Filtered/sorted profile is bounded to 4096 candidates/1 MiB and transient public-field SQL parameters; database log policy matters. Generic arbitrary adapter models/filters/hooks are not complete. |
| Compliance and retention — configured ForkLaunch services | IAM registrations and exported surfacing/compliance contracts | [Compliance](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/compliance.rs), [retention](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/retention.rs); actual [compliance](../cli/tests/rust_better_auth_compliance.mjs), [retention](../cli/tests/rust_better_auth_retention.mjs), generated HTTP/MCP checks | These cover the configured classified entities and retention policy, not third-party plugin tables. |
| JWT preset claims and trusted payload/subject hooks — configured plus public extensions | Configured `jwt.definePayload` emits user/session/email/organization fields; upstream JWT helpers add subject/defaults | [Payload resolver](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/http/jwt_payload.rs), subject handling via AuthHttp; actual [payload](../cli/tests/rust_better_auth_jwt_payload.mjs) and [subject](../cli/tests/rust_better_auth_jwt_subject.mjs) JOSE/local/external cases | Typed payload callback returns optional object maps. Arbitrary primitive/array JavaScript spread, prototypes, nonfinite numbers and lone-surrogate strings are not covered by the JSON-value profile. |
| JWT lifetime, absolute Date/numeric expiry and header suppression — public extensions | Upstream `jwt.expirationTime` and `disableSettingJwtHeader` options | [Expiration builders](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/http/jwt_expiration.rs), AuthHttp header guard; actual [Date](../cli/tests/rust_better_auth_jwt_expiration.mjs), [numeric](../cli/tests/rust_better_auth_jwt_numeric_expiration.mjs), [duration](../cli/tests/rust_better_auth_jwt_lifetime.mjs), [header](../cli/tests/rust_better_auth_jwt_header.mjs) fixtures | Typed duration/Date/numeric subsets are explicit; arbitrary JavaScript option types and complete string-duration grammar are not implied. Cookie/session identity and expiry remain independent. |
| Local key algorithms, legacy metadata, expiry and publication grace — public extensions | Upstream JWT `keyPairConfig`, `rotationInterval`, `gracePeriod`; default preset uses plugin defaults | [KeyRing](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/keyring.rs), [KeyPairConfig](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/keyring/config.rs); actual [keyring](../cli/tests/rust_better_auth_keyring.mjs), [legacy JOSE](../cli/tests/rust_better_auth_jwks_legacy.mjs), [rotation](../cli/tests/rust_better_auth_jwks_rotation.mjs), [grace](../cli/tests/rust_better_auth_jwks_grace.mjs); native [HTTP/rotation matrix](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/keyring/config/rotation_tests.rs) | Native supported algorithms/RSA sizes and whole-millisecond nonnegative intervals are bounded. Legacy native PKCS#8 normalization is read-only and rejects conflicting public metadata; it is not arbitrary Better Auth encrypted JWK import. Expiry causes lazy replacement, not timers or cookie/JWT expiry changes. |
| Custom JWKS repositories and public-only records — public extensions | Upstream JWT adapter `getJwks`/`createJwk` | [Typed adapter](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/keyring/adapter.rs), [repository session](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/keyring/repository/session.rs); actual [adapter oracle](../cli/tests/rust_better_auth_jwks_adapter.mjs) and native adapter tests | Fixed trusted tenant context must match the backend. Custom owners provide encryption, persistence and synchronization; mixed stores are not atomic. Public-only reads cannot sign. Public-only create returns and arbitrary JavaScript adapter objects remain outside the typed create API. |
| Explicit signing constraints and custom signing — public extensions | Installed upstream `signJWT`/`resolveSigningKey` accept `signingKeyId`, `signingAlgorithm`, `resolvedKey` and header metadata | [Trusted ID pin](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/keyring/sign.rs), [custom signer](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/keyring/custom.rs), [external HTTP signer](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/http/external_jwt.rs); actual [pin/adapter](../cli/tests/rust_better_auth_jwks_adapter.mjs), [custom](../cli/tests/rust_better_auth_custom_jwt.mjs), [external](../cli/tests/rust_better_auth_external_jwt.mjs) fixtures | ID pin lookup/no-fallback is tested. Trusted algorithm selection and bounded additional-profile lazy provisioning now have10 actual JOSE/five native cases and105 IAM tests. Existing matching unadvertised keys remain usable; trusted create callbacks retain returned-algorithm authority. Bounded external header/pin context now has six actual cases plus custom/HTTP native regressions; legacy signers intentionally ignore it, context-aware wrappers forward it. Opaque caller-resolved private-key reuse now has actual JOSE/native proof with zero storage/mint callbacks; expiry/removal is deliberately not rechecked. Arbitrary raw resolved-key/header options remain outside this typed capability. |
| Error hooks and diagnostics — configured plus compatibility extension | Configured `onAPIError.onError` calls [logBetterAuthApiError](../blueprint/iam-better-auth/domain/utils/betterAuthErrorLogging.util.ts); collector also receives upstream logger events | Native [observer](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/http/observer.rs), private error provenance; actual [observer](../cli/tests/rust_better_auth_error_observer.mjs) and [fault boundaries](../cli/tests/rust_better_auth_fault_boundaries.mjs) | Unexpected-only callback dispatch is tested for identified catches; all-5xx compatibility callback remains optional. Historical library tracing still logs all server errors; generated Configured tracing now distinguishes unexpected versus tagged internal catches with isolated count proof. Uncharacterized route/plugin event counts remain open. Native diagnostics intentionally omit freeform message/stack and use narrower structural metadata than the TS logger. Do not claim logging-byte parity or expand secret-bearing logs to obtain it. |
| OpenAPI, auth namespaces and callback helpers — configured | `openAPI({disableDefaultReference:true})`, registered Scalar source, custom callback helpers | [Application wiring](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/http/application.rs), [callbacks](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/http/callback.rs); actual [public](../cli/tests/rust_better_auth_public.mjs), [callback](../cli/tests/rust_better_auth_callbacks.mjs), [JWKS path](../cli/tests/rust_better_auth_jwks_path.mjs), generated runtime fixtures | Default/custom/root fixed auth mounts and bounded JWKS paths are exercised. Dynamic/encoded namespaces and unsupported root-auth/root-JWKS collision cases need separate actual routing evidence; no broad catch-all bypass is justified. Raw auth routes remain outside registered MCP tools. |
| Shared/generated client SDK — configured exported factory | [clientIamBetterAuthSdkClient](../blueprint/client-sdk/clientSdk.ts) and [CLI client template](../cli/src/templates/project/client-sdk/clientSdk.ts): `host`, `registryOptions`, optional `authBasePath`/`coreHost`, `createAuthClient`, origin header, `inferAdditionalFields<BetterAuthConfig>` | [Actual frontend client fixture](../framework-rs/parity/fixtures/rust_better_auth_frontend_client.mjs) invoked by [native generated runtime](../cli/tests/rust_better_auth_runtime.mjs), plus independent generated SDK compilation/runtime checks | Actual universal SDK, AJV and auth clients now execute against native HTTP: eight shared/generated client runs and112 actions, plus21 standalone HTTP requests per factory. DNS forwarding, fetch forwarding and Node cookie jar remain owned. Separate MCP probes pass; HTTP clients are not MCP clients. Bounded referenced date/binary coercion now has18 HTTP and35 SSE cases; arbitrary schema composition and broader fetch/client hook/plugin combinations remain unproven. |
| Opt-in fixed TOTP, backup codes and trusted devices — broader library | Actual Better Auth twoFactor plugin default password-login profile, separately enabled from the configured preset | [Native factor service](../blueprint-rs/crates/implementations/iam/forklaunch-iam-better-auth/src/two_factor/mod.rs), [actual reference](../cli/tests/rust_better_auth_two_factor.mjs), [actual-client/native bridge](../cli/tests/rust_better_auth_two_factor_native.mjs); restricted PG, full IAM117 and strict Clippy | Implemented fixed SHA1/6digit/30s lifecycle with encrypted factors, challenges, lockout, recovery and rotating trust. Default off; social callbacks/existing sessions independent. Bridge20 passes; generated opt-in realTCP client20 and HTTP/MCP discovery now pass alongside existing112 frontend actions. Owned cookie jars do not prove browser enforcement. Backup disableSession now passes actual-client/native29 and full IAM118; immutable416d archive-installed CLI also passes generated29 actual-client calls over real TCP, alongside112 frontend actions and HTTP/MCP lifecycle. Trusted fixed OTP delivery now passes actual-client21, full IAM120, final focused2 and restricted PG55.84s, with async callback failure and atomic consumption proofs. This is a handler bridge, not TCP/browser delivery. Bounded trusted OTP digit/validity/attempt options and no-hook enrollment ordering now pass focused native and actual-source checks. Trusted TOTP six/eight-digit and bounded period options now pass16 native tests and96 actual vectors. Trusted lifecycle flags, independent issuers and bounded passwordless policy now pass19 native tests, restricted PG and actual lifecycle checks. Transactional credential rechecks and atomic skip-enrollment rotation are explicit. Bounded whole-second challenge/trust lifetimes now pass actual clock/cookie checks, native22 and restricted PG, including inclusive challenge versus strict trust expiry and atomic renewal rollback. Trusted bounded account-lockout policy now passes actual helper/route checks and restricted PG, including mixed factors, disabled zero-write success and transactional failure budgets. Bounded backup generation/count/length and trusted synchronous generator now pass actual source, native26 and restricted PG, preserving duplicate/empty/Unicode proofs and atomic rollback. Trusted Plain/Hashed OTP field storage now passes actual policy-switch/error checks, native29 and restricted PG, independently of row encryption. Built-in encrypted OTP and bounded versioned field keys now pass5 Node/Rust vectors, native35 and restricted PG; malformed decrypt commits proof consumption before empty500. Bounded async custom OTP codecs now pass actual10, native14, restricted PG and HTTP clock-preservation checks; durable leases reject stale-authority races without holding DB transactions across callbacks. Built-in legacy-array/plain-JSON/encrypted backup field codecs and independent versioned keys now pass4 Node/Rust vectors, final5/affected54 native tests and restricted PG, with first HTTP MFA opt-in encrypted and authorized legacy migration. Trusted async backup codecs now pass actual12, focused11/fullIAM178 and restrictedPG, with durable phase leases, exact current authority/factor CAS and cancellation cleanup; native stale-authority safety deliberately exceeds upstream. Trusted server-only backup viewing now passes26 backup regressions (6new),14 actual-source observations and restricted read-only PostgreSQL checks, with final generation/lease rechecks and bounded async decode. Caller authorizes userId; no HTTP/MCP/client route or read-time migration. Optional trusted OTP background dispatch now passes3focused/36affected native and5actual checks with strict Clippy, preserving default awaiting and committed-proof error behavior. Owned lazy tasks require application scheduling/draining; Trusted request-password-reset delivery and reusable app-owned scheduler now pass9actual cases, focused reset/scheduler and restrictedPG consume/expiry/replay proofs; existing OTP task/dispatcher/future names remain aliases with explicit OTP override precedence. Default reset delivery stays disabled, Explicit verification-email delivery now passes11actual/3focused/restrictedPG and existing liveHTTP/MCP proofs; it always awaits callbacks independently of background scheduling, signs compatible1h email-address tokens and preserves manual disabled default. Configurable TTL/custom projection, signup/signin verification policy and broader background/email/invitation plugins remain open. |
| Features disabled by this preset versus optional upstream plugins — broader library | Installed preset plugins are JWT, OpenAPI and organization; Google is the configured social provider. Shared client installs additional-field inference only | [Configured-disabled fixture](../cli/tests/rust_better_auth_configured_disabled.mjs) exercises disabled behavior; actual preset source defines the enabled surface | Passkeys, MFA, admin, SSO/OIDC-provider and arbitrary third-party plugins must not be counted as implemented merely because Better Auth supports them. They remain part of broader requested feature assessment or separate IAM gates; identifying them as unconfigured does not silently remove the user's parity objective. |

### Finite follow-up criteria for gate 918

1. **Completed bounded signing extension contract.** Algorithm-only pins and
   advertised extra profiles, trusted callback algorithm authority, external
   header/pin context and opaque resolved-key reuse have the actual JOSE/native
   receipts listed above. Arbitrary raw resolved-key/header shapes remain outside
   this typed capability; this does not close the broader adapter/options gate.
2. **Finish the configured diagnostics/fault matrix.** Enumerate operation/catch
   boundaries for the currently configured auth endpoints, then compare status,
   body, cookie mutation, transaction outcome and observer dispatch. Record the
   deliberate sanitized diagnostic representation separately from missing event
   semantics. Existing signup/signout/session/permission receipts are not a proof
   for every route or every database failure.
3. **Characterize unsupported namespace combinations.** Run actual TS HTTP
   dispatch for encoded/dynamic paths and collisions before deciding which can
   be represented safely by exact native mounts. Explicit rejection remains a
   documented gap, not successful parity.
4. **Extend the bounded exported client composition proof.** The actual shared
   and generated mixed factories now pass112 native HTTP actions across default,
   custom/root and URL-host auth paths, preserving cookies, origin, explicit core
   host and additional fields. Per-document response validation is exercised;
   referenced special-value conversion and broader fetch/hooks/plugins remain
   separate work. MCP startup is probed independently of the HTTP SDK clients.
5. **Audit concrete configured persistence extension points.** Link each exposed
   service-context/transaction/mapper option to its actual ORM/native comparison.
   Distinguish supported configured entities and indexed queries from arbitrary
   custom schemas and adapter operations. A generic public trait is not itself
   evidence of behavioral equivalence.

Gate 918 stays open until these remaining capabilities and any additional
explicitly requested Better Auth features are implemented and validated, or the
user explicitly approves a narrower scope. This table does not convert
unsupported behavior into completed work and introduces no new percentage or
checklist closure.


### Trusted signup/signin verification policy (2026-10-14)

Optional EmailVerificationPolicy controls require/sendOnSignup/sendOnSignin;
manual delivery hook alone does not enable automatic sending. Required signup
returns token:null and no session, with generic submitted-profile duplicate
responses; required signin gates correct-password unverified users before MFA,
trust and session creation. Require without hook is supported. Default atomic
signup behavior remains unchanged. SignupOutcome is additive; legacy sign_up
fails closed before effects when required policy cannot return an AuthGrant.
Delivery preparation/scheduling runs outside DB transactions. Callback receives a
prospective user before persistence, unlike upstream transaction visibility;
mail can precede failed commit and concurrent duplicates can send before a winner.
Writer rechecks and atomic persistence prevent partial account/session state.
No claim of complete callback equivalence, autoSignIn, social or all hooks parity.
Actual13, focused3 including8HTTP configs, restrictedPG rollback/reentry/races,
fullIAM198pass/3ignored and strict full/minimal Clippy pass. Frozen11 manifest
/private/tmp/fl-email-policy-freeze.json verified. Logs /private/tmp/fl-email-policy-
{actual,native,pg,full,clippy,minimal-clippy}.log. Existing metadata already admits
nullable signup token and403; no new Cookie/JWT on denied paths. Broad660/682
(96.8%),22open unchanged.


### Independent signup auto-signin option (2026-10-14)

Trusted with_signup_auto_sign_in(false) on AuthHttp/CredentialService suppresses
signup sessions independently of verification requirements; defaulttrue preserves
existing behavior. New SignupOutcome::Unauthenticated returns token:null/no cookie,
with generic duplicate submitted-profile privacy; later signin remains enabled.
Verification sending fallback still depends on require, not auto-signin. Synthetic
configured public output fills absent image/phoneNumber:null. Legacy sign_up fails
before effects when no AuthGrant can be issued. New enum variant requires exhaustive
consumer matches to adapt; existing variants/default methods remain available.
Actual5,focused3(43.92s),affectedpolicy5(53.27s),restrictedPG1(83.36s) and strict
allfeature/alltarget+minimal Clippy pass. Concurrent duplicates yield one account,
zero sessions; owned roles/records/audits0. Frozen9 manifest
/private/tmp/fl-signup-auto-signin-freeze.json verified (manifest SHA256
f08f68d890ab363fd0e813b4c01f24575baa55e62870e45736171d278e08253d).
Logs /private/tmp/fl-signup-auto-signin-{actual,native,regression,pg,clippy,
minimal-clippy}.log. Existing nullable token metadata adequate; prospective-user
callback boundary and broaderhooks remain as previously documented. No fullIAM
rerun claimed for this slice (affected5 and focused3 used).660/682(96.8%),22open.


### Optional post-reset session revocation (2026-10-14)

Defaultfalse with_revoke_sessions_on_password_reset is available on trusted
CredentialService/AuthHttp. Enabled reset first commits password update and
consumed proof, then deletes all current user's sessions (including expired)
through tenant-bound indexed256-row cursor pages in a separate writer transaction.
Foreign users remain untouched;513-session test proves pagination. No cookie
clearing is invented. A second-delete fault rolls back earlier session deletions
while the changed password/consumed proof remain committed, matching upstream's
observable failure boundary. Native revocation batch is transactional; no claim
about arbitrary source adapter partial deletes or onPasswordReset hooks.
Actual4,focused3(18.68s),affectedreset8(18.70s),restrictedPG1(99.67s),strictfull
allfeature/alltarget18.08s+minimal3.08s pass. Ownedroles/records/audits0.
Frozen10 /private/tmp/fl-reset-revocation-freeze.json verified (manifest SHA256
3f2bdb242ddaa60b77ec07e6fcce790f3204940ce400a38dc46b3b97ed32af5b).
Logs /private/tmp/fl-reset-revocation-{actual,native,regression,pg,clippy,
minimal-clippy}.log. No schema/dependency changes. Further trusted hooks and
provider lifecycle remain open;660/682(96.8%),22broad gates unchanged.


### Trusted post-password-reset observer (2026-10-14)

with_password_reset_observer implements the bounded onPasswordReset subset:
prepare and owned future are always awaited after committed password/proof state,
before optional session revocation, outside DB transactions. Background dispatcher
is bypassed. Sync/future/projection failures return sanitized empty500 and preserve
sessions. Reentrant callback can use the new password; its newly created session
is subsequently revoked when enabled. Native cancellation may leave revocation
unperformed with committed password/consumed proof; no JavaScript cancellation
semantics claim. Blocking callbacks/panics are not intercepted.
Opaque context has only bounded zeroized public user_json, no Debug/Serde/password/
token/session/raw Request. Projection validates before cloning/serialization;
context snapshot captured before password update and projection error delayed
until after commit. No-hook path skips projection. Arbitrary user fields and
upstream optional Request are outside this supported subset.
Actual6,focused3(34.43s),restrictedPG1(135.28s),affectedreset11(34.57s),strict
allfeature/alltarget17.62s+minimal3.84s pass. Ownedroles/records/audits0.
Frozen11 /private/tmp/fl-reset-observer-freeze.json verified, SHA256
ee2b9da2af7c5db832dcaf2e02752d5afca1317f8d7a6278e49ed85b9fb79e97.
Logs /private/tmp/fl-reset-hook-actual.log and /private/tmp/fl-reset-observer-
{native,pg,regression,clippy,minimal-clippy}.log. Final source-only rustfmt reexport
ordering followed green checks; no deps/locks. Broader auth hooks/provider/plugin
gates remain open;660/682(96.8%),22open unchanged.

### 2026-10-14 — Awaited email-verification hooks

Independent trusted before/after builders now cover ordinary verification and
the existing change-email verification after-hook branch. Hooks always run
outside database transactions and bypass background dispatch. Before failure
leaves the user unverified; after failure preserves committed verification and
session changes, returns sanitized empty500 and suppresses cookies/redirects.
Already-verified replay, change-email confirmation and legacy branches retain
their observed hook skips. Writer ID/email rechecks reject stale callback changes;
this is a stronger stale-callback boundary than the upstream flow. Native
cancellation preserves the corresponding pre/post-commit state, without claiming
JavaScript disconnect equivalence. Public-user context validates borrowed fields
before cloning, owns bounded zeroized JSON, exposes no request/credentials or
Debug/Serde; no-hook paths skip projection. Blocking hooks/panics are not caught.

Actual Better Auth oracle12 passed; native2 covers14 branches and before/after
cancellation; affected email5 passed3.59s; restricted PostgreSQL1 passed132.89s;
strict all-feature/all-target18.09s and minimal2.95s passed. Frozen eight-path
manifest /private/tmp/fl-email-observer-freeze.json verified (SHA256
42e07c6aa2f787a9637249e1ce58461fef21f779265e827506d5a141c02b7f5c).
Evidence: /private/tmp/fl-email-verification-hooks-actual.log and
/private/tmp/fl-email-observer-{native,regression,pg,clippy,minimal-clippy}.log.
Auto-signin after verification remains separate; checklist660/682(96.8%),22open.

### 2026-10-15 — Optional sign-in after email verification

AuthHttp.with_auto_sign_in_after_verification enables ordinary newly verified
proof login after the awaited after-hook; defaultfalse. Current same-user session
is reused; anonymous/invalid/foreign/expired sessions create a target session.
Already-verified replay and change-email flows retain their independent behavior.
Redirects carry the login cookie; signed transient preference suppresses Max-Age
without shortening the configured persisted lifetime. Final writer rechecks
current target ID/email/verified state before granting a session; this stronger
stale-hook boundary is explicit. No transaction spans the application callback.

Expired cleanup commits independently before session insertion. Unexpected
insert/commit failure retains verified state/cleanup, returns sanitized empty500
and suppresses pending cookies. Native Result<()> storage cannot represent
upstream createSession=null/APIError; that different clear-cookie/error behavior
is documented as unsupported rather than mapped onto all storage failures.
Actual15, native2(17 lifecycle branches+cancellation), affected email7(3.66s),
restricted PostgreSQL1(137.88s) and strict full18.74s/minimal4.26s pass; owned
roles/records/audits0. Eight-path /private/tmp/fl-verification-auto-freeze.json
verified SHA2564ca4ee6e6413cbe17a2b130ca28282cdd189c6a74b1305e909ed2a09411ba5f2.
Logs /private/tmp/fl-verification-auto-{actual,native,regression,pg,clippy,minimal-clippy}.log.
No dependencies/locks; checklist660/682(96.8%),22broad gates remain open.


### 2026-10-15 — shared session-policy implementation

The additive `SessionPolicy` configures expiry, refresh age, freshness age and
refresh disabling across AuthHttp and its credential, Google, MFA, verification
and organization/session services. Direct service callers receive the same builder.
Default constructors preserve the existing one-day ForkLaunch preset; upstream's
unconfigured seven-day lifetime is a separate default, not silently substituted.
The typed domain accepts whole-second u32 values with positive expiry, allows
update age beyond expiry, and supports zero refresh age and zero freshness.

Persistent creation and cookie Max-Age use the policy. Actual Better Auth checks
confirm that transient credential/pending-MFA grants retain one-day stored expiry,
while password rotation, enrollment/disable and email-verification auto-login use
configured expiry even when cookies omit Max-Age. Stored expiry remains authoritative
for existing rows. JWT and MFA proof/trust lifetimes remain independent. Deferred
refresh, secondary storage and cookie caching remain open, along with broader
credential, organization and plugin options. This does not close the Better Auth gate.


### 2026-10-15 — Trusted credential policy and token lifetimes

Added bounded native credential policy for disabling signup and UTF-16 password
lengths, with upstream zero fallback, minimum-first new-password checks and
maximum-only existing-password proofs. Effective limits are 1..4096 code units.
Trusted whole-second reset and verification TTLs reach existing issuance paths;
reset zero falls back to 3600 seconds while verification zero is preserved.
HTTP request validation still precedes handler policy. No request can select policy.

Actual Better Auth fixture: 17 observations passed. Focused native tests: 3 passed;
full IAM: 217 passed, 3 PostgreSQL tests separately ignored; restricted PostgreSQL
proof passed in 176.66 seconds and owned roles, records and audit rows returned to
zero. Strict full and minimal Clippy passed. Frozen 20-source manifest:
`/private/tmp/fl-credential-policy-freeze.json`; evidence logs:
`/private/tmp/fl-credential-policy-{actual,native,full,pg,clippy,minimal-clippy}.log`.
Change-email delivery and delete-user remain disabled. Fresh clock sampling after
awaited work is a separate pending slice; unrestricted callbacks, broader plugins
and full Better Auth parity remain open. Checklist remains 660/682 (96.8%).


### 2026-10-15 — Fresh HTTP clock sampling across auth grants

A private optional time source now propagates the configured HTTP clock through
credentials, sessions, Google, MFA and initial organization session checks. Direct
service APIs retain their explicitly supplied timestamp. Both clock/factor builder
orders bind the same clock. Session expiry/refresh checks sample after authoritative
reads; grants sample after awaited hashing/delivery/provider/store work. Password
and MFA grant rotation recheck authority before mint/commit, preserving the existing
challenge/session equality rules and independent async OTP/backup lease clocks.
Late writes cannot renew authority that crossed its old expiry during the wait.
This strengthens selected native race boundaries; it does not promise a deadline
after COMMIT or exact upstream race behavior.

Actual delayed-work oracle: 3 cases passed. Pre-review focused clock test passed;
full IAM 218 passed/3 ignored in 166.38s; restricted PostgreSQL passed in 215.96s
with owned roles/records/audit rows zero. Root review then found an old supplied-time
load in externally supplied change-email proof application. Corrected final source
passes its dedicated delayed-writer test and all 8 affected email regressions, plus
strict full/all-target and minimal Clippy. The earlier full/PG runs are retained as
pre-correction evidence, not represented as reruns of the final source.

Final 34-source manifest: `/private/tmp/fl-clock-final-freeze.json`. Original logs:
`/private/tmp/fl-clock-{actual,native,full,pg,clippy,minimal-clippy}.log`; final review
logs: `/private/tmp/fl-clock-review-{native,email,clippy,minimal-clippy}.log`.
Secondary organization transaction rechecks remain a concrete follow-up, as do
broader auth options/plugins. No broad gate closes; checklist remains 660/682.
