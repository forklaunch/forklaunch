---
name: integrations
description: "Wire third-party services into a ForkLaunch app: GitHub (repo + autodeploy), Stripe (billing/ecommerce keys), managed gateway capabilities for any app (forklaunch infra add <service> email|sms|whatsapp|voice|payments|models), and any other provider whose credentials are environment variables. There is no `forklaunch integrate <service>` — this skill is the real path."
user-invokable: true
---

# Third-party integrations

## When to Use This Skill

- "Connect this to GitHub" / "set up auto-deploy on push"
- "Add Stripe" / "it needs to take payments"
- Any provider whose setup ends in *"here is your API key"* — Sentry, Datadog,
  Twilio, an LLM provider, an SMTP service
- "Send email" from a managed instance or a regularly deployed app —
  `forklaunch infra add <service> email`
  (below); no SMTP or SendGrid key
- A deploy is blocked complaining about a missing environment variable
- "Call patients to remind them" / outbound phone calls (see *Voice calls*)
- "Add AI" / "summarize this with an LLM" without a provider key, or a
  `ai-provider-direct` finding: `forklaunch infra add <service> models` (see
  *Models*)

## Read this first: there is no `integrate <service>` command

`forklaunch integrate` links a **local checkout to a platform application**, and
that is all it does:

```bash
forklaunch integrate --app <platform-application-id>
```

It takes no service name. There is no `integrate github`, `integrate stripe`,
`integrate aws`, `integrate datadog` or `integrate sentry` — if you have seen
those written down, they were wrong. Integrations happen two ways instead:

| kind of integration | mechanism |
|---|---|
| **GitHub** | a first-class command family, `forklaunch github …` |
| **everything else** | credentials as environment variables, `forklaunch config set` |

That split is not arbitrary. GitHub is the only provider the platform itself
holds an installation for (it needs build access to your repository). Every
other provider is your app's business, so the platform's only job is to carry
the secret to the running container.

## GitHub

### 1. Install the app on the org (once per organization)

```bash
forklaunch github status     # check before installing — it is often already done
forklaunch github install    # prints an installation link to open in a browser
```

`install` **prints a link**; it cannot complete the installation for you, because
GitHub requires a human to grant access. Hand the user the URL and wait for them
to say it is done, then re-run `status` to confirm.

`status` reports the installation and this app's repository connection:

```
[OK] GitHub App installed for acme-co (Organization)
[INFO] Installation ID: 157164343
```

### 2. Connect the application to a repository

```bash
forklaunch github connect \
  --repo https://github.com/acme-co/clinic-portal \
  --default-branch main
```

### 3. Autodeploy, if they want push-to-deploy

```bash
forklaunch github connect \
  --repo https://github.com/acme-co/clinic-portal \
  --default-branch main \
  --auto-deploy \
  --release-environment production \
  --region us-west-2
```

Or map branches to environments individually — repeat the flag:

```bash
  --branch-mapping main=production \
  --branch-mapping develop=staging
```

**Say what autodeploy means before turning it on.** Every push to that branch
cuts a release and deploys it, spending real money and changing what customers
see, with no further confirmation. That is a fine default for staging and a
decision worth making deliberately for production. Ask.

`forklaunch github disconnect` removes the repository link and stops autodeploy.
It does not uninstall the GitHub App.

## Stripe

Stripe is not a platform integration — it is your app's own Stripe account. Two
things have to happen: the code has to exist, and the keys have to reach it.

### 1. The code

Stripe support is a **module variant**, chosen at scaffold time or added later:

```bash
# at application creation
forklaunch init application my-app ... --modules billing-stripe --modules iam-better-auth

# or added to an existing app
forklaunch init module billing --path ./src/modules --module billing-stripe --database postgresql
```

`billing-stripe` is subscriptions and billing. `ecommerce-stripe` is one-off
purchases and marketplace payments. Pick before scaffolding — swapping later
means re-scaffolding the module.

### 2. The keys

These are the exact variable names the generated code reads. Getting one wrong
fails at container start, not at build:

| variable | module | what it is |
|---|---|---|
| `STRIPE_API_KEY` | `billing-stripe`, `ecommerce-stripe` | the secret key (`sk_test_…` / `sk_live_…`) |
| `STRIPE_WEBHOOK_SECRET` | both | signing secret for the webhook endpoint (`whsec_…`) |
| `STRIPE_CONNECTED_ACCOUNT_ID` | `ecommerce-stripe` | optional; the merchant account when it is not your own |
| `STRIPE_PLATFORM_FEE_BPS` | `ecommerce-stripe` | optional; your cut, in basis points |

Set them per environment and region:

```bash
forklaunch config set STRIPE_API_KEY=sk_test_... \
  --environment staging --region us-west-2 --service billing

forklaunch config set STRIPE_WEBHOOK_SECRET=whsec_... \
  --environment staging --region us-west-2 --service billing
```

- `--service` scopes the value to one service. Omit it and the value goes to
  **every** service in the app. For a provider key, scope it — a key that only
  billing needs should not be sitting in the environment of every container.
- `--force` sets a variable no component declares. Needed for a variable you
  added to code the platform has not seen a release of yet; otherwise the
  declaration check is doing its job and you should not bypass it.
- Values are **not readable back**. `config pull` returns what is set, but a
  secret you cannot re-read is a secret you must store somewhere else too.

### 3. Getting the key in the first place

Prefer, in this order:

1. **A Stripe MCP server or plugin, if the session has one.** That is the
   genuinely agent-native path — the key never passes through the chat. Check
   before asking the user to paste anything.
2. **Ask the user to fetch it**, naming exactly where it is: Stripe Dashboard →
   Developers → API keys → *Secret key*. Tell them to use a **test** key
   (`sk_test_…`) until the app is real.
3. Never invent, guess, or reuse a key from another project.

**Test keys until launch.** A live key on a half-built app is how test data
becomes real charges. Say this out loud rather than assuming they know.

### 4. Webhooks

Stripe needs a public URL to call, so the webhook can only be registered after
the app has been deployed once and has a host. Sequence it that way: deploy,
read the host, register the endpoint in Stripe, then set
`STRIPE_WEBHOOK_SECRET`. Trying to do it before the first deploy just produces
a URL that 404s.

### 5. Local development

`billing-stripe`'s subscription reads call the live Stripe API on every read, so
a placeholder key fails **locally too**, with `Invalid API Key provided`. There
is no offline mode. Use a real test key in local `.env` files, or expect that
code path to fail on a developer machine.

## Managed gateway capabilities, on any app

Email, SMS, WhatsApp, voice, payments and models are **managed gateway**
capabilities: the platform holds the vendor account and the app holds no
credential. They are not limited to managed instances. `forklaunch infra add
<service|worker> <capability>` declares one; on deploy the platform sets
`PLATFORM_GATEWAY_URL`, `INSTANCE_ID` and `INSTANCE_HMAC_KEY` on the
components the release binds it to (a managed instance gets them from its
provisioner). A worker can call any of them; events are delivered over HTTP to
`/platform-events/<capability>`, so a capability with events also belongs on an
HTTP service if the app needs them (`infra add <worker>` says so when none
holds it).

Say Fit Momma is deployed once, to its own `production` environment, and binds
`sms` to its notify worker and `models` to its coach service. Neither is a
managed instance, yet both work: the first deploy mints one gateway principal
for Fit Momma × production (its id is `INSTANCE_ID`, its key
`INSTANCE_HMAC_KEY`), and every feature keys its state by that id — its own
SMS opt-out list and limits, its own token budget, its own Stripe connected
account, its own Connect number, its own WhatsApp link. Nothing is shared with
a managed instance or another environment.

What a regular deployment gets, per feature (the platform side; the code is
`src/modules/managed-apps/domain/services/app-principal-*.ts`):

| feature | limits (platform default, env) | set up on deploy | notes |
|---|---|---|---|
| email | `EMAIL_GATEWAY_APP_*` (200/day, 30/min) | SES identity | see *Email* below |
| sms | `SMS_GATEWAY_APP_*` (100 segments/month, 10/min), the platform pool | configuration set | transactional only; no dedicated number |
| whatsapp | `WHATSAPP_APP_RPM` (20/min) | the environment's channel, from an operator-set link | no link: sends answer 409 with what to do; HIPAA apps: 403 |
| voice | `VOICE_APP_*` (1 call at once, 100 min/month); flows from `VOICE_APP_FLOWS` or the environment's override | a Connect number, only once flows exist | no flows: calls answer 403; recordings are not copied (no object store) |
| payments | `PAYMENTS_APP_RPM` (100/min); fee from `PAYMENTS_APPLICATION_FEES` (`<application id>` or `*`) | nothing: the connected account is created on the first onboarding link | |
| models | `MODEL_GATEWAY_APP_MODELS` (`luna,terra`), `_MONTHLY_TOKENS` (500k), `_RPM` (20) | nothing | HIPAA (the application's `complianceFrameworks` or the environment's compliance standards): BAA-covered models only |

An operator can override any of these for one environment in the principal's
`settingsOverrides.<feature>` (same fields as the product setting; an invalid
override is ignored). There is no CLI or API for it yet: it is a database edit
of `app_gateway_principal.settings_overrides` in managed-apps. Limits apply on
the next call; a `whatsapp` link or `voice` flows also need a redeploy of the
environment, which writes the channel row or claims the number. A call for a
feature the release does not declare answers **403** (`… does not declare
<feature>; add it with forklaunch infra add …`). Deleting the application or
the environment (or removing its last region) gives the number back, deletes
the SES identity, unlinks WhatsApp and marks the principal deleted, so its key
answers **401**; a Stripe connected account belongs to the business and is
kept. A re-created environment of the same name gets a new key on its next
deploy.

If `INSTANCE_GATEWAY_PUBLIC_URL` is unset on managed-apps there is no
fallback: managed-apps answers the ensure call with 503, the deploy finishes
(platform-management logs an `[AppGatewayPrincipal]` warning), a first deploy
leaves the components without gateway settings (the client throws where it is
used), and a later deploy keeps the settings it had.
Operator setup (env vars, overrides, linking WhatsApp, teardown, deploy
order): the ForkLaunch platform docs (Managed gateway page).

## Email (managed instances and regular deployments)

Neither a managed instance nor an app deployed to its own environment
(`forklaunch deploy create`) holds a mail credential. The platform sends
through Amazon SES from the caller's own sending identity, under its own SES
configuration set, inside its daily quota and rate, and refuses addresses on
its own suppression list (hard bounces and complaints).

| | managed instance | regular deployment (per application × environment) |
|---|---|---|
| sends from | `no-reply@<instance>.<sending domain>` | `no-reply@<app>-<6 hex>.<sending domain>` in `production`/`prod`/`main`; `no-reply@<app>-<env>-<6 hex>.<sending domain>` elsewhere |
| `INSTANCE_ID` / `INSTANCE_HMAC_KEY` | the instance's, from its provisioner | the environment's gateway principal, minted on its first deploy that declares any gateway capability, the same on every redeploy |
| limits | the product's (template), then the instance's override | the platform's application defaults (`EMAIL_GATEWAY_APP_*`), then the environment's override |
| events | the service that declared email | the HTTP service that declared email, at `https://<service>.<app DNS subzone>/platform-events/email`; for a worker, its server component receives them there through `api/platformEvents/email.ts` (the worker process does not). With no public DNS for the app they are recorded (suppression still applies) but not relayed |

For a regular deployment the deploy stores `PLATFORM_GATEWAY_URL`,
`INSTANCE_ID` and `INSTANCE_HMAC_KEY` as platform-owned variables of the
environment × region **only on the services and workers the release binds a
gateway capability to** (component scope). Other components never get them:
the framework reads those three variables as "this is a managed instance", and
a billing service that holds its own Stripe key and did not declare `payments`
would otherwise route Stripe through the gateway and be refused. Run
`infra remove <component> <capability>` and the next deploy removes them
again.
Nobody is asked for them; a value a person set on a component is left alone
(that component then cannot send). An application that backs a managed
instance is skipped: its instance's own values stay.

### 1. What the CLI writes

```bash
forklaunch infra add <service> email
forklaunch infra add <worker> email         # a worker that sends
forklaunch infra remove <service> email     # undoes it
```

Needs `@forklaunch/core` with `createEmailClient` (3.x) and a CLI with
`infra add … email` (1.31+).

- `registrations.ts`: an `EmailClient` built with `createEmailClient()` from
  `@forklaunch/core/http`, fed by the managed env contract
  (`PLATFORM_GATEWAY_URL`, `INSTANCE_ID`, `INSTANCE_HMAC_KEY`, all optional and
  injected by the platform). It is `Lifetime.Scoped`, so a service started
  without the contract boots and fails only where email is used.
- `api/platformEvents/email.ts`: the handler for email events (dedupes on
  `event.id`; a TODO to mark bounced/complained addresses undeliverable), plus
  the verified `/platform-events/:feature` route if the service had none.
- docker-compose: the local gateway mock; `.env.local`: the local contract.
- The manifest: `resources.capabilities = ["email"]`; the release manifest
  carries an `email` resource bound to the service.

### 2. Using it

```ts
const email = ci.resolve(tokens.EmailClient);
const { messageId } = await email.send({
  to: user.email.deanon,               // the recipient may be protected data
  subject: 'Your results are ready',   // NEVER protected data (see checks)
  text: `Hi ${user.firstName.deanon}, your results are in the portal.`,
  tags: { kind: 'results' }            // echoed on the events
});
if (await email.suppressed(address)) { /* ask for another address */ }
```

`send` validates before it calls (addresses, up to 50 recipients, a one-line
subject, `text` or `html`, 512 KB, up to 10 tags) and throws
`EmailRequestError` with the gateway's status: 400 bad input, 401 wrong key,
422 a suppressed recipient, 429 quota/rate (with `retryAfterSeconds`), 503
email not configured on the platform or the sending identity still verifying.
For a product whose manifest declares email, the platform creates the
instance's sending identity while the instance launches (for a regular
deployment, while the environment deploys), so DKIM usually verifies before
the first send; the 503 remains possible right after the first deploy.

#### One-click unsubscribe (bulk mail)

Gmail and Yahoo require bulk senders to offer one-click unsubscribe. Pass
`listUnsubscribe` (never raw headers; there is no headers option) and the
platform adds `List-Unsubscribe` and, with `oneClick`,
`List-Unsubscribe-Post: List-Unsubscribe=One-Click` (RFC 2369 / RFC 8058):

```ts
await email.send({
  to: member.email.deanon,
  subject: 'Your weekly recap',
  html,
  listUnsubscribe: {
    url: `https://app.example.com/unsubscribe?token=${token}`, // https only
    mailto: 'unsubscribe@example.com',                         // optional
    oneClick: true                                             // needs url
  }
});
```

With `oneClick`, the mailbox provider POSTs `List-Unsubscribe=One-Click`
(form-encoded, no cookies) to `url`: that request alone must unsubscribe, with
no confirmation page, so the token in the URL has to identify the person. The
headers go to every address in `to`; for per-person links send one message per
recipient. Rules (client, gateway and mock alike; 400 otherwise): at least one
of `url`/`mailto`; `url` https, printable ASCII up to 512 characters, no
`< > , " \` (percent-encode them); `mailto` a plain mailbox. Needs
`@forklaunch/core` 3.1+ (the first release with `listUnsubscribe`) and a
platform gateway that accepts it; the mock shows it on
`GET /__mock/email/messages` as `listUnsubscribe` and `headers`.

### 3. Events

Delivered signed to `/platform-events/email`, each with the SES `messageId`:
`email.delivered`, `email.bounced` (`bounceType`, `permanent`) and
`email.complained` (`feedbackType`). Deliveries can repeat; dedupe on
`event.id`. Permanent bounces and complaints are already suppressed by the
platform; mark the address undeliverable in the app too.

### Sending from your own domain (Pro)

By default mail goes out from `no-reply@<label>.<sending domain>`. On Pro and
Enterprise (`custom_domains`), a caller can send as
`"Display Name" <local@its-own-domain>` instead. The code does not change; the
gateway picks the From address.

- **Deployed app**: the domain is the application's custom domain
  (`forklaunch app domain set` first), or `<subdomain>.` it; one setting per
  environment.
  ```bash
  forklaunch app domain email set -e production [--from hello] [--name "Fit Momma"] [--subdomain mail]
  forklaunch app domain email status [-e production] [--json]
  forklaunch app domain email remove -e production [-y]
  ```
  `PUT /applications/:applicationId/custom-domain/email-sender`
  `{ environment, localPart?, displayName?, subdomain? }` → one sender;
  `GET …/email-sender[?environment=]` → always an array of senders (`[]` with
  no sender or no custom domain; not plan-gated, a lapsed plan shows
  `status: failed` with the plan message);
  `DELETE …/email-sender?environment=` → `{ message }`, 404 when none is set.
  A sender is `{ environment, domain, fromAddress, displayName?, status:
  pending_verification|verified|failed, dnsRecords: [{type,name,value,required}],
  dnsRecordsWritten, errorMessage? }`. No custom domain: 400. An environment
  not yet deployed with email: saved as `pending_verification` with an
  `errorMessage` and no DNS records, applied by its next deploy (every deploy
  hands the setting to managed-apps, and `null` once the custom domain is gone
  or the plan lost `custom_domains`).
- **Managed instance**: any domain the organization controls.
  ```bash
  forklaunch managed instance email set --id <id> --domain clinic.com [--from care] [--name "Clinic Care"]
  forklaunch managed instance email status --id <id> [--json]
  forklaunch managed instance email remove --id <id> [-y]
  ```
  `PUT /managed-mode/instances/:id/email-sender` `{ domain, localPart?,
  displayName? }` (editor), `GET` (viewer; 204 when none), `DELETE` (editor)
  → back to `no-reply@<host prefix>.<sending domain>`. The answer adds
  `active` (in use right now). A domain not under a public suffix, or the
  platform's own sending domain, is 400; a domain another organization sends
  from or holds as a custom domain is 409; 502/503 when SES or the ownership
  check is unavailable. The same organization may reuse one domain across
  instances and apps.
- **Rules** (the CLI checks them before calling): `localPart` lowercase
  `[a-z0-9._+-]`, not starting or ending with `.`, ≤ 64, default `no-reply`;
  `displayName` one line, no `<>"`, ≤ 64 (non-ASCII is sent RFC 2047
  encoded); `subdomain` one to three DNS labels. No plan: `403 Custom email
  senders require a Pro or Enterprise plan` on set/remove.
- **DNS**: three DKIM `CNAME`s (required) and an advisory DMARC `TXT`
  (`_dmarc.<domain>`, `v=DMARC1; p=none;`, `required: false`). Written by the
  platform only into a Route53 zone it runs for the same organization (a
  delegated custom domain, `dnsRecordsWritten: true`); otherwise publish them.
  SES gives up after 72 hours (`failed`); fix the records and `set` again.
- **Fallback**: until SES verifies the domain, whenever SES refuses it, and
  while the plan lacks `custom_domains`, mail goes from the default `no-reply@`
  address. A custom domain never makes a send answer 503. Events and the
  configuration set (`fl-<id>`) are unchanged.
- **Shared identity**: one SES identity per domain, tagged
  `ForkLaunchEmailSender=custom`; deleted when the last sender using it is
  removed (remove, custom-domain delete, environment teardown, instance
  destroy).

### 4. Local development

`docker compose up` runs the gateway mock (`forklaunch-gateway-mock`). It records
messages (`GET http://localhost:18088/__mock/email/messages`) and emits the
events: `bounce@…` or any address at `bounce.test` bounces (and is then
suppressed: a later send answers 422), `complaint@…` is delivered then
complained, anything else is delivered. `MOCK_EMAIL_DAILY_QUOTA` (default 200)
gives a 429 past it.

### 5. Checks and fixes (`forklaunch score`)

| check | severity | fix |
|---|---|---|
| `email-provider-direct-in-managed` | high | a managed service imports nodemailer, `@aws-sdk/client-ses(v2)`, `@sendgrid/mail`, postmark or mailgun, or reads `SMTP_*`/`SENDGRID_API_KEY`/`POSTMARK_*`/`MAILGUN_*`: run `infra add <service> email`, send with `EmailClient`, drop the SDK and keys |
| `email-protected-data` | high; critical with phi entities | a `.deanon` value in the `subject` of an email send: subjects show in notification previews and mail logs — use a generic subject and keep the detail in the body or behind a link |

## SMS (managed gateway): `forklaunch infra add <service> sms`

Texts go through the platform's AWS End User Messaging SMS gateway. The
service holds **no AWS credential, no Twilio key and no phone number**: it
signs each send with its instance key, and the platform sends from the
product's origination pool under a configuration set named for the instance
(`fl-<instanceId>`), so every receipt and reply is attributed to that instance.

### 1. What the CLI writes

```bash
forklaunch infra add <service> sms        # infra remove <service> sms undoes it
```

- `registrations.ts`: an `SmsClient` built with `createSmsClient()` from the
  gateway settings (`PLATFORM_GATEWAY_URL`, `INSTANCE_ID`, `INSTANCE_HMAC_KEY`,
  all optional; without them the client refuses on the first send, so the
  service still boots).
- `api/platformEvents/sms.ts`: the handler stub for SMS events, plus the
  `/platform-events/:feature` route that verifies them (`verifyPlatformEvent`).
- docker-compose: the `gateway-mock` service, with the service's gateway env.
- the manifest: `resources.capabilities = ["sms"]` (the release manifest turns
  it into an `sms` resource bound to the service).

### 2. Sending

```ts
const sms = ci.resolve(tokens.SmsClient);
const { messageId, segments } = await sms.send({
  to: '+14155550123',                  // E.164 only
  body: 'Your appointment is confirmed. Sign in for details: https://…',
  purpose: 'transactional'             // or 'promotional' (needs marketing consent)
});
```

Refusals arrive as `SmsRequestError` with `status`: 400 (not E.164, empty or
over 10 segments), 422 (the number opted out), 429 (monthly segment cap or
per-minute limit; `retryAfterSeconds` says when), 503 (the platform has no SMS
configured). Bad input throws `SmsValidationError` before any call.

**Never text protected data.** Carriers store texts and lock screens show them.
Send a neutral notice or a sign-in link; show the value after sign-in.

### 3. Events (`api/platformEvents/sms.ts`)

| type | data |
|---|---|
| `sms.delivered` | `{ messageId, to, deliveredAt }` |
| `sms.failed` | `{ messageId, to, reason }` |
| `sms.received` | `{ from, body, receivedAt, keyword? }` (a reply) |
| `sms.opted_out` | `{ phone, optedOutAt }` (they texted STOP) |

Deliveries can repeat; dedupe on `event.id`. STOP/HELP are answered by the
carrier and AWS; after STOP the platform refuses sends to that number (422)
until the person texts START.

### 4. Local

The `gateway-mock` compose service (or `npx -p @forklaunch/core
forklaunch-gateway-mock`) plays the platform and the handset:
`GET /__mock/sms/messages` lists sends; `POST /__mock/sms/inbound
{ "from": "+1…", "body": "STOP" }` simulates a reply; numbers ending in `0000`
fail; `MOCK_SMS_MONTHLY_CAP` (segments, default 100) and `MOCK_SMS_PER_MINUTE`
(default 60) set the limits.

### 5. Checks (`forklaunch score`)

- `sms-provider-direct-in-managed` (high): a managed service imports `twilio`,
  `@aws-sdk/client-pinpoint*`, `@aws-sdk/client-sns` (publishing to a phone),
  `vonage` or `messagebird`. Fix: `infra add <service> sms` and drop the SDK.
  Their keys (`TWILIO_*`, `VONAGE_*`, `MESSAGEBIRD_*`) are reported by
  `managed-provider-credentials`.
- `sms-protected-data` (critical when the app holds phi, else high): a
  `.deanon` value reaches an SMS body. Fix: text a link, not the value.

## WhatsApp (managed gateway)

An app sends WhatsApp through the platform's managed gateway (AWS End User Messaging
Social). It never holds a Meta token or an AWS credential.

**Add it:** `forklaunch infra add <service> whatsapp`. It writes:
- a `WhatsAppClient` registration built with `createWhatsAppClient()` from the gateway settings (`PLATFORM_GATEWAY_URL`, `INSTANCE_ID`, `INSTANCE_HMAC_KEY`, all optional, set by the platform only where `whatsapp` is declared);
- a `/platform-events/:feature` route and the handler stub `api/platformEvents/whatsapp.ts`;
- the local gateway mock in docker-compose, and the manifest capability `whatsapp` (release resource type `whatsapp`).

`infra remove <service> whatsapp` undoes it.

**Use it:**

```ts
const whatsapp = ci.resolve(tokens.WhatsAppClient);
await whatsapp.templates(); // [{ name, language, status: 'APPROVED' | 'PENDING' | …, category }]
await whatsapp.sendTemplate({ to: '+14155550123', template: 'appointment_reminder', language: 'en_US',
  components: [{ type: 'body', parameters: [{ type: 'text', text: 'Tuesday 3pm' }] }] }); // → { messageId }
await whatsapp.sendText({ to: '+14155550123', body: 'Which day works?' }); // only inside the 24-hour window
```

**WhatsApp's rules, enforced by the gateway.** A refusal is a `WhatsAppRequestError` with a `status`:
- **Business-initiated messages must use a Meta-approved template.** An unknown, untranslated or unapproved template is 400.
- **Free text is allowed only within 24 hours of the person's last message** to the business number. Outside that window it is 422; send a template instead.
- **Per-instance rate limit:** 429, with `retryAfterSeconds`.
- **No phone number linked** for the product or instance: 409. Linking a WhatsApp Business Account is a manual Meta embedded signup in the AWS console; the operator then records the phone number id with `forklaunch managed template update --slug <slug> --whatsapp-number-id <phone-number-id-…>` (or `managed instance whatsapp --id <id> --number-id …` for one instance's own number). For a regular app environment the operator sets `settingsOverrides.whatsapp.originationPhoneNumberId` on its gateway principal and redeploys the environment (the ForkLaunch platform docs, Managed gateway page).
- **HIPAA products cannot send WhatsApp at all:** 403. WhatsApp is not covered by the AWS BAA, and Meta signs none.

**Events** arrive at `api/platformEvents/whatsapp.ts`, already verified. Dedupe on `event.id`.
- `whatsapp.received` `{ from, text?, type, receivedAt, messageId }`: an inbound message. It opens the 24-hour window for `from`.
- `whatsapp.status` `{ messageId, status }`: `sent`, `delivered`, `read` or `failed`, for a `messageId` a send returned.

**Local development:** the gateway mock (`routes/whatsapp.mjs` in `forklaunch-gateway-mock`) serves `MOCK_WHATSAPP_TEMPLATES` (default `appointment_reminder:en_US:APPROVED`). It emits `whatsapp.status` after each send.
- `POST http://localhost:18088/__mock/whatsapp/inbound` with `{ "from": "+14155550123", "text": "hi" }` emits `whatsapp.received` and opens the window.
- `MOCK_WHATSAPP_HIPAA=1` makes every send 403; `MOCK_WHATSAPP_UNCONFIGURED=1` makes every call 409; `MOCK_WHATSAPP_RPM` sets the limit.

**Checks** (`forklaunch score`):

| Check | Severity | Fix |
|---|---|---|
| `whatsapp-provider-direct-in-managed` | high | A managed service calls `graph.facebook.com`, imports a WhatsApp SDK (including `@aws-sdk/client-socialmessaging`), or reads `WHATSAPP_*TOKEN`/`META_*TOKEN`. Use `createWhatsAppClient()` and drop the credential. |
| `whatsapp-protected-data` | critical when the service's entities hold `phi`, else high | A `.deanon` value, or a variable assigned from one, is passed to a WhatsApp send. Send a template that says only that something is waiting in the app. |

**Other conversational channels.** The client is channel-neutral (`ConversationChannelClient`: `sendTemplate`, `sendText`, `templates`). Apple Messages for Business is not offered: it has no server API without Apple's approval and a messaging service provider.

## Voice calls (managed gateway)

Outbound phone calls go through the platform's Amazon Connect instance, from a
managed instance or any app deployed on ForkLaunch. The app holds no
AWS key, no Connect instance id and no contact-flow ARN: it asks the platform
to place a call with a **flow name** from its product's catalog, and the
platform dials from the phone number claimed for that instance, inside the
instance's limits. Amazon Connect is on AWS's list of HIPAA-eligible
services, so it can be covered by the AWS BAA on the platform's account
(confirm that agreement is in place before promising it to a health
customer). Either way, **call attributes must never carry health
information**: Connect stores them in its contact records.

### 1. The code: one command

```bash
forklaunch infra add <service> voice      # --dryrun to preview
forklaunch infra remove <service> voice
```

It writes:

- `registrations.ts`: a `VoiceClient` built by `createVoiceClient()` from the
  managed contract (`PLATFORM_GATEWAY_URL`, `INSTANCE_ID`,
  `INSTANCE_HMAC_KEY`, all optional), no vendor keys;
- `api/platformEvents/voice.ts`: a handler stub for call events, plus the
  shared `/platform-events/:feature` route (signed deliveries, verified);
- docker-compose: the `gateway-mock` service, which answers the voice routes
  and delivers events back to this service; `.env.local` gateway settings;
- the manifest: `resources.capabilities = ["voice"]`.

### 2. Using it

```ts
const voice = ci.resolve(tokens.VoiceClient);
const { callId } = await voice.startOutboundCall({
  to: '+15551230000',              // E.164
  flow: 'appointment_reminder',    // a catalog name, never an ARN
  attributes: { appointmentId: appt.id }  // ids only: no .deanon values
});
await voice.call(callId);    // { status: initiated|in_progress|ended, startedAt, endedAt?, durationSeconds?, disconnectReason? }
await voice.endCall(callId); // hang up (reason `api`)
```

Refusals arrive as `VoiceRequestError` with `status`: 400 (bad number, unknown
flow, an ARN, bad or `fl_`-prefixed attributes), 403 (the product has no voice
flows; for a regular app environment, no flows in `VOICE_APP_FLOWS` or its
override), 404 (not this instance's call), 409 (no number claimed yet, or the
call already ended; an app environment claims its number on the deploy after
flows exist), 429 (too many calls at once or the month's minutes spent;
`retryAfterSeconds`), 502/503 (Connect unavailable / not configured).

### 3. Events

Handled in `api/platformEvents/voice.ts`, in this order per call; deliveries
can repeat, so dedupe on `event.id`:

| type | data |
|---|---|
| `voice.call.started` | `{ callId, flow }` |
| `voice.call.ended` | `{ callId, durationSeconds, disconnectReason }` (`customer`, `api`, `busy`, …) |
| `voice.recording.ready` | `{ callId, recordingKey }`, a key in the instance's object store (`voice/recordings/<callId>.wav`); sent only when the product declares an `object_store` resource, since the recording is copied into that bucket |

### 4. Local development

The gateway mock in docker-compose plays the platform: flows from
`MOCK_VOICE_FLOWS` (default `appointment_reminder`), calls end by themselves
after `MOCK_VOICE_CALL_MS` (200), `MOCK_VOICE_RECORD=1` adds a recording
event, `MOCK_VOICE_MAX_CONCURRENT` (2) gives 429s, a number ending in `9999`
is busy, and `MOCK_VOICE_NO_NUMBER=1` gives the 409. Its control endpoints
are on `localhost:18088` (`/__mock/requests?feature=voice`,
`/__mock/events?feature=voice`).

### 5. Checks and fixes

| check | severity | fix |
|---|---|---|
| `voice-provider-direct-in-managed` | high | a managed service imports `@aws-sdk/client-connect`, Vonage, or Twilio's voice API, or reads their ids/keys: run `infra add <service> voice` and use `createVoiceClient()`; drop the SDK and keys (Twilio keys are reported by `managed-provider-credentials`) |
| `voice-protected-data` | high, critical when the service holds `phi` | a `.deanon` value reaches `startOutboundCall` attributes: pass an id and let the contact flow look up what it reads out |
| `capability-wiring` | high | manifest and `registrations.ts` disagree about `VoiceClient`: rerun `infra add` / `infra remove` |

Platform side (for operators): `CONNECT_INSTANCE_ID` enables voice (unset:
503); each product's flow catalog and limits live in
`voice_template_settings` (set them with `forklaunch managed template update
--voice-flow name=<contact-flow-id> --voice-max-concurrent N
--voice-monthly-minutes N`; one instance's limits with `managed instance voice`);
the number per instance in `voice_instance_line`
(claimed when an instance of a product with voice flows is provisioned,
reset or updated, released when it is destroyed; a failed claim does not fail
the launch, it is recorded in the instance's `capability_status` and retried
on the next run, and calls answer 409 until then); recordings are copied from
the Connect bucket into the instance's first `object_store` bucket (an
instance without one, or an adopted one, gets no `voice.recording.ready`);
Connect contact events reach `POST /vendor-webhooks/voice` through an
EventBridge rule to the SNS topic `VOICE_SNS_TOPIC_ARN`. That topic, the rule,
and every other managed feature's topics and IAM grants are created by
`src/modules/managed-apps/scripts/managed-integrations/setup.ts` (dry run by
default, `--apply` to write, never deletes); the order of operations and the
manual vendor steps are in `docs/managed-integrations-setup.md`.

## Payments through the managed gateway (Stripe Connect)

The section above is for an app with its **own** Stripe account. A product sold
as managed instances works differently: say you sell a booking app to dental
practices. Each practice (each instance) takes its own patients' payments into
its own bank account, and none of them should ever hold a Stripe key. The
product's publisher runs a **Stripe Connect platform** linked to ForkLaunch;
every instance gets its **own connected account** under it and is the merchant
of record (direct charges). The instance calls Stripe through the platform,
signed with its own instance key.

### What the CLI writes

```bash
forklaunch infra add <service> payments
forklaunch score --offline      # payments checks should pass
forklaunch infra remove <service> payments
```

- `registrations.ts`: `StripeClient` built by `createStripeClient`
  (`@forklaunch/core/http`). A service with no Stripe gets
  `factory: () => createStripeClient({ Stripe })` and the `stripe` dependency.
  A service that already registers one (billing-stripe, ecommerce-stripe) has
  its factory **switched**, never duplicated:
  `createStripeClient({ Stripe, apiKey: STRIPE_API_KEY })`, with
  `STRIPE_API_KEY` / `STRIPE_WEBHOOK_SECRET` made `optional(...)` (used only
  when the gateway settings are absent). A factory it does not recognise is
  refused, not guessed.
- The gateway contract (`PLATFORM_GATEWAY_URL`, `INSTANCE_ID`,
  `INSTANCE_HMAC_KEY`, all optional), the local gateway mock in docker-compose,
  and the manifest capability (release resource type `payment`).
- `/platform-events/:feature` plus `api/platformEvents/payments.ts` exporting
  `handle(event)`: dedupes on `event.id`, with a TODO where the app records the
  payment. In billing-stripe it hands the event to the existing
  `StripeWebhookService`.

### Using it

```ts
import Stripe from 'stripe';
import { createPaymentsClient, createStripeClient } from '@forklaunch/core/http';

// Onboarding: a Stripe-hosted page for the practice owner.
const payments = createPaymentsClient();
const { url } = await payments.onboardingLink({
  returnUrl: 'https://dental.example.com/settings/payments?done=1',
  refreshUrl: 'https://dental.example.com/settings/payments'
});
const { chargesEnabled, payoutsEnabled, requirementsDue } = await payments.status();

// The real Stripe SDK; requests go to the platform, pinned to this instance's account.
const stripe = createStripeClient({ Stripe });
const session = await stripe.checkout.sessions.create({
  mode: 'payment',
  line_items: [{ price: priceId, quantity: 1 }],
  success_url: 'https://dental.example.com/paid',
  metadata: { appointmentId: appointment.id } // an opaque id, never patient data
});
```

Pass your own `Stripe` class: the registration's `type: Stripe` check uses
`instanceof`, and an ESM service could otherwise get a different build of the SDK.

What the platform does to every call: refuses it before onboarding (409);
forwards only an allowlist (customers, products, prices, plans, checkout and
billing-portal sessions, payment links, subscriptions, payment intents, payment
methods, tax calculations, refunds, invoice reads). Payouts, transfers, bank
accounts and account changes get a 403. It refuses metadata keys named like
protected data (`ssn`, `dob`, `diagnosis`, …) and SSN-shaped text (400),
rate-limits per instance (429), sets `Stripe-Account` to the instance's own
account whatever the app sent, and adds the product's application fee. The SDK
raises its usual typed errors (`StripePermissionError`, …).

The fee and the rate are settings: `forklaunch managed template update --slug
<slug> --payments-fee-percent 2 --payments-fee-amount 30 --payments-rpm 120` for
the product, `forklaunch managed instance payments-gateway --id <id>
--fee-percent 1` for one instance. Unset, the platform's `PAYMENTS_APPLICATION_FEES`
and `PAYMENTS_RPM` apply.

### Events

Stripe's webhooks go to the platform, which checks Stripe's signature and relays
each event once to `/platform-events/payments`: `type` = the Stripe event type
(`checkout.session.completed`, `invoice.paid`, `account.updated`, …), `id` = the
Stripe event id, `data` = the event's object. The app verifies them with
`verifyPlatformEvent` and never needs `STRIPE_WEBHOOK_SECRET`.

### Local mock

`infra add` wires the gateway mock (`npx -p @forklaunch/core forklaunch-gateway-mock`)
into docker-compose. It answers Stripe-shaped objects in memory and enforces
the same allowlist and metadata policy. Onboarding completes at once
(`MOCK_PAYMENTS_AUTO_ENABLE=0` to wait for `POST /__mock/payments/enable`).
`POST /__mock/payments/complete/<checkout session id>` pays a session and sends
`checkout.session.completed`. `MOCK_STRIPE_UPSTREAM=http://localhost:12111`
forwards to the official `stripe/stripe-mock` docker image instead.

### Checks and fixes

| check | means | fix |
|---|---|---|
| `payments-stripe-keys-in-managed` (high) | a managed service declares a Stripe key as required, reads it from `process.env`, or builds `new Stripe(` with no `createStripeClient` | `forklaunch infra add <service> payments`; keep any key `optional(...)` |
| `stripe-webhook-unverified` (high) | a service's endpoints act on Stripe events with neither `constructEvent` nor `verifyPlatformEvent` | verify with `stripe.webhooks.constructEvent(rawBody, signature, secret)`, or receive platform events |
| `payments-protected-data` (critical with phi, else high) | a `.deanon` value is written into `metadata` / `description` / `statement_descriptor` of a Stripe call | send the record id; look the record up when the event comes back |

The platform's metadata policy checks key names only, because it cannot see
where a value came from. The CLI check reads the code, so it is the real guard.
Stripe signs no BAA, so health data must never reach it.

## Models (managed gateway): `forklaunch infra add <service> models`

AI models hosted by the platform (Azure AI Foundry), called with no provider
key. The platform enforces the model allowlist, the monthly token budget and
the rate limit, and offers a HIPAA product only BAA-covered models. Model calls
have no events.

```bash
forklaunch infra add <service> models     # or <worker>; infra remove undoes it
```

- `registrations.ts`: a `ModelGatewayClient` built with
  `createModelGatewayClient()` from `@forklaunch/core/http`, fed by the gateway
  settings (all optional). It is `Lifetime.Scoped`: the client refuses to be
  created without the settings, so a service started without them boots and
  fails only where models are called.
- docker-compose: the local `gateway-mock` (`forklaunch-gateway-mock`), which
  serves the model routes; `.env.local`: the local settings.
- The manifest: `resources.capabilities = ["models"]`; the release manifest
  carries a `models` resource (type `models`) bound to the component.
- An existing direct client (`new OpenAI(...)`, `new Anthropic(...)`, its key,
  its dependency) is **left as it is**: move calls over, then delete it.

```ts
const models = ci.resolve(tokens.ModelGatewayClient);
const reply = await models.chat.completions.create({
  model: 'terra', // a catalog alias
  messages: [{ role: 'user', content: 'Summarize this intake form: …' }]
});
```

`models.chat.completions.stream(...)` yields OpenAI-shaped chunks;
`models.models()` returns the allowed aliases, `monthlyTokenBudget`,
`tokensUsedThisMonth` and `withheldForHipaa`. Refusals are
`ModelGatewayRequestError` with `status`: 400 (no messages, or the model
rejected the prompt), 401 (wrong key), 403 (the alias is not allowed, no model
is enabled, a HIPAA app asked for a non-BAA model, or the environment's release
does not declare `models`), 429 (per-minute limit or the month's token budget;
`retryAfterSeconds`), 502/503 (the provider did not answer / the gateway is not
configured). A regular deployment starts on the platform's application defaults
(`MODEL_GATEWAY_APP_*`, see the table above).

Locally the mock answers `[mock <model>] <your last message>`; `MOCK_MODELS`
(default `sol,terra,luna`), `MOCK_MONTHLY_TOKENS` and `MOCK_FAIL_EVERY` set the
catalog, budget (429) and outages (503). `.env.local` points at it on
`http://localhost:18088` (`INSTANCE_ID=local-instance`,
`INSTANCE_HMAC_KEY=local-dev-instance-key`). Checks: `ai-provider-direct` (a phi
service calls a provider directly; fix: this capability) and
`capability-wiring` (manifest and `registrations.ts` disagree about
`ModelGatewayClient`).

### Moving off a direct OpenAI or Anthropic client

1. `forklaunch infra add <service> models`. The existing client, its key and its
   dependency stay, so calls can move one at a time.
2. Swap each call to the gateway client; the request is OpenAI's chat
   completions shape with a catalog alias as `model`:
   ```ts
   // before: ci.resolve(tokens.OpenAIClient).chat.completions.create({ model: 'gpt-4o-mini', messages })
   const reply = await ci
     .resolve(tokens.ModelGatewayClient)
     .chat.completions.create({ model: 'terra', messages });
   ```
   An Anthropic `messages.create({ system, messages, max_tokens })` becomes
   `chat.completions.create` with `{ role: 'system', content: system }` first
   and the text in `reply.choices[0].message.content`.
3. When nothing uses it, delete the old registration, `OPENAI_API_KEY` /
   `ANTHROPIC_API_KEY` (in code and in each environment), and the `openai` /
   `@anthropic-ai/sdk` dependency.
4. `forklaunch score --offline`: `ai-provider-direct` and `capability-wiring`
   should be clean.

## Everything else

Any other provider follows one shape: **the code reads an environment variable,
and you set it.**

```bash
forklaunch config set SENTRY_DSN=https://... --environment production --region us-west-2
forklaunch config set OPENAI_API_KEY=sk-...  --environment production --region us-west-2 --service worker
```

Two things to check before you set anything:

- **Does the code actually read that name?** Grep the service's
  `registrations.ts`. A variable nothing reads is set successfully and does
  nothing, which is the most confusing possible outcome.
- **Is it already supplied?** The deployment pipeline injects database, cache,
  queue and shared-secret variables automatically (`DATABASE_URL`,
  `REDIS_URL`, `HMAC_SECRET_KEY`, `BETTER_AUTH_SECRET`, `ENCRYPTION_KEY`, and
  the rest). Setting those by hand fights the platform. See `/managed-apps` for
  the full list.

## Compliance boundary — say this when it applies

ForkLaunch-hosted Bedrock model calls are covered by ForkLaunch's BAA. **Nothing
else is.** WhatsApp in particular is covered by no BAA, which is why the platform
refuses every WhatsApp send from a HIPAA product. A direct Anthropic or OpenAI key, Stripe, Twilio, SendGrid, a direct
AWS service — each is a separate processor, and if the app handles health or
other regulated data, the user needs their own agreement with each one. Raise
this the moment a regulated app reaches for a third-party key; it is much
cheaper to hear before the integration than after.

## Plain-English summary

There is no single "integrate" command, and anything that told you otherwise was
wrong. **GitHub** has its own commands — `forklaunch github install` to give the
platform access to the user's repositories (a human has to click through it),
then `connect` to link the app, optionally with push-to-deploy. **Everything
else, Stripe included, is just a secret in an environment variable**, set with
`forklaunch config set`, scoped to the one service that needs it.

For Stripe specifically: choose `billing-stripe` (subscriptions) or
`ecommerce-stripe` (purchases) when scaffolding, set `STRIPE_API_KEY` and
`STRIPE_WEBHOOK_SECRET`, use test keys until the app is genuinely live, and
register the webhook only after the first deploy has produced a public address.

## Related

- `/cli` — `config`, `github` and every other command
- `/deploy-mode` — where the first deploy fits, and the decisions it forces
- `/managed-apps` — per-customer instances, where each customer supplies their own keys
- `/security` — secrets hygiene beyond "set the variable"
