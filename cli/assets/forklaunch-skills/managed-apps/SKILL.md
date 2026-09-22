---
name: managed-apps
description: "Managed mode end to end: publish an app template, launch a per-customer instance, hand over the one-time claim link, and the failure modes (stuck provisioning, DLQ, the decrypt bug, the OAuth relay)."
user-invokable: true
---

# Managed Mode (Managed Apps)

> Companion to `/managed-provisioning`, which covers the same feature from the
> "where does each surface stop" angle. This skill is the operator/agent runbook:
> the exact CLI, the variable system, and every way a launch gets stuck.

## What this is, plainly

You built one app. You want to sell it so that **each customer runs their own
private copy** — their own database, their own deployment, their own web
address — instead of everyone sharing one system.

Say you built a patient-records app for dental practices. Dr. Chen's practice
and Dr. Osei's practice should never share a database. In managed mode:

- The **template** is your app, published once: a git repo plus a list of
  released versions. You own it.
- An **instance** is one running copy — Dr. Chen's — with its own deployment
  and its own web host.
- The **claim link** is the handover: a one-time URL you hand to Dr. Chen. She
  opens it, sets a passphrase, and the instance becomes hers. She never gets a
  ForkLaunch login.

**When to use managed mode vs a plain deploy.** A plain `forklaunch deploy`
gives you *one* running app that *you* operate. Managed mode is for when you are
the vendor and you need *many* isolated copies of the *same* app, one per
customer, each handed off to its owner. If you only ever run one copy, you do
not need any of this.

Everything below drives the ForkLaunch control plane through the `forklaunch
managed` command family, or — for the per-instance lifecycle the CLI does not
cover yet — the `/managed-mode` routes named inline, which the dashboard's
instance page (`/dashboard/managed-apps/instances/:id`) also drives: reset
(admin, typed host echo), resize, update policy, apply-variables, template and
instance variables, the deployment feed, versions, and rollout start/resume.
The app list nests instances under their template, and an instance's
application surface carries a banner linking back to it. **Run `forklaunch login` first** — the single exception
is `instance claim`, which your customer runs and which needs no account.

## The two nouns and the two "publishes"

```
TEMPLATE  (you publish once)                 INSTANCE (one per customer)
──────────────────────────────              ───────────────────────────────
template create        → draft              instance create   → provisioning
template publish       → adds a VERSION           ↓ (worker builds infra + deploys)
template publish-template → published       awaiting_claim
template vars set      → declare env vars   instance claim-link  (operator reveals ONCE)
                                            instance claim       (customer consumes)
                                            → active
                                            instance destroy     → destroyed
```

**The single most common mistake: `publish` and `publish-template` are
different commands, and you need both.**

- `template publish` adds a **version** — a semver pinned to a git ref. A
  template with no version has nothing to deploy.
- `template publish-template` publishes the **template itself**, moving it out
  of `draft`. `instance create` refuses a draft template outright.

So the full path from nothing to launchable is **create → publish →
publish-template**. Doing only `publish` leaves the template a draft, and
`instance create` will 404 with "no published template".

## 1. Template lifecycle

### create (a draft)

```bash
forklaunch managed template create \
  --slug clinic-portal \
  --name "Clinic Portal" \
  --repo https://github.com/your-org/clinic-portal \
  --description "Patient records for dental practices"
```

A new template is a **draft**, and nothing can launch from a draft. `--repo` is
the git repository the platform builds images from — see the buildable-repo
requirement below. Everything else about a template — Stripe product, base
domain, cluster type, frontend domain, default instance size, key-rotation
opt-in — is set afterwards with `template update`.

`--cluster-type` (`template update`) is
where every instance of the template runs, decided once by the publisher — a
customer launching an instance is never asked. Unset means
**org-shared** (the publishing org's shared hosts). Setting it on the
publisher's own application (`forklaunch app hosting`, or the cluster picker in
`deploy create`) does NOT carry over: instances are new applications derived
from the template, not from that app. A hosting type a component declares in
the repo's manifest (`[projects.metadata] hostingType`) still wins over the
template's value.

### publish (a VERSION — platform builds the image)

```bash
forklaunch managed template publish \
  --slug clinic-portal \
  --semver 1.4.0 \
  --git-ref v1.4.0        # tag, branch, or commit sha
```

A newly added version starts **`pending`**: the platform builds the image from
`--git-ref` before any instance can launch from it (statuses: `pending →
building → published`, or `build_failed`; about 30 s when it works). There is
no way to supply a prebuilt image — the version API takes a semver and a git
ref and nothing else. Three rules, each the cause of a real `build_failed`:

- **`--git-ref` is a full SHA, a branch, or a tag** — the builder clones by
  ref name, so a short SHA fails with "Could not find remote branch". Use
  `git rev-parse HEAD`.
- **Publish as the org whose GitHub App installation can read the repo.** The
  clone uses the *publishing* org's installation token
  (`forklaunch github status` shows which account it is on). A template can
  be visible from another org that cannot build it.
- **A failed semver is burned** — republishing the same semver is a 409;
  bump it.

### publish-template (the TEMPLATE itself)

```bash
forklaunch managed template publish-template --slug clinic-portal
```

This is exactly `template update --status published`, under a name that says
what it is for. Until you run it — however many versions the template has — no
instance can be launched.

### update (the general form)

```bash
forklaunch managed template update --slug clinic-portal \
  --name "Clinic Portal" --description "..." \
  --status published \                  # draft | published | retired
  --stripe-product prod_ABC \           # stored, but billing does NOT read it yet
  --cluster-type org-shared \           # org-shared | platform-shared | dedicated — where NEW instances run
  --base-domain buildbespoke.app \      # the zone instances are hosted under
  --frontend-domain app.example.com \   # instance UIs become <hostPrefix>.<domain>; --clear-frontend-domain removes it
  --default-instance-size pico \        # tier new instances launch with; --clear-default-instance-size resets
  --supports-key-rotation               # allow `instance rotate-keys` (services re-encrypt on boot); --no-supports-key-rotation
```

Only the fields you pass change. An empty update is refused (it would report
success while doing nothing). `retired` stops new instances launching. Note:
`--stripe-product` records an id but **nothing in billing reads it today**, so
setting it does not by itself charge anyone.

### list

```bash
forklaunch managed template list                     # published only
forklaunch managed template list --include-unpublished   # + drafts and retired
```

## 2. Template variables — the three kinds

A template **declares what environment variables each of its instances needs**.
`forklaunch managed template vars set` is where the value's *origin* is chosen,
and choosing the right kind is the whole point:

| kind | where the value comes from | needs |
|------|----------------------------|-------|
| `static` | the **same literal** for every instance (`LOG_LEVEL=info`). The template holds it. | `--value` |
| `generated` | a **recipe, not a value**. Each instance derives its OWN, seeded on its instance id. The template stores no secret. | `--generator` |
| `custom` | you type it in **per instance** (one customer's own Stripe key). The template only declares that it exists. | nothing (`--required` optional) |

```bash
# static — one literal shared by all instances
forklaunch managed template vars set --slug clinic-portal \
  --key LOG_LEVEL --kind static --value info

# generated — each instance derives its own secret
forklaunch managed template vars set --slug clinic-portal \
  --key SESSION_SECRET --kind generated --generator 32-bytes-base64

# custom, required, service-scoped
forklaunch managed template vars set --slug clinic-portal \
  --key STRIPE_KEY --kind custom --required \
  --scope service --service billing
```

**If you are about to use `static` for a secret, you almost certainly want
`generated`.** A static secret is one secret shared across every customer, and
it sits in the template at rest. A generated variable stores no secret anywhere:
each instance derives its own value, seeded on the instance id, so a
provisioning retry re-derives the **same** value rather than a new one that no
longer matches what was already deployed.

**Generator recipes** (`--generator`), which are the platform's own
`generateKeyMaterial` vocabulary — a typo resolves to nothing:
`32-bytes-base64`, `64-bytes-base64`, `hex-key`, `key-material`, `private-pem`,
`public-pem`.

**Scope** (`--scope`, default `application`):
- `application` reaches **every service** in the deployed app.
- `service` reaches **exactly one** named service and requires `--service`.
  (`--service` with `--scope application` is refused — it would silently widen.)

**`--required` applies only to `custom`.** A required custom variable with no
value **stops the instance from provisioning** — a deliberate launch-time
failure. static always has a value and generated is always derivable, so neither
can be "missing" and `--required` is rejected on them.

`set` is an **upsert** (same key + scope + service replaces the declaration).
Static values are **not readable back** — `vars list` reports that a value *is
set*, not what it is. To change one, set it again.

```bash
forklaunch managed template vars list --slug clinic-portal   # KEY KIND SCOPE SERVICE REQUIRED SOURCE
forklaunch managed template vars unset --slug clinic-portal --key LOG_LEVEL --scope application
```

### You do NOT declare the standard platform variables

You only declare **app-specific** variables. The standard infrastructure and
secret variables come **for free** — and, importantly, they are supplied by the
**shared platform-management deployment pipeline that every ForkLaunch app
gets**, not by managed-apps. managed-apps only pushes what your `template vars`
declarations resolve to (via `sync-platform-env-vars`, `origin: platform`, which
never clobbers anything a human set on the application). The deploy pipeline
adds, per component, without any `vars set`:

- **Database**: `DB_HOST`, `DB_PORT`, `DB_USER`, `DB_PASSWORD`, `DB_NAME`,
  `DATABASE_URL`, `PGSSLMODE`, `DB_SSL`
- **Cache / queue**: `REDIS_HOST`, `REDIS_PORT`, `REDIS_URL`, `REDIS_TLS`,
  `KAFKA_*` (when those runtime deps are present)
- **Runtime defaults**: `HOST=0.0.0.0`, `PORT=8000`, `PROTOCOL`, `VERSION`,
  `DOCS_PATH`, `OTEL_*`
- **Shared secrets**: `HMAC_SECRET_KEY`, `BETTER_AUTH_SECRET`, `ENCRYPTION_KEY`
  (handled as special shared keys so every service in the app agrees)

(These names live in `platform-management`'s `deployment-processor.service.ts`
`infraVars` and `environment-variable.util.ts` `resolveKnownPlatformVar`,
resolved from Pulumi outputs at deploy time.)

**The managed identity the platform injects per instance** (from
`instance-provisioner.service.ts`, application scope, at provision time — and
re-applied on every managed rollout; a hand-run `deploy create --force` on an
instance's backing app keeps whatever `config set` holds):

| variable | what it is |
|---|---|
| `INSTANCE_ID` / `FORKLAUNCH_INSTANCE_ID` | the managed instance id |
| `INSTANCE_HMAC_KEY` / `FORKLAUNCH_INSTANCE_HMAC_KEY` | the per-instance key the app uses to call the platform gateway (SMS, LLM, app-claimed) and that the relay uses for a `forward` route |
| `PUBLIC_HOST` | the instance's front-door host, `<prefix>-instance.<zone>`; the app puts it in its OAuth `state` (`r:<host>:<nonce>`) so the relay can route the callback back |
| `PLATFORM_GATEWAY_URL` | the managed-apps base URL for gateway calls |
| `PLATFORM_CALLBACK_URL` | the instance-gateway mount the app POSTs lifecycle events to (`/claim/claimed` when its own claim ceremony completes → the platform records `app_claimed_at` and emails the owner) |

Declare a template variable with one of these names only to give it a
placeholder for local runs; the platform's per-instance value wins on
provisioning. A `<service>_…_URL` name (e.g. `IAM_URL`) is auto-filled as the
inter-service URL and must not be declared either.

**Deploy config gate.** Every `getEnvVar('X')` a service reads that is not
typed `optional(...)` in its config injector is a **required key**; a release
whose environment lacks one is parked with "missing keys: X" instead of
deployed. Either declare it on the template or make the read optional.

**Do not ship your own Twilio (or other SMS) credentials in a template.**
Managed mode uses a **platform SMS gateway** (`SmsService`). An instance calls
`POST /sms/otp` on the managed-apps gateway, **authenticated with its own
per-instance HMAC key**: the request's `keyId` is the instance id, verified
server-side against `instance.smsHmacKey`, with a single-use nonce. The instance
sends only a **code and a purpose** (sign-in or claim); the platform renders one
of two fixed, template-locked messages (`renderMessage`, code constrained to
4–10 digits) from **its own** sender identity — the Twilio credentials live only
in the platform's environment and instances never see them. So a compromised
instance can neither impersonate another instance nor send arbitrary content.
Messages are rate-limited per phone (5/hr) and per instance (20/hr, 100/day),
and every send is audited via `SmsDispatchEntity`. (A `log-sms` provider prints
the code in dev when no Twilio creds are configured.)

## 3. Instance lifecycle

### create → provisioning

```bash
forklaunch managed instance create --template clinic-portal --region us-west-2
# override the compute tier (default is pico):
forklaunch managed instance create --template clinic-portal --region us-west-2 --instance-size micro
```

**Compute size.** Managed instances are usually tiny single-tenant apps, so they
default to the **`pico`** tier (~0.1 vCPU / 256 MB per service) on the shared-org
EC2 pool — much smaller than the platform-wide `micro` default. `pico` is
EC2-only (below AWS Fargate's 256-CPU floor), which is exactly where managed
instances run. Pass `--instance-size <pico|nano|micro|small|…>` to pin an
instance higher; a template may also opt an individual service higher in its
manifest (the per-service size wins). The size is stamped onto every service in
the deployable manifest at provision time.

The template must be **published** and have a **built** version. The call
returns immediately with the instance in `provisioning`; the actual work runs in
a background worker:

1. **build** — the pinned version's image (done at publish time)
2. **infra** — a backing Application is created (idempotent: the `applicationId`
   is flushed *before* the deploy, so a retry never creates a second, billable
   application)
3. **services** — env vars resolved and written, then the version deployed
4. a per-instance SMS/LLM gateway key (`smsHmacKey`) is minted
5. a **one-time claim link** is minted; state moves to `awaiting_claim`

Watch it with `forklaunch managed instance get --id <id>` (or `instance list
--state provisioning`). Two things that look like a slow launch and are not:

- **`launchApprovalState: pending`** — the org gates production deploys, so
  the launch deployment is parked `awaiting_approval` (`requestedBy: system`,
  so any admin can approve). `forklaunch deploy approvals list --status
  pending` → `deploy approvals approve --id <id>`; the launch resumes at once.
  Or turn the gate off for production (`/deployment-approvals`).
- **`awaiting_claim` before the app answers** — until #861 is everywhere,
  the state can flip when the claim link is minted, with the deployment still
  `deploying`. Confirm `instance deployments --id <id> --limit 1` says
  `completed` and the instance's `/health` answers before sending a link.
  (A local `curl` that cannot resolve a host `dig` resolves is your own
  resolver's negative cache from an earlier probe.)

A launch on the shared pool takes 6–8 minutes. The lifecycle states, in order: `provisioning → provisioning_failed?
→ awaiting_claim → active → suspended? →
destroying → destroyed`. A claimed instance can **never** step straight back
to `awaiting_claim` (that would be an account-takeover primitive); `destroyed`
is terminal. (`awaiting_claim_blocked` exists in the transition map but
nothing writes it today — reserved.)

The one sanctioned way back to the claimable pool is a **reset**:
`awaiting_claim | active | suspended → resetting → awaiting_claim`
(or `→ provisioning_failed`, where a retry re-runs the reset; or `→
destroying`). A reset wipes the instance's data with a `wipeData` deployment,
rotates its key, and only once the wipe succeeded *and* the instance answers
on its host (the deployment-result callback plus a live `/health` probe, 12 ×
10 s) clears the previous owner (`claimedAt`, `ownerEmail`, backup key, claim
token, `appClaimed`, any deferral) in the same flush that mints a fresh claim
link and bumps `resetCount` / `lastResetAt`. While `resetting` the instance is
neither relay- nor gateway-eligible. A failed reset lands in
`provisioning_failed` with identity intact; the same call retries it.

```http
POST /managed-mode/instances/:id/reset   { "confirmHost": "<the instance host, echoed back>" }   → 202 { "state": "resetting" }
```

Platform admin only. Allowed from `awaiting_claim | active | suspended`, and
from `provisioning_failed` as the retry. 409 codes: `RESET_HOST_MISMATCH`
(the echo did not match), `RESET_ROLLOUT_IN_PROGRESS`, `RESET_NOTHING_TO_RESET`
(a `provisioning_failed` instance that never launched — retry the launch or
destroy it instead); any other state is an invalid transition.

```bash
forklaunch managed instance reset --id <instance-id> --confirm-host <the instance host>   # admin; prompts for the host on a TTY
forklaunch managed instance get --id <instance-id>                                       # resetting → awaiting_claim
forklaunch managed instance claim-link --id <instance-id>                                # a fresh one-time link
```

What you will see, as observed on the org pool (3–4 minutes end to end):

1. `POST …/reset` → `202 { "state": "resetting" }`; the row shows
   `latestDeploymentId` for the wipe deploy and `lastError` cleared.
2. The wipe deploy runs: each component database dropped and recreated, the
   instance's Redis key prefix swept, then the services deploy and run their
   own migrations on the empty databases.
3. The platform's deployment callback plus a live `/health` probe flip the row
   to `awaiting_claim`: `claimedAt` / `ownerEmail` null, `resetCount` + 1,
   `lastResetAt` set.
4. `POST …/instances/:id/claim-link` → a fresh one-time link (72 h); the
   pre-reset token is dead.

If the wipe deploy fails, the row lands in `provisioning_failed` with
`lastError` (`Application deployment failed: …`) and identity intact; the
same reset call retries it.

What the wipe does: on a dedicated substrate it is a run-once task on the
instance's own RDS that drops and recreates each component database (named
as the services' `DB_NAME`); on a pool it is the partition provisioner
dropping and recreating the partition's databases and sweeping its Redis
keys. Either way the redeploy's migrations rebuild empty schemas; backups
encrypted to the previous owner's key are left untouched and unreadable by
anyone.

### Instance vars (custom values) — can come BEFORE create

If the template declares a **required custom** variable, the instance will not
provision until it has a value. Set values per instance:

```bash
forklaunch managed instance vars list --id <instance-id>    # KEY ... VALUE(SET/MISSING)
forklaunch managed instance vars set  --id <instance-id> --key STRIPE_KEY --value sk_live_...
forklaunch managed instance vars unset --id <instance-id> --key STRIPE_KEY
```

`vars list` never prints a value — for custom it says `SET` or `MISSING`, for
static `(from template)`, for generated `(derived per instance)`. A row that is
both **REQUIRED and MISSING** is flagged as blocking the provision. There is no
`--scope`/`--service` here: the template's declaration already fixed the
scoping; an instance supplies the value, not the scoping.

### claim-link (operator reveals — ONCE)

```bash
forklaunch managed instance claim-link --id <instance-id> --json > claim.json
```

(`--json` still prints the `[WARN] … purged` line first; parse from the
first `{`.) The link is `https://forklaunch.com/claim/<token>`: the customer
chooses a 12+ character passphrase, optionally an email for the claim
notice, acknowledges that nobody can recover it, and the page ends by
showing their instance's hosts (and UI link, given a `frontendDomain`).

**This can only be done once.** Revealing the link **purges it** from the
platform — the value is erased the moment it is returned to you. If you lose it,
the only remedy is to destroy the instance and launch a new one. Capture the
output; do not run it "just to check". A claim link only exists while the
instance is `awaiting_claim` and unexpired (72h TTL); otherwise the command
reports no link available. `--dryrun` does NOT consume it.

### endpoints — pointing a frontend at an instance

Every instance carries `hostPrefix` (public, e.g. `clinic-portal-a1b2c3`),
`endpoints` (one https base URL per HTTP component, derived from the release
manifest) and, once the template has a `frontendDomain`, `frontendUrl`
(`https://<hostPrefix>.<frontend domain>`). `instance list --json`, the
instance GET and the claim response all return them; the claim page shows the
customer their `frontendUrl`. How to build the Vercel side (edge middleware per
instance, why a subdomain per instance and not a shared origin) and the prompt
to hand a coding agent: `docs/managed-instance-frontend.md`.

### claim (customer consumes — no login)

Your customer runs this on their own machine, with the link you gave them:

```bash
forklaunch managed instance claim \
  --id <instance-id> \
  --token <one-time-token> \
  --backup-public-key age1...    # an age RECIPIENT derived from their passphrase
```

This is **public** — the one-time token is the credential; there is no
ForkLaunch account. The platform stores **only the public half** of the backup
key and can never decrypt the customer's backups. **A lost passphrase means
unrecoverable backups — permanently, no support path.** The customer must
derive `age1...` in their browser / on their machine and store the passphrase in
a password manager *before* claiming. A bad/expired/already-used token and an
unknown id all fail as **one identical error**, deliberately, so the endpoint
cannot be used to probe which links are live.

### destroy

```bash
forklaunch managed instance destroy --id <instance-id>            # prompts to retype the id
forklaunch managed instance destroy --id <instance-id> --confirm  # CI / scripts
```

Irreversible; no backup is taken first. Teardown runs in the background
(`destroying → destroyed`); the row says `destroyed` within seconds, while
the services and DNS take some minutes to actually go — `instance list`
hides destroyed rows, `instance get` still answers. The prompt is never
shown when stdin is not a terminal, so a forgotten `--confirm` fails fast
instead of hanging CI.

### The whole lifecycle, as run unattended

Provision → platform claim → the product's own claim → sign-in → reset →
destroy was run end to end on a fresh Health Vault instance on 2026-09-21
(45 minutes including two fleet rollouts). The only human steps are the
two the customer does in a browser; everything else is the CLI above. What
the run taught, in the order an operator meets it: the approval park on a
gated org; wait for the vault, not the state; `--json` on `claim-link`
prints a WARN line before the JSON; the product's own claim link is minted
with the instance's `HMAC_SECRET_KEY` (copy the whole value — base64 ends
in `=`); a template is buildable only by the org whose GitHub App
installation reads the repo, from a full SHA/branch/tag, on a fresh
semver; a two-instance rollout is two waves of ~4 min; a reset on a
claimed instance is 3 min to `awaiting_claim` with both claims cleared.
The product-side recipe with exact commands is the Health Vault
`operations.md` §1b.

### summary

```bash
forklaunch managed summary            # instances + sign-in eligibility + relay callback URL
forklaunch managed summary --json
```

The one place that shows, in one call, which instances are running, whether
sign-in would actually work for each (relay eligibility), and the exact OAuth
callback URL to register per template.

### Rolling a new version to existing instances — fleet rollouts

Publishing a version does **not** move running instances. A **fleet rollout**
moves every instance of a product to a published version in waves, and is
the managed path:

```bash
forklaunch managed rollout start --template clinic-portal --semver 1.4.0 --waves 10,100 --halt-above 10   # POST /managed-mode/rollouts → 201
forklaunch managed rollout list [--template clinic-portal]        # every rollout, newest first
forklaunch managed rollout get --id <rollout-id>                  # waves + per-instance items
forklaunch managed rollout advance --id <rollout-id>              # RESUME a halted/restarted rollout — not "next wave"
# record an outcome by hand if the platform callback did not (managed-apps direct):
POST /rollouts/:id/instances/:instanceId/result
```

- `wavePercents` is **cumulative fleet coverage**, default `[10, 100]`: a 10 %
  canary, then everyone. `failureThresholdPercent` (default 10) halts a wave
  once more than that share of its instances fail.
- Each instance's deploy outcome arrives from the platform automatically and
  halts or advances the rollout. **There is no promote step and no approval
  gate on an update**: the rollout launches the next wave on its own once the
  current one stays healthy; the canary is the control. `advance` re-launches
  not-yet-started items of the current wave and re-evaluates — safe to repeat,
  never skips a wave.
- Item states: `pending / updating / healthy / failed / rolled_back /
  deferred`. An instance with `updatePolicy=deferred` is recorded as deferred
  and skipped.
- Rollouts and the per-instance deploy below are two paths to the same
  backing application; never run both on one instance at once (a reset is
  refused while a rollout item is `updating`).

**Fallback — one instance by hand.** Deploy the release to that instance's
backing application from a checkout whose `.forklaunch/manifest.toml` carries
the instance's `platform_application_id` (`instance list --json` →
`applicationId`):

```bash
forklaunch release create -v 1.4.0 -n "…" --local -y
forklaunch deploy create -r 1.4.0 -e production --region us-west-2 --no-wait --force
```

`--force` is the CLI's "yes, I mean to deploy a template repo as a single
app" switch; without it the managed-template guard refuses. Per-instance
config for that path is `forklaunch config set -e production -r <region>
--force KEY=value` on the same checkout.

### What reaches an instance that is already running

`template vars set` and `instance vars set` **write the declaration only**;
`forklaunch config set` (and the dashboard env editor) write the instance's
application config. None of it reaches the running tasks until the next
deploy of that instance. These make that deploy without waiting for a launch,
reset or rollout (control plane routes in parentheses):

```bash
forklaunch managed instance apply-variables --id <id>                 # re-resolve variables, redeploy the SAME version → 202 (POST …/apply-variables)
forklaunch managed instance update --id <id> --size micro             # resize: redeploys the current version → 202     (PATCH …/instances/:id)
forklaunch managed instance update --id <id> --update-policy deferred --deferred-until 2026-10-01T00:00:00Z   # no redeploy → 200; --clear-deferral
forklaunch managed instance deployments --id <id> --limit 20          # the platform's deployment list for the instance (GET …/deployments)
forklaunch managed instance get --id <id> --json                      # the full lifecycle row: instanceSize, updatePolicy, pendingUpdate, latestDeploymentId, lastResetAt, resetCount, keyGeneration, appClaimedAt …
forklaunch managed instance resume --id <id>                          # re-queue a failed launch, or release an approval hold (NOT the reset retry)
```

- An update deploy is the current version only: no DNS, no wipe, no key
  rotation. The instance carries `pendingUpdate = 'size' | 'variables'` until
  the platform's deployment-result callback clears it; on error `lastError` is
  set and the marker stays so you can retry the same call.
- Refused with 409 `UPDATE_ROLLOUT_IN_PROGRESS` while a fleet rollout item is
  updating this instance, and `UPDATE_NOT_RUNNING` unless the instance is
  `awaiting_claim`, `active` or `suspended` (the propagatable states).
- `--instance-size` on `instance create` sets the launch size; the PATCH
  above is the resize after launch. A fleet-wide change is `template update`
  default size + a rollout.
- Follow any update / reset / rollout deploy to completion with
  `GET /instances/:id/deployments` (`status`, `errorMessage` per deployment).

### Rotating an instance's secrets

```bash
forklaunch managed instance rotate-keys --id <id> --confirm-host <the instance host>   # admin → 202 { state, keyGeneration }
```

Every `generated` template variable is re-derived for a new generation, the
earlier generations are delivered to the app as `LEGACY_<KEY>S` (newest
first, comma-separated), the gateway HMAC key is minted anew, and the current
version is redeployed; the instance carries `pendingUpdate = 'keys'` until the
deployment-result callback confirms deploy success and a live `/health`.
`keyGeneration` (0 at launch) and `lastKeyRotationAt` are on the row. **The
app must re-encrypt on boot** from `LEGACY_<KEY>S`, and the template must
declare that it does (`template update --supports-key-rotation`); otherwise
409 `ROTATE_UNSUPPORTED_BY_TEMPLATE`. Other 409s: `ROTATE_HOST_MISMATCH`,
`ROTATE_NOT_RUNNING`, `ROTATE_ALREADY_PENDING`, `ROTATE_ROLLOUT_IN_PROGRESS`.

## 4. Failure modes & troubleshooting

**Instance stuck in `provisioning` (never reaches `awaiting_claim`).** The
background job almost certainly **dead-lettered**. Check the dead-letter queue
and the platform logs:

```bash
forklaunch dlq stats            # counts per queue
forklaunch dlq retry ...        # requeue a dead-lettered job
forklaunch dlq remove ...       # drop one
```

The provisioning worker is a **database worker** (the queue lives in the same DB
as the instance row, so job and state commit together). On failure the
provisioner records `lastError` on the instance and moves it to
`provisioning_failed`, which `instance list` surfaces in its Errors section. A
`provisioning_failed` instance can re-enter `provisioning` — retry is a
supported path, not a stuck row.

**Encrypted columns need the tenant first.** Instance rows (`smsHmacKey`,
custom variable values) are encrypted under the instance's *organization*;
template rows under the no-tenant context. Any route or job that reads one
without a session must resolve the organization from plain columns first
(`resolveInstanceOrgById` / `resolveInstanceOwnerByHost`) and fork a
tenant-scoped EM, or the decrypt derives the wrong key and fails. The
provisioning paths all do this now (`loadInstanceForProvisioning`,
`withEncryptionContext`); keep the rule when adding a route.

**Version won't build / instance create fails.** The git repo must be buildable
by the **publishing org's** ForkLaunch GitHub App installation (`forklaunch
github status`), `--git-ref` must be a full SHA/branch/tag, and the semver
must be new. The worker's dead-letter entry carries git's own message
(`forklaunch dlq stats`; "Repository not found" = wrong installation,
"Could not find remote branch" = short SHA).

**Instance up but sign-in fails.** `forklaunch managed summary` shows relay
eligibility per instance and the product's routes; an ineligible instance
(`resetting`, `destroying`, …) has its OAuth callback refused, and a product
with no route declared runs the legacy Epic path. `suspended` stays
relay-eligible on purpose, so a mid-flight sign-in fails at the (down) instance
rather than looking like a relay misconfiguration. The relay's own reasons are
logged under `[Relay]` — the table is in `/managed-relay`.

**"no published template" on `instance create`.** The template is still a draft
— you ran `publish` (a version) but not `publish-template`. See section 1.

## 5. The OAuth relay for hosted instances

Hosted instances that sign users in through an external provider (Epic is the
motivating case) share **one** registered redirect URI per product:

```
callback URL:   https://relay-<templateId>.<platform zone>/callback      (from `managed summary`)
state format:   r:<instance host>:<nonce>                                (the instance mints it)
```

The relay parses the `state`, checks the host is an eligible instance of this
product, burns the nonce (single-use, 10 min), and hands the callback to the
instance according to a **relay route** the product declared on its template
— `{ name, component, path, mode }`:

- **`redirect`** (browser OAuth, and what Epic needs): 302 the browser to
  `https://<prefix>-<component>.<zone><path>` with the provider's query
  intact; the instance finishes the exchange with its own PKCE verifier. No
  provider secret on the platform.
- **`forward`** (webhooks): HMAC-signed POST to the component over the mesh.

```http
forklaunch managed template relay list  --slug <slug>            # the callback URL to register + every route with its URL
forklaunch managed template relay set   --slug <slug> --name default --component vault --path /epic/callback --mode redirect
forklaunch managed template relay clear --slug <slug> --name <name> | --all
# (PUT /managed-mode/templates/<slug>/relay-routes — whole-list replace; `set` splices one route in)
```

`default` is served at the bare `/callback`; other names at
`/callback/<name>`, each with its own URL in `managed summary`
(`relayConfigs[].routes[].callbackUrl`). Publish rejects a route whose
component does not serve the path. A template with no routes keeps the legacy
Epic exchange+forward path. Everything else — the DNS/ALB plumbing, the
per-mode contract, the debugging table — is in `/managed-relay`.

## Plain-English summary

Managed mode lets you sell one app as many private copies. You **publish a
template** (create → publish a version → publish-template — the last two are
different and both required), **declare its app-specific env vars** (static =
shared literal, generated = per-instance secret from a recipe, custom = filled
in per customer; the shared platform deploy pipeline injects the standard
DB/Redis/secret vars for free, and the platform runs the SMS gateway, so don't
ship Twilio creds), then **launch an instance per
customer** and hand them a **one-time claim link** they consume with no login.

When a launch gets stuck in `provisioning`, a required custom variable is
missing or the job dead-lettered — check `instance vars list`, `forklaunch
dlq stats` and the logs. Confirm the repo is buildable by the ForkLaunch
GitHub App. For provider sign-in, register the product's relay URL once and
declare a relay route pointing at the component that finishes the flow. To
give an instance back to the pool, reset it (never re-claim it).

## Still manual today

- A product's own post-claim ceremony (Health Vault's phone claim) mints its
  link with an operator script; minting it from the platform at claim time is
  the intended end state.
- Recording a rollout item's result by hand is managed-apps-direct (no CLI).
