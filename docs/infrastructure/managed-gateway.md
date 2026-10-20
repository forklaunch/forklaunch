---
title: "Managed Gateway"
description: "Email, SMS, WhatsApp, voice calls, Stripe Connect payments and AI models through the ForkLaunch platform, with no vendor keys in the app."
category: "Infrastructure"
---

## Overview

The managed gateway lets a ForkLaunch app send email, texts and WhatsApp messages, place phone calls, take payments and call AI models without holding a vendor account or key. The platform holds the accounts (Amazon SES, AWS End User Messaging, Amazon Connect, Stripe Connect, Azure AI Foundry). Your service signs each request with a key the platform gives it, and the platform applies limits, compliance rules and logging before it calls the vendor.

It works for **any app deployed on ForkLaunch**:

- an app deployed to its own environment with `forklaunch deploy create`, and
- a managed instance (one copy of a product per customer).

Each capability is added to a service (or a worker) with one command and removed with another:

```bash
forklaunch infra add <service> <capability>      # email | sms | whatsapp | voice | payments | models
forklaunch infra remove <service> <capability>
```

| Capability | Client from `@forklaunch/core/http` | Token in `registrations.ts` | Vendor behind it | Events |
| :--------- | :---------------------------------- | :-------------------------- | :--------------- | :----- |
| `email` | `createEmailClient()` | `EmailClient` | Amazon SES | yes |
| `sms` | `createSmsClient()` | `SmsClient` | AWS End User Messaging SMS | yes |
| `whatsapp` | `createWhatsAppClient()` | `WhatsAppClient` | AWS End User Messaging Social | yes |
| `voice` | `createVoiceClient()` | `VoiceClient` | Amazon Connect | yes |
| `payments` | `createStripeClient({ Stripe })`, `createPaymentsClient()` | `StripeClient` | Stripe Connect | yes |
| `models` | `createModelGatewayClient()` | `ModelGatewayClient` | Azure AI Foundry | no |

See the [infra command reference](/docs/cli/infra.md) for the flags.

## What `infra add` Writes

For every managed gateway capability:

- **`registrations.ts`**: the gateway settings `PLATFORM_GATEWAY_URL`, `INSTANCE_ID` and `INSTANCE_HMAC_KEY` (all `optional(string)`), and the client registration, built from them. A service started without the settings (a unit test, a deploy that does not bind the capability) still boots and fails only where the capability is used: the email, voice and models clients are `Lifetime.Scoped`, so they are only built when resolved, and the SMS client defers its refusal to the first send.
- **`docker-compose.yaml`**: a `gateway-mock` service (see [Local Development](#local-development)), and the gateway settings on the service's container. For a worker, both its worker and server containers get them.
- **`.env.local`**: the same settings, for running the service outside Docker.
- **`.forklaunch/manifest.toml`**: the capability in the project's `resources.capabilities`. The release manifest turns it into a resource of that type (`payment` for payments) bound to the component.
- **For a capability with events**: `api/platformEvents/<capability>.ts`, a handler stub that dedupes on `event.id`, plus the `/platform-events/:feature` route (verified with `verifyPlatformEvent`) mounted in `server.ts` if the service had none.

`infra remove` undoes exactly those edits. It leaves code it did not write: for example a `ModelGatewayClient` you built by hand is kept, with a note.

## Using the Clients

Resolve the registered client from the container. Each call is signed for you.

### Email

```ts
const email = ci.resolve(tokens.EmailClient);
const { messageId } = await email.send({
  to: user.email.deanon,
  subject: 'Your results are ready',        // never protected data
  text: `Hi ${user.firstName.deanon}, your results are in the portal.`,
  tags: { kind: 'results' }                 // echoed on the events
});
```

### SMS

```ts
const sms = ci.resolve(tokens.SmsClient);
await sms.send({
  to: '+14155550123',                       // E.164 only
  body: 'Your appointment is confirmed. Sign in for details: https://...',
  purpose: 'transactional'
});
```

Never text protected data: send a neutral notice or a sign-in link.

### WhatsApp

```ts
const whatsapp = ci.resolve(tokens.WhatsAppClient);
await whatsapp.sendTemplate({
  to: '+14155550123',
  template: 'appointment_reminder',
  language: 'en_US',
  components: [{ type: 'body', parameters: [{ type: 'text', text: 'Tuesday 3pm' }] }]
});
await whatsapp.sendText({ to: '+14155550123', body: 'Which day works?' }); // only within 24h of their last message
```

### Voice

```ts
const voice = ci.resolve(tokens.VoiceClient);
const { callId } = await voice.startOutboundCall({
  to: '+15551230000',
  flow: 'appointment_reminder',             // a flow name from the catalog, never an ARN
  attributes: { appointmentId: appt.id }    // ids only, no protected data
});
```

### Payments

```ts
import Stripe from 'stripe';
import { createPaymentsClient } from '@forklaunch/core/http';

// Onboarding: a Stripe-hosted page for the business that gets paid.
const payments = createPaymentsClient();
const { url } = await payments.onboardingLink({
  returnUrl: 'https://app.example.com/settings/payments?done=1',
  refreshUrl: 'https://app.example.com/settings/payments'
});

// The real Stripe SDK, pinned by the platform to your own connected account.
const stripe = ci.resolve(tokens.StripeClient);
await stripe.checkout.sessions.create({
  mode: 'payment',
  line_items: [{ price: priceId, quantity: 1 }],
  success_url: 'https://app.example.com/paid',
  metadata: { appointmentId: appointment.id } // an opaque id, never patient data
});
```

A service that already registers a Stripe client (billing-stripe, ecommerce-stripe) has its factory switched to `createStripeClient({ Stripe, apiKey: STRIPE_API_KEY })`, with the key made optional. It is used only when the gateway settings are absent.

### Models

```ts
const models = ci.resolve(tokens.ModelGatewayClient);

const reply = await models.chat.completions.create({
  model: 'terra',                           // a catalog alias, not a deployment name
  messages: [{ role: 'user', content: 'Summarize this intake form: ...' }]
});

for await (const chunk of models.chat.completions.stream({ model: 'luna', messages })) {
  process.stdout.write(chunk.choices[0]?.delta?.content ?? '');
}

const { models: allowed, tokensUsedThisMonth, monthlyTokenBudget } = await models.models();
```

The request and response follow the OpenAI chat completions shape. `models()` lists the models you may call and where the monthly budget stands.

## Send from Your Own Domain (Pro)

On the Pro and Enterprise plans, managed-gateway email can go out as `"Display Name" <local@your-domain>` instead of the platform's default sender. Your code does not change: `email.send(...)` is the same call, and the platform picks the from address.

| | regular deployment (per application × environment) | managed instance |
|---|---|---|
| default sender | `no-reply@<app label>.<platform sending domain>` | `no-reply@<host prefix>.<platform sending domain>` |
| domain | the application's custom domain (`forklaunch app domain set` first), or one to three labels in front of it (`--subdomain`) | any domain under a public suffix that your organization controls, named on the command |
| set | `forklaunch app domain email set -e <env> [--from <local-part>] [--name "<display name>"] [--subdomain <sub>]` | `forklaunch managed instance email set --id <id> --domain <domain> [--from <local-part>] [--name "<display name>"]` |
| check | `forklaunch app domain email status [-e <env>] [--json]` | `forklaunch managed instance email status --id <id> [--json]` |
| undo | `forklaunch app domain email remove -e <env> [-y]` | `forklaunch managed instance email remove --id <id> [-y]` |
| who | editors set and remove | editors set and remove, viewers read |

How it behaves:

- **Sends never fail because of it.** Until Amazon SES verifies the domain, whenever SES refuses it, and while the plan lacks custom domains, mail goes from the default sender. A custom domain never makes a send answer 503. An instance's `status` shows `in use: yes/no` (`active`) for which address is used right now.
- **DNS.** The answer lists three DKIM `CNAME` records (required) and an advisory `_dmarc` `TXT` (`v=DMARC1; p=none;`, `required: false`). When the domain is a delegated custom domain of the same organization, the platform writes the CNAMEs itself (`dnsRecordsWritten: true`); otherwise publish them at your DNS host. SES gives up after 72 hours (`status: failed`): fix the records and run `set` again.
- **An environment not yet deployed with email**: `set` saves the sender as `pending_verification`, with an `errorMessage` that says so and no DNS records. The environment's next deploy sets it up, and `status` then lists the records.
- **Rules checked before the request**: `--from` is lowercase `a-z 0-9 . _ + -`, not starting or ending with a dot, at most 64 characters (default `no-reply`); `--name` is one line, at most 64 characters, without `<`, `>` or `"` (other alphabets are fine); `--subdomain` is one to three DNS labels; `--domain` is dot-separated DNS labels.
- **Plan changes**: without the plan, `set` and `remove` answer `403 Custom email senders require a Pro or Enterprise plan`. Reading is not gated: a sender whose plan lapsed shows `status: failed` with that message, and mail goes back to the default sender. The setting is kept, so upgrading again restores it.
- **Other errors**: 400 with a validation message (the application has no custom domain; for an instance, a domain not under a public suffix or the platform's own sending domain); 409 (instances) a domain another organization sends from or holds as a custom domain; 502/503 the mail provider or the ownership check is unavailable, try again.
- **Reading and removing**: an app's `status` reads an array, empty when nothing is set or the application has no custom domain. `remove` on an environment with no sender says so (the platform answers 404). Removing the sender, deleting the custom domain or the environment, or destroying the instance returns mail to the default sender.

See the [domain command](/docs/cli/domain.md) and the ForkLaunch platform docs (Managed gateway page, *Send from your own domain*).

## Local Development

There is no platform on your machine, so `infra add` puts the gateway mock in `docker-compose.yaml`. It ships with `@forklaunch/core` (`forklaunch-gateway-mock`), serves every capability's routes, checks your signatures the way the platform does, and sends signed events back to your service.

| Setting | In `docker-compose.yaml` | In `.env.local` |
| :------ | :----------------------- | :-------------- |
| `PLATFORM_GATEWAY_URL` | `http://gateway-mock:8080` | `http://localhost:18088` |
| `INSTANCE_ID` | `local-instance` | `local-instance` |
| `INSTANCE_HMAC_KEY` | `local-dev-instance-key` | `local-dev-instance-key` |

The mock listens on port 8080 in the Compose network and on **port 18088** on your machine. Its `MOCK_INSTANCE_HMAC_KEY` must equal `INSTANCE_HMAC_KEY`; `infra add` sets both. You can also run it by hand:

```bash
PORT=18088 MOCK_INSTANCE_HMAC_KEY=local-dev-instance-key npx -p @forklaunch/core forklaunch-gateway-mock
```

Useful mock controls (environment variables on the `gateway-mock` service, and endpoints on `http://localhost:18088`):

| Capability | Controls |
| :--------- | :------- |
| email | `GET /__mock/email/messages`; `bounce@...` bounces, `complaint@...` complains; `MOCK_EMAIL_DAILY_QUOTA` (default 200) |
| sms | `GET /__mock/sms/messages`; `POST /__mock/sms/inbound {"from","body"}`; numbers ending `0000` fail; `MOCK_SMS_MONTHLY_CAP`, `MOCK_SMS_PER_MINUTE` |
| whatsapp | `POST /__mock/whatsapp/inbound {"from","text"}`; `MOCK_WHATSAPP_TEMPLATES`, `MOCK_WHATSAPP_HIPAA=1` (403), `MOCK_WHATSAPP_UNCONFIGURED=1` (409), `MOCK_WHATSAPP_RPM` |
| voice | `MOCK_VOICE_FLOWS`, `MOCK_VOICE_CALL_MS`, `MOCK_VOICE_RECORD=1`, `MOCK_VOICE_MAX_CONCURRENT`; numbers ending `9999` are busy; `MOCK_VOICE_NO_NUMBER=1` (409) |
| payments | `POST /__mock/payments/complete/<session id>`; `MOCK_PAYMENTS_AUTO_ENABLE=0` with `POST /__mock/payments/enable`; `MOCK_STRIPE_UPSTREAM` to forward to `stripe/stripe-mock` |
| models | answers `[mock <model>] <your last message>`; `MOCK_MODELS` (default `sol,terra,luna`), `MOCK_MONTHLY_TOKENS` (429 past it), `MOCK_FAIL_EVERY=N` (503 on every Nth call) |

`GET /__mock/requests?feature=<capability>` and `GET /__mock/events?feature=<capability>` show what the mock received and sent.

## Deployed Apps

You do not set the gateway settings yourself. When you deploy:

- **An app deployed with `forklaunch deploy create`**: the first deploy that declares any managed gateway capability creates a gateway identity for that application and environment. Its id is `INSTANCE_ID` and its key is `INSTANCE_HMAC_KEY`, and they stay the same on every redeploy. The deploy sets `PLATFORM_GATEWAY_URL`, `INSTANCE_ID` and `INSTANCE_HMAC_KEY` **only on the services and workers the release binds a capability to**. Other components never get them. Each environment has its own limits, SMS opt-out list, token budget, Stripe connected account, phone number and WhatsApp link, shared with no other environment or instance.
- **A managed instance**: its provisioner sets the same three variables, and the product (template) holds the settings.

After `forklaunch infra remove`, the next deploy removes the settings from that component again.

## Errors and Limits

Every client throws a typed error with the status the gateway answered and, on a 429, `retryAfterSeconds`: `EmailRequestError`, `SmsRequestError`, `WhatsAppRequestError`, `VoiceRequestError`, `ModelGatewayRequestError` (all extend `InstanceGatewayRequestError`). Payments use the Stripe SDK's own errors (`StripePermissionError`, ...).

| Status | Meaning |
| :----- | :------ |
| thrown before any call | `PLATFORM_GATEWAY_URL`, `INSTANCE_ID` or `INSTANCE_HMAC_KEY` is missing: the component has not declared the capability, or the mock is not running locally |
| 400 | bad input (an address, a number that is not E.164, an unknown template or flow, a rejected prompt) |
| 401 | wrong key or signature (locally: `INSTANCE_HMAC_KEY` differs from the mock's) |
| 403 | the release for this environment does not declare the capability (`... add it with forklaunch infra add <component> <capability> and deploy`); a model not on the allowlist; a HIPAA app sending WhatsApp or asking for a model without BAA coverage; voice with no flows |
| 409 | something still to set up: no WhatsApp number linked, no voice number yet, payments before onboarding |
| 422 | a recipient who opted out or is suppressed; WhatsApp free text outside the 24-hour window |
| 429 | a rate limit, quota or monthly budget; wait `retryAfterSeconds` |
| 502 / 503 | the vendor did not answer, or the capability is not configured on the platform |

The platform sets the default limits for deployed apps (per-minute rates, monthly SMS segments and voice minutes, the daily email quota, the model allowlist and monthly token budget). They start small and an operator can raise them for one environment. The numbers are in the platform documentation: the `/integrations` skill, section *Managed gateway capabilities, on any app* (`forklaunch context` installs it in your project). A managed instance takes its limits from its product's settings instead.

## Events Need an HTTP Service

Email, SMS, WhatsApp, voice and payments send events (delivery receipts, replies, opt-outs, call and Stripe events). The platform delivers them signed, over HTTP, to the HTTP service that declared the capability (for a deployed app, `https://<service>.<app DNS subzone>/platform-events/<capability>`), and they are handled in `api/platformEvents/<capability>.ts`. An app with no public DNS has its events recorded but not relayed. Deliveries can repeat, so dedupe on `event.id`.

A worker can hold any of these capabilities and call the gateway, but the worker process does not receive events. If only a worker holds a capability with events, `infra add` prints a note. If the app needs the events, add the capability to an HTTP service as well:

```bash
forklaunch infra add reminders-worker sms     # sends
forklaunch infra add api sms                  # receives sms events
```

`models` has no events.

## Moving Off a Direct OpenAI or Anthropic Client

If a service calls OpenAI or Anthropic with its own key, `forklaunch score` reports `ai-provider-direct` when its entities hold health data (`phi`): the data may be reaching a vendor with no BAA. The fix is the `models` capability:

1. Add it. Your existing client, its key and its dependency are left as they are.

   ```bash
   forklaunch infra add <service> models
   ```

2. Move each call to the gateway client. The request shape is OpenAI's chat completions; `model` is a catalog alias.

   ```ts
   // Before
   const openai = ci.resolve(tokens.OpenAIClient);
   const reply = await openai.chat.completions.create({ model: 'gpt-4o-mini', messages });

   // After
   const models = ci.resolve(tokens.ModelGatewayClient);
   const reply = await models.chat.completions.create({ model: 'terra', messages });
   ```

   An Anthropic `messages.create({ system, messages, max_tokens })` call becomes `chat.completions.create` with the system prompt as the first message (`{ role: 'system', content: system }`) and the text read from `reply.choices[0].message.content`. Call `models.models()` to see which aliases you may use (locally, the mock's `MOCK_MODELS`).

3. When nothing uses it any more, delete the old client registration, `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`, the `openai` / `@anthropic-ai/sdk` dependency, and the key in each environment (`forklaunch config`).
4. Run `forklaunch score --offline`. `ai-provider-direct` should be gone, and `capability-wiring` checks that the manifest and `registrations.ts` agree.

For a HIPAA app (the application's or environment's compliance standards include HIPAA), the gateway offers only BAA-covered models; `models()` lists any withheld in `withheldForHipaa`.

## Checks

`forklaunch score` reads the code for each capability:

| Check | What it catches |
| :---- | :-------------- |
| `capability-wiring` | the manifest and `registrations.ts` disagree (rerun `infra add` / `infra remove`) |
| `ai-provider-direct` | a service with `phi` entities calls an AI provider directly |
| `email-provider-direct-in-managed`, `sms-provider-direct-in-managed`, `whatsapp-provider-direct-in-managed`, `voice-provider-direct-in-managed`, `payments-stripe-keys-in-managed` | a managed service uses a vendor SDK or key directly |
| `email-protected-data`, `sms-protected-data`, `whatsapp-protected-data`, `voice-protected-data`, `payments-protected-data` | a `.deanon` value reaches a subject, message body, call attributes or Stripe metadata |

## Related

- [Infra command](/docs/cli/infra.md)
- [Domain command](/docs/cli/domain.md)
- [Deploy command](/docs/cli/deploy.md)
- [Infrastructure overview](/docs/infrastructure/overview.md)
