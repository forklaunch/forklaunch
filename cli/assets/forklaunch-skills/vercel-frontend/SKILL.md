---
name: vercel-frontend
description: "Deploy a frontend (Next.js, Vite, Nuxt, SvelteKit, Astro) to Vercel and wire it to deployed ForkLaunch services: origin strategy and rewrite proxying, custom domains and DNS, env vars both directions, CORS, better-auth cookies."
user-invokable: true
---

# Vercel Frontend Skill

Covers the gap between "backend is deployed on ForkLaunch" and "a browser UI on Vercel can
actually call it". `forklaunch init application` does **not** generate a client, and nothing in
the generated backend is configured for a browser on another domain.

The frontend can be anything Vercel builds — Next.js, Vite (React/Vue/Svelte), Nuxt, SvelteKit,
Astro, Remix. ForkLaunch only supplies backend services over HTTPS, so everything here is written
framework-agnostically; where a framework's own convention matters it is called out.

Verified end-to-end against CLI v1.3.3, Vercel CLI 54.21.1, August 2026.

> **Verification status.** The direct cross-origin path (Strategy C) was exercised end to end:
> deploy, CORS preflight, sign-up, JWT. The proxy and custom-domain strategies (A and B) are
> derived from Vercel's rewrite semantics and from ForkLaunch's own
> `platform-management/domain/utils/dns.util.ts`, but have **not** been run end to end. Treat
> their command snippets as a starting point, and verify with the checks in
> [Verifying end to end](#verifying-end-to-end).

## Prerequisites

### 1. Vercel CLI

```bash
command -v vercel >/dev/null 2>&1 || npm install -g vercel
vercel --version
```

### 2. Vercel login — THE USER MUST DO THIS, NOT YOU

`vercel login` opens an interactive browser/device flow against the user's real account. Do not
attempt it from a script or agent context; it will hang or authenticate the wrong account.

Check first, and only ask if it fails:

```bash
vercel whoami          # prints the username if already logged in
vercel teams ls        # shows which scopes (personal + orgs) are available
```

If `vercel whoami` errors, stop and tell the user:

> Run `vercel login` in your terminal (or type `! vercel login` here), then tell me which scope
> to deploy into — `vercel teams ls` lists them.

Never guess the scope. A personal account and a company team are different billing and
visibility domains.

## Step 0: pick an origin strategy — do this before writing any config

Almost every problem in this document (CORS preflight, `SameSite=None`, `MISSING_OR_NULL_ORIGIN`,
third-party cookie blocking) exists **only because the browser treats the UI and the API as
different sites**. Choosing a different origin layout makes those problems not happen, rather
than making you configure your way out of them.

Decide this first. Retrofitting later means re-pushing backend config and redeploying.

| | Browser sees | CORS needed? | Cookie config | Use when |
|---|---|---|---|---|
| **A. Proxy through Vercel** | same origin | no | default `Lax` works | default choice |
| **B. Custom domain, shared parent** | cross-origin, **same-site** | yes | default `Lax` works | you own a domain |
| **C. Direct to `*.app.forklaunch.com`** | cross-site | yes | `None; Secure` | quick demos only |

Prefer A. Fall back to B when you need the API reachable at a stable public hostname of its own
(mobile clients, webhooks, partners). Use C only for throwaway work — it is the least safe and
the most fragile, because it puts the login session on a third-party cookie that Safari blocks
outright and any Chrome user can switch off.

### Strategy A: proxy the backend through your Vercel app

Add rewrites so the browser only ever talks to your own origin and Vercel forwards to ForkLaunch
server-side. `vercel.json` works for **every** framework, because it is a platform feature rather
than a framework one:

```json
{
  "rewrites": [
    { "source": "/api/auth/:path*",   "destination": "https://iam.<app>-<env>-<region>-<hash>.app.forklaunch.com/api/auth/:path*" },
    { "source": "/api/iam/:path*",    "destination": "https://iam.<app>-<env>-<region>-<hash>.app.forklaunch.com/:path*" },
    { "source": "/api/orders/:path*", "destination": "https://orders.<app>-<env>-<region>-<hash>.app.forklaunch.com/:path*" }
  ]
}
```

Then the client calls **relative** paths — `fetch('/api/orders')` — and needs no
`*_URL` env var at all for those services.

Four things that will bite you:

- **Mirror better-auth's path exactly.** better-auth mounts at `/api/auth` (or whatever
  `BETTER_AUTH_BASE_PATH` says). Proxy `/api/auth/:path*` straight through, unprefixed. If you
  nest it (`/api/iam/api/auth/...`) its generated callback and redirect URLs will not match what
  the browser requested.
- **Order matters, and a catch-all eats everything.** Vercel evaluates `rewrites` top to bottom
  and takes the first match. An SPA fallback such as
  `{ "source": "/(.*)", "destination": "/spa" }` — which this repo's own `client/vercel.json`
  has — must be **last**. A proxy rule appended after it silently never runs.
- **`vercel.json` cannot read env vars.** Destinations are literal strings, so one file cannot
  serve dev/preview/prod pointing at different backends. Options: keep one ForkLaunch environment
  per Vercel project; or, on Next.js only, use `next.config.js` `rewrites()`, which is JS and
  *can* read `process.env`:

  ```javascript
  // next.config.js — Next.js only
  module.exports = {
    async rewrites() {
      return [
        { source: '/api/auth/:path*', destination: `${process.env.IAM_ORIGIN}/api/auth/:path*` },
        { source: '/api/orders/:path*', destination: `${process.env.ORDERS_ORIGIN}/:path*` }
      ];
    }
  };
  ```

  Note `IAM_ORIGIN` has **no public prefix** — it is read at build/server time and never shipped
  to the browser, which is part of the benefit.
- **You still need `CORS_ORIGINS`.** The scaffold wires it straight into better-auth as
  `trustedOrigins: CORS_ORIGINS` (`iam/auth.ts`), and better-auth rejects untrusted origins on its
  own, independently of what the browser enforced. Vercel forwards the browser's `Origin` header
  through the proxy, so it still has to be on the list. What you skip is the *preflight* plumbing
  and the `SameSite=None` cookie — not the trusted-origin list.

With a proxy, set `BETTER_AUTH_URL` to your **UI origin** (`https://<ui>.vercel.app`), since that
is the URL the browser actually requested and the one cookies and redirects must match. Two
knock-on effects:

- `iam/auth.ts` builds the Google callback as
  `${baseURL}${BETTER_AUTH_BASE_PATH}/callback/google`, so the redirect URI becomes
  `https://<ui>/api/auth/callback/google`. Update the Google Cloud console entry to match, or
  social sign-in breaks with `redirect_uri_mismatch`.
- Leave service-to-service `JWKS_PUBLIC_KEY_URL` pointed at the iam hostname directly. Other
  services fetch it from inside AWS and should not be routed back out through Vercel.

## Custom domains and DNS (Strategy B)

### There is no CLI command for this

`fl` has no `domain` subcommand as of v1.3.3. Custom domains are dashboard/API only:

```
Dashboard -> your app -> Custom Domain
GET /applications/:applicationId/custom-domain    # status + delegation/validation records
```

So an agent cannot detect or configure this from the CLI. **Ask the user whether a custom domain
is configured before you write any URL into Vercel** — if one is pending and you wire the
generated `*.app.forklaunch.com` hostnames, every env var has to be rewritten later.

### The DNS step is the user's, and it blocks everything

The default is **delegated** mode: ForkLaunch creates a hosted zone and returns its name servers
(`nsRecords` on the response); the customer adds one NS record at their registrar delegating the
subdomain to them, and status starts at `PENDING_DELEGATION`. Once public DNS sees the NS
records, ForkLaunch writes the certificate validation records itself — the customer has nothing
more to do. **Manual** mode (the previous behaviour, for customers who keep their own zone)
instead returns CNAME validation records the customer must add themselves, starting at
`PENDING_VALIDATION`. Either way nothing resolves until status reaches `VALIDATED`, and manual
mode goes to `FAILED` if the records never appear. Tell them plainly:

> Add the NS record (delegated mode) or CNAME records (manual mode) shown on the dashboard, then
> wait for the domain to show **Validated**. Until then the custom hostnames will not resolve
> and I should not put them in Vercel.

### Get the hostname format right — non-prod environments are suffixed

From `platform-management/domain/utils/dns.util.ts`, only `prod`, `production` and `main` get the
bare hostname. Everything else has the environment appended:

```
production   -> iam.example.com
development  -> iam-development.example.com
staging      -> iam-staging.example.com
```

Guessing `iam.example.com` for a `development` environment is the common mistake.

### Why this is the safer layout

Put the UI on the same registrable domain as the services — `app.example.com` calling
`iam.example.com`. Those are different **origins** (so CORS still applies) but the same **site**,
because `SameSite` is computed on the registrable domain, not the full host. That means:

- session cookies work at `SameSite=Lax` — no `None`, so the session never depends on
  third-party-cookie behaviour, which differs per browser and is not under your control
- CSRF protection stays meaningful, whereas `SameSite=None` removes it
- the backend hostnames are yours, so they stay stable across redeploys and can be re-pointed
  without touching the frontend

Leave the cookie host-only (no `Domain` attribute). iam is the only service that sets a session
cookie — the rest take a Bearer JWT — so a `Domain=.example.com` cookie would only widen its
exposure to every subdomain for no benefit.

This is exactly why `*.vercel.app` is the worst case: `vercel.app` is on the Public Suffix List,
so `ui.vercel.app` is a site unto itself. It is cross-site from `*.app.forklaunch.com` **and**
from every other `*.vercel.app` app.

Once the domain is `VALIDATED`, point the Vercel env vars — or the Strategy A rewrite
destinations — at the custom hostnames, and put the UI's custom hostname in `CORS_ORIGINS`.

## The two-way env-var contract (Strategy C, and the parts A and B still need)

This is the part people lose hours to. Wiring is **bidirectional** — each side must be told
about the other:

| Direction | Variable | Set where | Value | Needed for |
|---|---|---|---|---|
| UI -> backend | `<PREFIX>IAM_URL` | Vercel project env | public iam endpoint | B, C |
| UI -> backend | `<PREFIX><SVC>_URL` | Vercel project env | public endpoint, **one per service** | B, C |
| backend -> UI | `CORS_ORIGINS` | ForkLaunch config | the UI's public origin | A, B, C |
| backend -> UI | `BETTER_AUTH_URL` | ForkLaunch config | origin the **browser** requests | A, B, C |

Under Strategy A the `*_URL` rows disappear (the client uses relative paths) but the two backend
rows remain, and `BETTER_AUTH_URL` becomes the UI origin rather than the iam endpoint.

There is no API gateway. Under B and C the browser talks to each service on its own hostname, so
it needs one URL variable per service it calls.

### The public-env prefix is your framework's, not Vercel's

Vercel stores whatever key you give it; the prefix that decides whether a value is exposed to
browser code is a bundler rule. A variable without the right prefix is `undefined` at runtime with
no build error.

| Framework | Prefix | Read as |
|---|---|---|
| Next.js | `NEXT_PUBLIC_` | `process.env.NEXT_PUBLIC_IAM_URL` |
| Vite (React/Vue/Svelte) | `VITE_` | `import.meta.env.VITE_IAM_URL` |
| Nuxt 3 | `NUXT_PUBLIC_` | `useRuntimeConfig().public.iamUrl` |
| SvelteKit / Astro | `PUBLIC_` | `import { PUBLIC_IAM_URL } from '$env/static/public'` |
| Create React App | `REACT_APP_` | `process.env.REACT_APP_IAM_URL` |

Next.js and Vite also **inline** these at build time, so changing one in Vercel requires a
**rebuild**, not just a redeploy — `vercel deploy --prod` after `vercel env add`, never a
promote of the existing build.

Get the backend endpoints from Dashboard -> your app -> **Endpoints**:

```
iam     iam.<app>-<env>-<region>-<hash>.app.forklaunch.com
orders  orders.<app>-<env>-<region>-<hash>.app.forklaunch.com
```

## Order of operations (there is a chicken-and-egg)

The backend needs the UI's URL and the UI needs the backend's URLs, so you cannot do it in one
pass:

0. Pick an origin strategy, and ask whether a custom domain is configured.
1. Deploy the backend (`release create` -> `deploy create`), read the Endpoints page.
2. Point the UI at the backend — rewrite destinations (A) or `<PREFIX>*_URL` env vars (B, C) —
   then deploy the UI and read back its public URL.
3. Push `CORS_ORIGINS` + `BETTER_AUTH_URL` to the ForkLaunch environment.
4. Redeploy the backend so the new env is picked up. **See the known blocker below.**

Under Strategy A you deploy the UI twice: once to learn its origin, and again if the rewrite
destinations changed. That is expected, not a mistake.

## Deploying the client

```bash
cd client

# Non-interactive project creation. Appends VERCEL_OIDC_TOKEN to .env.local;
# it does NOT clobber keys you already put there.
vercel link --yes --scope <team-id> --project <name>

# Strategy B/C only — Strategy A uses relative paths and needs none of this.
# One call per target; the value is read from stdin. Swap the prefix for your framework.
for env in production preview development; do
  printf 'https://iam.<...>.app.forklaunch.com'    | vercel env add NEXT_PUBLIC_IAM_URL    $env --force
  printf 'https://orders.<...>.app.forklaunch.com' | vercel env add NEXT_PUBLIC_ORDERS_URL $env --force
done

vercel deploy --prod --yes    # a REBUILD — required, since these are inlined at build time
vercel alias ls               # find the PUBLIC url — see next section
```

### Pick the right URL — not every alias is public

`vercel alias ls` returns several aliases for one deployment. The team-scoped one
(`<project>-<team>.vercel.app`) sits behind Vercel deployment protection and returns **302**;
the short alias (`<project>-<word>.vercel.app`) is public and returns **200**.

Always verify before using it as `CORS_ORIGINS` — a protected URL will never work from a browser:

```bash
curl -s -o /dev/null -w "%{http_code}\n" https://<candidate>.vercel.app   # want 200
```

### Vercel blocks vulnerable Next.js versions at deploy time (Next.js only)

```
Error: Vulnerable version of Next.js detected, please update immediately.
```

This passes `next build` locally and only fails at upload, so it surprises you late. Fix:

```bash
npm install next@latest react@latest react-dom@latest
```

(Observed: `next@15.5.4` rejected, `next@16.3.0` accepted.)

## Backend side: CORS (Strategies B and C)

Under Strategy A the browser never makes a cross-origin request, so this whole section — and the
scaffold gap it describes — stops mattering. That is the strongest practical argument for
proxying.

**Generated non-iam services have no CORS support at all.** Only the iam module wires it. A
scaffolded business service calls:

```typescript
const app = forklaunchExpress(SchemaValidator(), otel, { auth: authOptions });
```

with no `cors` option, and its `registrations.ts` never reads `CORS_ORIGINS`. Any browser call
to it is blocked no matter what you configure on the platform.

Add it manually, mirroring iam. In `registrations.ts`, inside the `environmentConfig` chain:

```typescript
CORS_ORIGINS: {
  lifetime: Lifetime.Singleton,
  type: array(string),                       // import `array` from @<app>/core
  value: getEnvVar('CORS_ORIGINS')?.split(','),
},
```

and in `server.ts`:

```typescript
const corsOrigins = ci.resolve(tokens.CORS_ORIGINS);

const app = forklaunchExpress(SchemaValidator(), openTelemetryCollector, {
  auth: authOptions,
  cors: {
    origin: corsOrigins,
    methods: ['GET', 'POST', 'PUT', 'DELETE', 'OPTIONS'],
    credentials: true
  }
});
```

Verify with a preflight — a correct response echoes the origin back:

```bash
curl -s -i -X OPTIONS https://<svc>/<route> \
  -H "Origin: https://<ui>.vercel.app" -H "Access-Control-Request-Method: GET" \
  | grep -i "access-control-allow-origin"
```

No `access-control-allow-origin` header means it is NOT working, even though the status is 204.
A disallowed origin returns 204 with the header absent, which looks deceptively like success.

## Backend side: config push

`CORS_ORIGINS` and `BETTER_AUTH_URL` are declared `origin: platform` in the release manifest, but
the platform has **no value to inject** for them, so they resolve EMPTY after a successful deploy.

This is a deliberate exception to the general rule "never hand-supply a platform-injected
variable". Both encode a public URL the platform cannot know: the UI's origin, and the service's
own public hostname. Supply them yourself.

They appear at BOTH application and service scope in `config pull`. Push them under the service
section too, or the service-scoped entry stays empty:

```bash
cat > env.env <<'EOF'
# application
ENCRYPTION_KEY=<keep the existing value — changing it breaks data encrypted at rest>
CORS_ORIGINS=https://<ui>.vercel.app
BETTER_AUTH_URL=https://iam.<...>.app.forklaunch.com

# iam (<service-id from `forklaunch config pull`>)
BETTER_AUTH_URL=https://iam.<...>.app.forklaunch.com
CORS_ORIGINS=https://<ui>.vercel.app
EOF

forklaunch config push -r <region> -e <env> -i env.env
forklaunch config pull -r <region> -e <env> -o /tmp/check.env   # confirm both scopes
```

`config push` auto-creates the environment; `config pull` fails with "Environment not found" if
it does not exist yet. Push before you pull on a new app.

## KNOWN BLOCKER: env-var changes do not reach running containers

Verified August 2026, and currently unresolved.

After `config push`, **none** of these caused the running ECS task to pick up the new value:

- `forklaunch deploy create ...` (x3) — each printed `Operation successful!`
- Dashboard -> Manage -> Environment Variables -> **Save & Deploy** -> Deploy `<service>` only
  — deployment status reached `completed`

Ground truth is the container boot time. Across ~50 minutes and 4 "successful" deploys it never
changed:

```bash
forklaunch observe logs -e <env> --limit 5 | grep "Server is running"
# 2026-08-07T10:28:36Z  ... still the original boot at 11:14Z
```

The dashboard reported `CORS_ORIGINS` as set ("User Supplied (6)", "All 6 variables set") the
whole time, while the preflight kept returning no `access-control-allow-origin`.

**Always confirm an env change actually landed by checking the boot timestamp, not the deploy
status.** A green deploy does not mean the process restarted. If the timestamp has not moved,
the new value is not live, and there is currently no documented CLI or dashboard path that
forces it. Escalate rather than burning deploys.

## Auth flow the UI must implement

A user who just signed up has **no role**, and every protected endpoint returns
403 `Invalid Authorization roles.` until an organization exists AND is active. The client must
do all four steps, in order:

```
1. POST /api/auth/sign-up/email        (or /sign-in/email)
2. POST /api/auth/organization/create
3. POST /api/auth/organization/set-active   <- skipped most often; without it everything 403s
4. GET  /api/auth/token                -> JWT for the business services
```

Then call business services with `Authorization: Bearer <jwt>`; they verify it via JWKS.

Other things that bite:

- **`firstName` / `lastName` are required** on sign-up, not just better-auth's `name`. Omitting
  them gives `400 {"code":"MISSING_FIELD","message":"firstName is required"}`.
- **better-auth rejects any request with no `Origin` header**
  (`MISSING_OR_NULL_ORIGIN`). Browsers always send one, but server-side calls (Next.js route
  handlers, server actions, SSR, curl) do not — set it explicitly, and make sure the value is in
  `CORS_ORIGINS`.
- **All iam fetches need `credentials: 'include'`** — the session cookie lives on the iam origin,
  which is never the same origin as the UI.
- **Role vocabularies do not match.** iam surfaces better-auth's org roles
  (`owner`/`admin`/`member`), but generated controllers gate on core's `ROLES`
  (`viewer`/`editor`/`admin`/`system`). An org creator is `owner`, which matches nothing. Map
  them in `iam/domain/services/surfacing.service.ts` (`owner -> admin`, `member -> viewer`),
  then flush redis (`auth:roles:*` is cached) and restart.

## Cross-site cookies (Strategy C only — last resort)

Reach for this only after ruling out A and B. `SameSite=None` opts the session cookie into
third-party-cookie territory, where behaviour is the browser's call rather than yours: Safari
blocks third-party cookies outright, Firefox partitions them under Total Cookie Protection, and
Chrome keeps them but exposes a user-facing setting that can switch them off. It also gives up
the CSRF protection `SameSite` exists to provide. So a session that works in your Chrome may
simply not work for a Safari user — a proxy (A) or a shared parent domain (B) avoids the whole
class of problem.

(Chrome's third-party-cookie *deprecation* was cancelled in 2024 and Privacy Sandbox was retired
through 2026, so these cookies are not disappearing — but they remain user-disableable, which is
enough to make them a bad foundation for a login session.)

The scaffold leaves better-auth cookies at the default `SameSite=Lax`, with no `Secure` and no
`Domain`:

```
set-cookie: better-auth.session_token=...; Max-Age=86400; Path=/; HttpOnly; SameSite=Lax
```

A UI on `*.vercel.app` calling an API on `*.app.forklaunch.com` is **cross-site**, so the browser
will not send that cookie on XHR. Cross-site requires `SameSite=None` + `Secure`, which requires
https. Patch `iam/auth.ts`:

```typescript
advanced: {
  database: { generateId: false },
  useSecureCookies: baseURL.startsWith('https://'),
  defaultCookieAttributes: baseURL.startsWith('https://')
    ? { sameSite: 'none' as const, secure: true, httpOnly: true }
    : { sameSite: 'lax' as const, secure: false, httpOnly: true }
}
```

This depends on `BETTER_AUTH_URL` being set to the public https endpoint — if it is empty,
`baseURL` falls back to `http://0.0.0.0:8000` and the https branch never runs.

Note that local development is unaffected: `localhost:3001` -> `localhost:8000` differ only by
port, which is same-site, so `Lax` works and this bug stays invisible until you deploy.

## Verifying end to end

```bash
# 1. backend reachable
curl -s -o /dev/null -w "%{http_code}\n" https://iam.<...>/health      # 200
curl -s https://iam.<...>/api/auth/jwks                                 # real keys

# 2. protected route rejects anonymous callers
curl -s -o /dev/null -w "%{http_code}\n" https://orders.<...>/orders    # 401

# 3. Strategy A only — the proxy actually forwards, and does so same-origin
curl -s -o /dev/null -w "%{http_code}\n" https://<ui>.vercel.app/api/auth/jwks   # 200, not 404
# 404 here means the rewrite never matched — almost always an SPA catch-all listed above it.

# 4. Strategies B/C only — CORS echoes the UI origin back
curl -s -i -X OPTIONS https://iam.<...>/api/auth/sign-in/email \
  -H "Origin: https://<ui>.vercel.app" -H "Access-Control-Request-Method: POST" \
  | grep -i access-control-allow-origin

# 5. cookie attributes match the strategy
curl -s -i -X POST https://<ui-or-iam-origin>/api/auth/sign-in/email \
  -H 'Content-Type: application/json' -H 'Origin: https://<ui>' \
  -d '{"email":"...","password":"..."}' | grep -i set-cookie
# A or B -> SameSite=Lax   |   C -> SameSite=None; Secure

# 6. drive the real UI (gstack)
$B goto https://<ui>.vercel.app
$B console --errors     # CORS failures show up here, not in the network status code
```

## Related

- `/cli` — release/deploy commands, scaffold bugs, CodeBuild pnpm blockers
- `/frontend-patterns` — page/hook/SDK patterns once the connection works
- `/backend-patterns` — cross-service auth, HMAC signing, surfacing functions
