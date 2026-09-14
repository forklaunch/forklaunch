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
managed` command family. **Run `forklaunch login` first** — the single exception
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
requirement below. The Stripe product is set later with `template update
--stripe-product <id>`; the base domain is not settable through the API at all
(instances use the platform-wide default).

### publish (a VERSION — platform builds the image)

```bash
forklaunch managed template publish \
  --slug clinic-portal \
  --semver 1.4.0 \
  --git-ref v1.4.0        # tag, branch, or commit sha
```

A newly added version starts **`pending`**: the platform builds the image from
`--git-ref` before any instance can launch from it (statuses: `pending →
building → published`, or `build_failed`). There is no way to supply a prebuilt
image — the version API takes a semver and a git ref and nothing else. The git
ref must live in a repo the **ForkLaunch GitHub App can build** (see failure
modes).

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
  --status published \        # draft | published | retired
  --stripe-product prod_ABC   # stored, but billing does NOT read it yet
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

> **Accuracy caveat.** The managed-*identity* names some docs list —
> `PUBLIC_HOST`, `INSTANCE_ID`, `INSTANCE_HMAC_KEY`, `PLATFORM_*` — are **not
> actually emitted as environment variables** by any code found in the module.
> The per-instance HMAC key (`smsHmacKey`) is minted and stored on the instance
> row but used **server-side** (see the SMS gateway below), not injected into the
> app's environment. Treat those four names as intended contract, not verified
> behavior; confirm against the live env before depending on one.

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

Watch it with `forklaunch managed instance list` (or `--state provisioning`).
The lifecycle states, in order: `provisioning → provisioning_failed?
→ awaiting_claim (/ awaiting_claim_blocked) → active → suspended? →
destroying → destroyed`. A claimed instance can **never** re-enter
`awaiting_claim` (that would be an account-takeover primitive); `destroyed` is
terminal.

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
forklaunch managed instance claim-link --id <instance-id>
```

**This can only be done once.** Revealing the link **purges it** from the
platform — the value is erased the moment it is returned to you. If you lose it,
the only remedy is to destroy the instance and launch a new one. Capture the
output; do not run it "just to check". A claim link only exists while the
instance is `awaiting_claim` and unexpired (72h TTL); otherwise the command
reports no link available. `--dryrun` does NOT consume it.

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
(`destroying → destroyed`). The prompt is never shown when stdin is not a
terminal, so a forgotten `--confirm` fails fast instead of hanging CI.

### summary

```bash
forklaunch managed summary            # instances + sign-in eligibility + relay callback URL
forklaunch managed summary --json
```

The one place that shows, in one call, which instances are running, whether
sign-in would actually work for each (relay eligibility), and the exact OAuth
callback URL to register per template.

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

**KNOWN BUG (being fixed) — declared custom instance vars block provisioning.**
`InstanceVariableEntity.value` is a compliance-classified, encrypted column
(`fp.text().compliance('pci')` on a `defineComplianceEntity`). MikroORM's
encrypted column type **needs a tenant-scoped EntityManager to decrypt** — an
`em` created with no `{ context: { tenantId } }` reads with an empty tenant id
and decryption fails ("Failed to decrypt encrypted column value"). Crucially,
the column is decrypted whenever an `InstanceVariableEntity` row is **hydrated**,
regardless of which field you actually read.

The failing path is the **provisioning worker**. `provisioning-worker.handlers.ts`
resolves a **bare** EntityManager (`ci.resolve(tokens.EntityMgr)`, no tenant
context) and passes it into the provisioner, which calls
`TemplateVariableResolver.resolve` — that loads the instance's
`InstanceVariableEntity` rows and reads `value`. With no tenant context on the
`em`, the decrypt throws, the worker records a failure, and **the job
dead-letters**. Effect: **any instance whose template declares a `custom`
variable is blocked; instances with no declarations are unaffected** (the
resolver short-circuits on an empty declaration list). The same hydration hazard
exists in `VariableService.listInstanceVariables` when reached without tenant
context, even though it only reads the `key` — the correct read endpoints pass
`{ context: { tenantId: req.session.organizationId } }`, but the worker has no
session. If you see that decrypt error, or a stuck instance that has custom
vars, this is the cause — check `forklaunch dlq stats` and the managed-apps
logs.

**Version won't build / instance create fails.** The git repo must be buildable
by the **ForkLaunch GitHub App** — the App has to be installed on the repo with
build access, and `--git-ref` must exist. A version stuck at `pending`/`building`
or landing in `build_failed` points here.

**Instance up but sign-in fails / host not reachable.** The instance's host
needs **DNS/relay set up** (the `relay.<zone>` record and the instance's own
host record). `forklaunch managed summary` shows relay eligibility per instance;
an ineligible instance has its OAuth callback refused. `suspended` stays
relay-eligible on purpose, so a mid-flight sign-in fails at the (down) instance
rather than looking like a relay misconfiguration.

**"no published template" on `instance create`.** The template is still a draft
— you ran `publish` (a version) but not `publish-template`. See section 1.

## 5. The OAuth relay (Epic-style) for hosted instances

Hosted instances that sign users in through an external OAuth provider (the
motivating case is **Epic**) share **one** registered redirect URI. An OAuth
provider registers a single `redirect_uri` per client id, but there are many
instances on many hosts — so every instance points its callback at the
**platform relay**, and carries its own host inside the OAuth `state`:

```
state format:  r:<host>:<nonce>        (exactly three colon-separated parts)
callback URL:   https://relay.<zone>/callback   (one per template/product, from `managed summary`)
```

The relay parses the `state`, pulls the per-instance `<host>` out of it, and
forwards the callback to that instance. Key properties:

- **One redirect per product, not per instance.** The URL is derived per product
  from the zone its instances live under (each product has its own OAuth client
  id), never a single platform-wide URL.
- **Single-use state.** The relay claims each state atomically in Redis (`SET NX
  PX`, 10-minute TTL); a replay is refused, so the relay cannot be used as a
  redirect oracle.
- **Host is validated** against a strict pattern, and the three-part split means
  a nonce can never smuggle a second host.
- Only instances in a **relay-eligible state** (`awaiting_claim`, `active`,
  `suspended`) are routed.

Register the callback URL from `forklaunch managed summary` with your OAuth
provider once per template; all its instances share it.

## Plain-English summary

Managed mode lets you sell one app as many private copies. You **publish a
template** (create → publish a version → publish-template — the last two are
different and both required), **declare its app-specific env vars** (static =
shared literal, generated = per-instance secret from a recipe, custom = filled
in per customer; the shared platform deploy pipeline injects the standard
DB/Redis/secret vars for free, and the platform runs the SMS gateway, so don't
ship Twilio creds), then **launch an instance per
customer** and hand them a **one-time claim link** they consume with no login.

When a launch gets stuck in `provisioning`, the job dead-lettered — check
`forklaunch dlq stats` and the logs. The live gotcha to know: a template with
**custom instance variables** can trip a **known decryption bug** (the encrypted
`value` column is read without tenant context → 500 → DLQ), which is being
fixed. Also confirm the repo is buildable by the ForkLaunch GitHub App and that
the instance's DNS/relay is set up.

## Not fully verified

Managed mode is newer than the rest of the platform and parts are still being
wired. Treat the standard-injected-variable **naming** as the documented
contract, not a guarantee that every listed name is already emitted verbatim by
the control plane — confirm against the live `sync-platform-env-vars` payload
before depending on a specific key. The decrypt bug above is real and
outstanding at time of writing.
