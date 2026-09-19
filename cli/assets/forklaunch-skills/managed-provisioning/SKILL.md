---
name: managed-provisioning
description: "Managed instances as a state machine: every state, every edge, who moves it, which route does it, and how a client hooks into it (provisioning, claim, reset, propagation, rollouts, teardown)."
user-invokable: true
---

# Managed Provisioning

## What this is, plainly

You built an app. You want to sell it so **each customer gets their own private
copy**: one dental practice's patient records never share a database with
another's.

A **template** is the blueprint (your git repo, built once per version into an
image). An **instance** is one customer's copy. **Claiming** is the handover:
the customer sets a passphrase and the instance becomes theirs.

Say you sell a booking system to dental practices. You publish version 1.4.0 of
the template. An operator on your team launches an instance for Dr. Chen's
practice; twenty minutes later it is running at its own address, empty, waiting
to be claimed. The operator sends Dr. Chen a one-time link. Dr. Chen opens it,
sets a passphrase, and the instance is hers. Later you ship 1.5.0 and roll it
out to ten percent of practices, then all of them. When Dr. Chen's practice
closes, you wipe her instance and hand it to the next practice, or destroy it.

Every one of those moments is a **state** on the instance, and every arrow
between them is an **edge** that exactly one actor is allowed to pull. This
skill is the contract for that machine, so a client (dashboard, CLI, agent,
your own admin tool) can drive it without reading the source.

## The states

| state | meaning | who sees it |
|---|---|---|
| `provisioning` | the backing application is being created and the pinned version deployed; a launch that needs deployment approval parks here (`launchApprovalState: pending`) | operator |
| `provisioning_failed` | a launch or a reset ran and failed; `lastError` says why; nothing was torn down | operator |
| `awaiting_claim` | running, empty, one-time claim link exists (or can be minted); nobody owns it yet | operator |
| `awaiting_claim_blocked` | declared in the map (a hold on an unclaimed instance) but **nothing writes it today**; treat as reserved | operator |
| `active` | claimed; the customer's passphrase-derived backup key is on file; serving | operator + customer |
| `suspended` | claimed but taken down (billing, abuse); comes back to `active` | operator |
| `resetting` | being wiped and returned to the pool: data erased, key rotated, identity cleared, redeployed empty | operator |
| `destroying` | the backing application is being torn down | operator |
| `destroyed` | terminal; the row stays for audit | operator |

`launchApprovalState` (`not_required` / `pending` / `approved`) is a mirror of
the platform's deployment-approval gate, only meaningful while `provisioning`.

`updatePolicy` (`auto` / `deferred`) with `updateDeferredUntil` says whether a
fleet rollout may touch this instance.

`pendingUpdate` (`null` / `'size'` / `'variables'`) says a propagation deploy
is in flight; it clears when the platform reports the deploy finished.

## The edges

```
                          ┌──────────────────────────────────────────┐
                          │                                          ▼
   create ──► provisioning ──► awaiting_claim ──► active ◄──► suspended
                 │   ▲             │  ▲  ▲          │             │
                 │   │             ▼  │  │          │             │
                 │   │   awaiting_claim_blocked      │             │
                 │   │                │  │          │             │
                 ▼   │                │  └──────────┴──── resetting ◄┘
       provisioning_failed ───────────┼───────────────────▲   │
                 │      (retry launch)│                   │   │
                 │      (retry reset) └───────────────────┘   │
                 └────────────────────────────────────────────┴──► destroying ──► destroyed
```

The map is a hard allow-list (`instance-state-transitions.ts`). Anything not
listed is rejected with 409 `InvalidInstanceTransition`.

| from | to | pulled by |
|---|---|---|
| `provisioning` | `awaiting_claim` | worker, on a successful launch (claim link minted in the same flush) |
| `provisioning` | `provisioning_failed` | worker, on a failed launch |
| `provisioning` | `destroying` | operator (`DELETE /instances/:id`), even mid-run or parked for approval |
| `provisioning_failed` | `provisioning` | worker, when the operator retries a **launch** (`resume-provisioning`) |
| `provisioning_failed` | `resetting` | operator, when the failed run was a **reset** (`POST /instances/:id/reset` again) |
| `provisioning_failed` | `destroying` | operator |
| `awaiting_claim` | `active` | the customer, on claim (public route) |
| `awaiting_claim` | `awaiting_claim_blocked` | reserved; no writer today |
| `awaiting_claim` | `resetting` | operator (admin) |
| `awaiting_claim` | `destroying` | operator |
| `awaiting_claim_blocked` | `awaiting_claim` | reserved; no writer today |
| `awaiting_claim_blocked` | `destroying` | operator |
| `active` | `suspended` | operator / billing |
| `active` | `resetting` | operator (admin) |
| `active` | `destroying` | operator |
| `suspended` | `active` | operator / billing |
| `suspended` | `resetting` | operator (admin) |
| `suspended` | `destroying` | operator |
| `resetting` | `awaiting_claim` | platform callback + live probe, on a successful wipe deploy |
| `resetting` | `provisioning_failed` | platform callback, on a failed wipe deploy (identity intact) |
| `resetting` | `destroying` | operator |
| `destroying` | `destroyed` | worker |

Three invariants the map encodes on purpose:

- **No state ever goes directly back to `awaiting_claim` except `resetting`
  (and the blocked hold).** Re-claiming a live instance is an account-takeover
  primitive. The only road back to the pool erases everything first.
- **A failed reset retries as a reset, never as a launch.** The launch path
  mints no claim link for a still-claimed instance and would leave the previous
  owner's identity behind an `awaiting_claim` state.
- **`destroying` is reachable from everywhere except `destroyed`.** An instance
  parked for an approval that never comes, or wedged mid-run, must still be
  destroyable, or it keeps billing.

Relay and gateway eligibility (`RELAY_ELIGIBLE_STATES`): `awaiting_claim`,
`active`, `suspended`. **Not** `resetting` (a mid-wipe instance must not
accept an OAuth callback or send an OTP) and not `provisioning`.

## Who can pull which edge (routes on managed-apps)

All operator routes are org-scoped and bearer-authenticated; managed-apps
applies its own tenancy checks on every call. Roles: VIEWER reads, EDITOR
launches and pays, ADMIN defines what the org can deploy and can erase data.

### Templates and versions (`/app-templates`)

| method + path | role | does |
|---|---|---|
| `GET /app-templates` | VIEWER | list |
| `GET /app-templates/:slug` | VIEWER | one template |
| `POST /app-templates` | ADMIN | create (`draft`) |
| `PATCH /app-templates/:slug` | ADMIN | update; `status: published` is what makes it launchable |
| `POST /app-templates/:slug/versions` | ADMIN | register a semver; queues the build-once image + manifest build |
| `POST /app-templates/:slug/versions/:semver/build` | ADMIN | re-run a failed build |
| `GET /app-templates/:slug/versions` | VIEWER | versions with `pending / building / published / build_failed / retired` |
| `POST /app-templates/:slug/relay/ensure` | ADMIN | ensure the template's OAuth relay route exists |
| `GET/PUT/DELETE /app-templates/:slug/variables[/:key]` | VIEWER / ADMIN | variables every instance of the template receives |
| `PUT/GET/POST(verify)/DELETE /app-templates/:slug/domain` | ADMIN / VIEWER | the product's delegated custom domain |

**Publishing a version does not publish the template.** `createInstance`
requires the template `status` to be `published`. If launch 404s with "no
published template", this is why.

### Instances (`/instances`)

| method + path | role | from → to | notes |
|---|---|---|---|
| `GET /instances` | VIEWER | | list |
| `GET /instances/:id` | VIEWER | | the row: state, host, version, claim identity, `resetCount`, `pendingUpdate`, `lastError` |
| `POST /instances` | EDITOR | ∅ → `provisioning` | body `{templateSlug, region, instanceSize?}`; the version is the template's current published one; returns the host immediately, 202 |
| `POST /instances/:id/resume-provisioning` | EDITOR | `provisioning_failed` → `provisioning`, or re-queue a launch parked for approval | the **launch** retry |
| `POST /instances/:id/claim-link` | EDITOR | | reveal the one-time link (purged on reveal), 404 once revealed or claimed |
| `POST /instances/:id/claim-link/reissue` | EDITOR | | new link, old token dead |
| `POST /instances/:id/claim-link/send` | EDITOR | | send by email/SMS without the operator seeing it |
| `PATCH /instances/:id` | EDITOR | stays `active`/`suspended` | body `{instanceSize?, updatePolicy?, updateDeferredUntil?}`; 200 if only policy changed, 202 `{state}` when size changed (`pendingUpdate: 'size'`, an `update` deploy is queued) |
| `POST /instances/:id/apply-variables` | EDITOR | stays | after variables were written, redeploy the same version so the tasks see them (`pendingUpdate: 'variables'`), 202 |
| `GET /instances/:id/deployments?limit=` | VIEWER | | the platform's deployment list for the backing application; follow an update/reset/rollout deploy here |
| `GET/PUT/DELETE /instances/:id/variables[/:key]` | VIEWER / EDITOR | | per-instance variable overrides; writing does **not** redeploy, call `apply-variables` |
| `POST /instances/:id/reset` | **ADMIN** | `awaiting_claim`/`active`/`suspended`/`provisioning_failed` → `resetting` | body `{confirmHost}` must echo the host; 202 `{state}` |
| `DELETE /instances/:id` | EDITOR | any live state → `destroying` | |
| `GET /instances/relay-config/:templateSlug` | VIEWER | | |
| `PUT /instances/relay-config/:templateSlug/{credentials,routes}` | ADMIN | | |
| `POST /instances/claim` | **public** | `awaiting_claim` → `active` | body `{token, backupPublicKey}`; every failure is one identical 404 |

409 codes a client should handle by name:

| code | route | meaning |
|---|---|---|
| `RESET_HOST_MISMATCH` | reset | the echo did not match the host (case-insensitive, trimmed) |
| `RESET_ROLLOUT_IN_PROGRESS` | reset | a fleet rollout item is `updating` this instance |
| `RESET_NOTHING_TO_RESET` | reset | `provisioning_failed` instance that never launched: retry the launch or destroy |
| `UPDATE_ROLLOUT_IN_PROGRESS` | PATCH / apply-variables | same rollout guard |
| `UPDATE_NOT_RUNNING` | PATCH / apply-variables | only `active` / `suspended` can be propagated to |
| `InvalidInstanceTransition` | any | the edge is not in the map |

### Fleet rollouts (`/rollouts`)

| method + path | role | does |
|---|---|---|
| `POST /rollouts` | EDITOR | `{templateSlug, targetSemver, wavePercents? (cumulative, default [10,100]), failureThresholdPercent? (default 10)}`; 201 |
| `GET /rollouts`, `GET /rollouts/:id` | VIEWER | rollout `pending / running / halted / complete / rolled_back` and each item `pending / updating / healthy / failed / rolled_back / deferred` |
| `POST /rollouts/:id/advance` | EDITOR | resume a halted rollout (re-launch pending items, re-evaluate); there is no separate "promote" |
| `POST /rollouts/:id/instances/:instanceId/result` | EDITOR | record an item's outcome by hand (the worker does this itself) |

A rollout is self-advancing: wave N is launched, each item's deploy is followed
to completion, and when the failure share of the wave stays under the threshold
the next wave starts. Instances with `updatePolicy: deferred` (and a future
`updateDeferredUntil`) are skipped as `deferred`. An instance mid-rollout
refuses reset and propagation until its item leaves `updating`.

### Customer-facing (public, no account)

| route | does |
|---|---|
| `POST /instances/claim` | claim: token + browser-derived backup public key |
| `POST /instance-gateway/sms/otp` | template-locked OTP on behalf of an instance |
| `POST /instance-gateway/claim/claimed` | the instance tells the platform its app-side claim finished (`appClaimedAt`) |
| `GET /callback[/:route]` | the OAuth relay (Epic etc.) for eligible states only |

### Worker-facing (HMAC, `/internal`)

These are how the machine actually moves. A client never calls them, but a
client that wants to *watch* the machine should know they exist, because the
row changes when they land, not when the operator route returns.

| route | does |
|---|---|
| `GET /internal/instances/:id/provision-spec` | the worker fetches what to do: `phase: 'provision' | 'reset' | 'update'`, version, size, placement, DNS, whether to rotate the key |
| `POST /internal/instances/:id/provision-progress` | mid-flight checkpoint (so a retry skips finished steps) |
| `POST /internal/instances/:id/provision-result` | launch outcome: `provisioning` → `awaiting_claim` / `provisioning_failed` |
| `POST /internal/managed-instances/deployment-result` | the platform's deployment callback: finishes resets by evidence, clears `pendingUpdate`, records rollout item results |
| `POST /internal/managed-instances/resume-provisioning` | the approval gate releasing a parked launch |
| `POST /internal/template-versions/:id/build-result` | build-once outcome for a version |

## How each edge actually completes

**Launch (`provision`).** Idempotent step list: backing application created
(its id flushed *before* the deploy, so a retry never creates a second one),
pinned version deployed, DNS, `smsHmacKey` minted, one-time claim link minted,
`awaiting_claim`. Each step is skipped when its output already exists. This is
mutation-tested; do not "simplify" the guard away, a retry costs money.

**Claim.** The link points at ForkLaunch (`/claim/:token`), not at the
instance. The passphrase is turned into the backup key **in the browser**
(`client/lib/backup-key.ts`) and only the public half is posted. That file is
the review gate: a server-side derivation would look identical and would end
the "we can never decrypt your backups" guarantee. A lost passphrase means
unrecoverable backups; there is no reset for it.

**Reset.** The operator route only moves the row to `resetting`, forgets the
previous deployment handle, and writes an audit line with a hash prefix of the
displaced owner. The worker fetches a `reset` spec: same version, `wipeData`
deploy, key rotated once. On the platform side a `wipeData` deploy runs the
pool provisioner as deprovision-then-provision for every database (or a
run-once wipe task on the dedicated RDS), sweeps the instance's Redis key
prefix, then deploys. Completion is **by evidence**, not by the worker saying
so: the platform's deployment-result callback must report success *and* a live
`GET https://<host>/health` must return 200 (12 tries, 10 s apart). Only then
are `claimedAt`, `ownerEmail`, `backupPublicKey`, the claim token, `appClaimed*`
and the deferral cleared, in the same flush that mints the new claim link,
stamps `lastResetAt`, and bumps `resetCount`. On failure the row goes to
`provisioning_failed` with the identity intact and the same call retries it.

**Propagation (`update`).** Same version, fresh deploy, no DNS, no wipe, no key
rotation. `pendingUpdate` is the marker; the deployment-result callback clears
it (or sets `lastError` and keeps it so the operator can retry).

**Teardown.** `destroying` is enqueued; the worker tears down the backing
application (snapshots first), then `destroyed`.

## Hooking a client into the machine

1. **Read the row, not the response.** Every mutating route returns 202 with
   the state it moved *to*; the outcome lands later via the internal
   callbacks. Poll `GET /instances/:id` (every 20 s is plenty; a launch is
   minutes, a reset is 3–6 minutes, a resize 2–4 minutes) until the state
   leaves the transitional one (`provisioning`, `resetting`, `destroying`) or
   `pendingUpdate` clears.
2. **Show `lastError` whenever the state is `provisioning_failed` or
   `pendingUpdate` is still set after a deploy finished.** It carries the
   platform's reason (a missing config key, a task that could not pull, an
   IAM denial).
3. **Follow the deploy itself with `GET /instances/:id/deployments`** when
   you need progress rather than a final state; the platform's deployment row
   has `status` and `errorMessage`.
4. **Check live health separately.** The row can say `active` while the
   service is down (a deploy that rolled back, a task that died). Nothing in
   the row proves the app is up except the reset's own probe. A client that
   cares hits `https://<host>/health`.
5. **Render the claim link once, in something the operator must dismiss.**
   It is purged on reveal; a toast loses it.
6. **Treat `awaiting_claim` after a reset as a brand-new instance.**
   `resetCount` and `lastResetAt` are the only trace the previous owner
   leaves. The old claim token, key and app-side claim are gone.
7. **Gate destructive buttons by role and by echo.** Reset needs ADMIN and the
   typed host; destroy needs EDITOR. Do not pre-fill the echo.

## Where each surface stops

| | CLI | managed-apps | `/managed-mode` proxy (dashboard) |
|---|---|---|---|
| List / get templates, versions | ✅ | ✅ | ✅ |
| Create / publish template, version | ✅ | ✅ | ✅ |
| Template and instance variables | ✅ | ✅ | ✅ |
| Launch, list, get, destroy | ✅ | ✅ | ✅ |
| Reveal / reissue / send claim link | ✅ | ✅ | ✅ |
| Resume a parked launch | ✅ | ✅ | ✅ |
| Reset (wipe, return to pool) | pending | ✅ | ❌ not proxied yet |
| PATCH size / policy, apply-variables, deployments | pending | ✅ | ❌ not proxied yet |
| Fleet rollouts | pending | ✅ | ❌ not proxied yet |
| Claim (customer) | ✅ | ✅ public | ✅ `/claim/:token` |

**managed-apps is never called directly by the dashboard.** The dashboard
calls platform-management, which proxies under `/managed-mode` and forwards
the caller's `Authorization` header so managed-apps still applies its own
tenancy checks. Adding a capability means adding it in three places: the
managed-apps handler, the managed-apps SDK export, and the `/managed-mode`
proxy. A handler that is not in the SDK is unreachable; that is exactly how
template publishing went missing once. The rows marked "not proxied yet" are
reachable today only by calling managed-apps with a bearer token (the CLI and
scripts do this); the dashboard work adds the proxies.

## Architecture rules

- **Ambiguous claim failures are collapsed on purpose.** Bad token, expired,
  already-claimed and unknown-id all return one identical 404. Distinguishing
  them lets an attacker probe which links are live.
- **Data wipes only run through the reset.** The public deployment route
  refuses `wipeData` with 409 `WIPE_DATA_INTERNAL_ONLY`; only the internal
  deployment route, called by the managed-apps executor for a `reset` spec,
  may set it.
- **Never merge a platform change while a migration or reset is in flight**
  unless the autodeploy queue is known to serialize behind it; the worker
  that finishes the run must be the one that started it.

## Fields that look wired but are not

- `imageUri` on a template: read by the provisioner, **never written**.
- `stripeProductId`: settable, **billing never reads it**.
- `baseDomain`: read for host allocation, **not in any create or update
  schema**; settable only by writing the column.
- `Application.managedMode`: written by the dashboard at app creation,
  **never read**.

Assume a field is inert until you have found the line that writes it *and* the
line that reads it.

## What has run live (Sep 2026)

Launch, claim, destroy, all six substrate moves (org pool / platform pool /
dedicated in both directions) with source cleanup, and the first reset
(Postgres wipe proven; the Redis sweep grant fixed in #832, the retry edge added
in the same PR). Propagation (`PATCH`, `apply-variables`) and rollouts have unit
coverage and have not yet run against production; the numbers above for their
durations are estimates from the deploy path they share with launch.
