---
name: integrations
description: "Wire third-party services into a ForkLaunch app: GitHub (repo + autodeploy), Stripe (billing/ecommerce keys), and any other provider whose credentials are environment variables. There is no `forklaunch integrate <service>` — this skill is the real path."
user-invokable: true
---

# Third-party integrations

## When to Use This Skill

- "Connect this to GitHub" / "set up auto-deploy on push"
- "Add Stripe" / "it needs to take payments"
- Any provider whose setup ends in *"here is your API key"* — Sentry, Datadog,
  Twilio, an LLM provider, an SMTP service
- "Send email" from a managed instance — `forklaunch infra add <service> email`
  (below); no SMTP or SendGrid key
- A deploy is blocked complaining about a missing environment variable

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

## Email (managed instances)

A managed instance never holds a mail credential. The platform sends through
Amazon SES from the instance's own sending identity
(`no-reply@<instance>.<platform sending domain>`), under a per-instance SES
configuration set, inside the product's daily quota and rate, and refuses
addresses on the instance's suppression list (hard bounces and complaints).
## SMS (platform-held): `forklaunch infra add <service> sms`

Texts go through the platform's AWS End User Messaging SMS gateway. The
service holds **no AWS credential, no Twilio key and no phone number**: it
signs each send with its instance key, and the platform sends from the
product's origination pool under a configuration set named for the instance
(`fl-<instanceId>`), so every receipt and reply is attributed to that instance.

### 1. What the CLI writes

```bash
forklaunch infra add <service> email
forklaunch infra remove <service> email     # undoes it
```

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

### 3. Events

Delivered signed to `/platform-events/email`, each with the SES `messageId`:
`email.delivered`, `email.bounced` (`bounceType`, `permanent`) and
`email.complained` (`feedbackType`). Deliveries can repeat; dedupe on
`event.id`. Permanent bounces and complaints are already suppressed by the
platform; mark the address undeliverable in the app too.

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
forklaunch infra add <service> sms        # infra remove <service> sms undoes it
```

- `registrations.ts`: an `SmsClient` built with `createSmsClient()` from the
  gateway settings (`PLATFORM_GATEWAY_URL`, `INSTANCE_ID`, `INSTANCE_HMAC_KEY`,
  all optional; outside managed mode the client refuses on the first send, so
  the service still boots).
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
else is.** A direct Anthropic or OpenAI key, Stripe, Twilio, SendGrid, a direct
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
