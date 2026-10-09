# Candidate BetterAuth organization/session authorization

An old login must not lend another login access to its selected organization.
This candidate checks the exact signed user, organization and session, followed
by current membership, on every request. There is no authorization decision cache.

The new internal HMAC routes include all three identifiers:
`/:id/organizations/:organizationId/sessions/:sessionId/surface-roles` and
`.../surface-permissions`. Their signed path is route-relative, matching the
existing framework HMAC contract. JWT payloads add `sessionId` from the
already-authenticated BetterAuth session. No caller-supplied header selects scope.

The old user-only BetterAuth endpoints remain registered but return empty sets.
Old tokens require a new login; mixed old/new generated services fail closed.
Exported remote factory names and outer parameters stay compatible. Local
factories require scoped methods; old user-only implementations cannot authorize.
The legacy IAM-base implementation and global platform role allowlists are untouched.

The exact existing BetterAuth organization ACL statements now have a single source
used by both BetterAuth configuration and permission lookup. Organization owners
retain their declared read/write permissions; they are not renamed to platform
administrators. Custom permissions remain restricted to the verified organization.
Session/member/custom-role queries request refresh to avoid stale identity-map reads.

Repository-only regressions execute the actual TypeScript scaffold code through
an installed esbuild transformer and bounded in-memory ORM/SDK fakes:

```
node cli/tests/iam_authorization_scope.cjs /absolute/path/esbuild/lib/main.js
```

Eight cases cover concurrent organizations, user/session mismatches, expiry,
membership removal, custom permissions, missing/legacy claims, onboarding after an
empty result, role changes, absence of decision-cache calls, signed-path scope,
local legacy refusal, and actual framework HMAC signature changes. These are not
real database, HTTP middleware or emitted-scaffold compilation evidence.

Before integration, emit a fresh candidate scaffold without overlays, normally
install dependencies, compile/lint, and exercise owned PostgreSQL/Redis with normal
signup/signin, organization create/activate/token endpoints. Retain session A's
organization token, create a second login B without an organization and prove its
JWT cannot inherit A's owner access. Explicitly activate/deactivate an organization,
remint tokens, test scoped HMAC endpoints with correct and tampered components,
expired/revoked sessions and current membership. Use public BetterAuth flows;
do not seed verification flags or roles, loosen authorization, or call a provider.
The existing twelve-auth-probe baseline does not validate these new routes.
