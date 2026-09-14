---
name: security
description: "Security: auth surfaces, device flow, rate limiting, security events, secrets hygiene, HMAC, branch protection."
user-invokable: true
---

# ForkLaunch Platform Security

## When to Use This Skill

Use when the user asks about:

- Authentication surfaces (better-auth, JWT, sessions, device authorization)
- The CLI device-approval flow (`/cli-login`, `forklaunch login`)
- Rate limiting (better-auth custom rules, `RateLimiter` from core)
- Security-event emission and alerting
- HMAC service-to-service auth
- Secrets hygiene and environment separation
- Deployment approval gates
- Branch protection / change management

For field encryption, tenant isolation, and PII classification, use the
`compliance` skill — that is the authoritative reference.

## Auth Surfaces Overview

| Surface | Where | Mechanism |
| --- | --- | --- |
| Dashboard login | `src/modules/iam/auth.ts` (better-auth) | Email/password, Google/GitHub social, magic link |
| Session → JWT | better-auth `jwt()` plugin | 7d JWT, JWKS at `/api/auth/jwks` |
| CLI login | `deviceAuthorization()` plugin + `client/app/cli-login/page.tsx` | OAuth 2.0 device flow |
| Service-to-service | `access: 'internal'` handlers | HMAC-SHA256 signed requests |
| Route RBAC | `access: 'protected'` + `allowedRoles` | App-level `surfaceRoles` / `surfacePermissions` (see backend-patterns) |

All better-auth endpoints mount through the catch-all in
`src/modules/iam/server.ts` (`/api/auth/{*any}`); there are no hand-written
device routes.

## CLI Device Authorization Flow

1. CLI: `POST /api/auth/device/code` with `client_id: "forklaunch-cli"`.
2. User's browser opens `/cli-login?user_code=…`, signs in, approves/denies.
3. CLI polls `POST /api/auth/device/token` until approved.
4. CLI exchanges the session token for a JWT via `GET /api/auth/token`
   (`bearer()` plugin) and stores it with `0600` perms.

### Hardening rules (do not regress)

- `deviceAuthorization` must keep: short `expiresIn`, explicit `interval`,
  and `validateClient` restricted to known client IDs (`forklaunch-cli`).
- Device codes are **single-use**: approval of a non-`pending` code and
  token exchange of a consumed/expired code must fail. Tests live in
  `src/modules/iam/__test__/device-authorization*.test.ts` — keep replay and
  expiration tests green when touching the flow.
- better-auth `rateLimit.customRules` must cover `/device/code`,
  `/device/token`, `/device/approve`, `/device/deny` (Redis
  secondary-storage backed; disabled only when `NODE_ENV=test`).
- The `/cli-login` page must show the user code for comparison against the
  terminal, offer an explicit Deny action, and warn against approving codes
  the user did not initiate.

## Rate Limiting

Two mechanisms exist:

1. **better-auth built-in** (`auth.ts` `rateLimit` block) — Redis-backed via
   the secondary-storage adapter; add per-path `customRules` for any new
   sensitive auth endpoint.
2. **`RateLimiter` from `@forklaunch/core/http`** — generic sliding-window
   limiter over a `TtlCache` (`RedisTtlCache` is DI-registered in every
   module). `RateLimiter.buildKey({ tenantId, route, userId, operationType })`
   → `check(key, limit, windowMs)`. Fails open on cache errors. Use for
   non-auth hot paths.

## Security Events

The shared taxonomy lives in `src/modules/monitoring/securityEvents.ts`
(`SecurityEventType`, `SecurityEventSeverity`, `recordSecurityEvent`). Emit
from any module:

```typescript
import { recordSecurityEvent, SecurityEventType } from '@forklaunch-platform/monitoring';

recordSecurityEvent(otelCollector, SecurityEventType.HMAC_AUTH_FAILED, 'platform-management', {
  organizationId,
  route: req.path
});
```

- Emission is best-effort — never throws, safe in catch blocks.
- Series: `securityEventTotal` counter with `event_type`, `severity`,
  `source`, `organization_id` labels.
- Alerting: observability-api alert rules use `metricName:
  'security_events'` (all high-severity) or
  `'security_events:<event_type>'` (exact type). Default high-risk rules are
  seeded via the security-alert-defaults service. See `docs/security-events.md`.
- When adding a new event type: add it to the taxonomy + default-severity
  map, emit it, and decide whether it needs a default alert rule.

## HMAC Service-to-Service Auth

Use `generateHmacAuthHeaders` — full path-signing rules (mount-prefix
stripping, no query params in the signed path) are in the
`backend-patterns` skill. Failures on the receiving side should emit
`SecurityEventType.HMAC_AUTH_FAILED`.

## Deployment Approval Gate

Deployments to environments that require approval need a persisted,
un-expired, four-eyes-approved `DeploymentApproval` record
(platform-management). Approval is required by default for production-named
environments, configurable per environment (`requireDeploymentApproval`,
via `PATCH /applications/:id/environments/:name/approval-config`), and
bypassed for `auto_release_and_deploy` applications — the decision is
centralized in `DeploymentApprovalService.isApprovalRequired`. The gate is
enforced at dispatch (`deployment.service.ts`) and re-verified by the worker
before Pulumi execution (`deployment-processor.service.ts` — parks, never
throws). Approval state changes emit `DEPLOYMENT_APPROVAL_*` security events.

## Secrets Hygiene & Environment Separation

- Never commit `.env.local` / `.env.production*`; templates only. CI blocks
  live-credential patterns via `scripts/check-env-separation.sh`.
- Production secrets live in the encrypted `environment_variable` store and
  reach ECS via SSM/Secrets Manager `secrets` entries — never in the repo,
  never in test env files. Policy: `docs/environment-separation.md`.
- `getSecret()` (boot-validated) over raw `process.env` for required secrets.

## Change Management

- Branch protection config-as-code: `.github/rulesets/main-branch-protection.json`,
  applied/verified via `scripts/apply-branch-protection.sh` /
  `scripts/check-branch-protection.sh`; policy in `.github/BRANCH_PROTECTION.md`.
- Required CI: `depcheck` (dependency alignment + env separation),
  `lint-and-format`, `build-and-test`, `e2e`.
- `CODEOWNERS` routes security-sensitive paths (iam, deployment worker,
  `.github/`, `scripts/`) to maintainers.
- Dependabot (`.github/dependabot.yml`) watches npm workspaces and actions.

## Access Reviews

Recurring access reviews are generated by iam's access-review service
(`scripts/generate-access-review.ts`, `access-review:generate`): users,
roles, organizations, and last activity derived from sessions, persisted as
`AccessReview` records. Cadence and process: `docs/access-reviews.md`.
