---
name: managed-app-repo
description: "Prepare an application repository to run as a ForkLaunch managed app: the trust boundary between platform and instance, which secret signs what, the hosted-gateway variable contract, and the handover ceremony — with the mistakes that make an instance impossible to open."
user-invokable: true
---

# Preparing a repo to run as a managed app

> Companion to `/managed-apps` (the operator runbook: publish a template, launch
> an instance, claim it) and `/managed-provisioning` (where each platform surface
> stops). This skill is for the **product repository**: what the application
> itself must be built to do so a stranger can be handed their own copy and
> actually open it.

## The one-paragraph version

A managed app is one product that runs as many private copies — one per customer.
Each copy is created by the platform, handed to its owner through a one-time
link, and from then on is theirs. Two things make this hard, and both are about
trust: the copy has to prove who it is when it calls home, and the person has to
prove who *they* are before the copy will open. Every failure in this document is
one of those two proofs done in a place the attacker can reach, or not done at
all.

## Rule 1: a signing secret never reaches a browser

**A shared signing secret (HMAC) authenticates a machine, not a person.** It says
"this request came from something holding the key". That is only meaningful while
exactly the trusted parties hold it.

Anything shipped to a browser, a mobile bundle, or a downloadable client is
public. Minified, obfuscated, in an environment variable your bundler inlined —
still public. A key in a client turns "requests from this instance" into
"requests from anyone who opened the app once", and the platform has no way to
tell the difference, because the signature is valid.

**So: signing secrets live only in a server you control.** For a web product that
means the instance's own backend services. This is not a burden — assume every
non-desktop product has a backing server, because it does: something answers the
API calls. Sign there.

```
browser / mobile app        ← NEVER holds a signing secret
        │  session cookie or short-lived user token
        ▼
your instance's server      ← holds INSTANCE_HMAC_KEY, HMAC_SECRET_KEY
        │  HMAC-signed request
        ▼
the platform gateway
```

If a client genuinely must call something directly, it gets a **per-user,
narrowly-scoped, revocable, short-lived credential** — never the instance's
signing key. An API key is acceptable *when it is scoped to one user and one
capability and can be revoked without redeploying everyone*. "One key for the
whole instance, embedded in the page" is the thing this rule exists to forbid.

**Desktop and CLI are the exception worth naming.** With no server, there is no
protected place, so there is no instance secret — each user authenticates as
themselves and the app holds only their own credential.

## Rule 2: know which secret signs which direction

A managed instance has two different shared secrets, and confusing them is the
most common wiring bug.

| Secret | Direction it authenticates | Who else holds it |
|---|---|---|
| `INSTANCE_HMAC_KEY` | **instance → platform** ("I am instance X") | the platform |
| `HMAC_SECRET_KEY` | **service → service inside one instance** | only this instance |

`INSTANCE_HMAC_KEY` is the channel to the platform: sending a text message
through the hosted gateway, reporting status, receiving a platform-initiated
call. `HMAC_SECRET_KEY` is the app's own internal plumbing.

**Where this bites:** if the platform needs to call *into* your instance — to
mint a setup link at handover, say — that endpoint must accept
`INSTANCE_HMAC_KEY`. Requiring `HMAC_SECRET_KEY` locks the platform out, because
the platform does not hold your app's internal secret and should not. An endpoint
the platform is expected to call, guarded by a secret the platform cannot have,
looks configured and can never work.

Accept both where it makes sense — `INSTANCE_HMAC_KEY || HMAC_SECRET_KEY` — so
the endpoint works both hosted and self-hosted.

## Rule 3: select your providers from the environment, and say which you chose

A managed instance must not carry credentials for shared infrastructure. Text
messaging is the standard case: the platform runs the account, and the instance
asks the platform to send.

Structure provider selection as an explicit ladder:

```ts
//! Provider selection by environment:
//!   PLATFORM_GATEWAY_URL + INSTANCE_ID + INSTANCE_HMAC_KEY -> platform gateway (hosted)
//!   TWILIO_ACCOUNT_SID + TWILIO_AUTH_TOKEN + TWILIO_FROM_NUMBER -> direct (self-host)
//!   otherwise -> log-only dev provider
```

Three requirements on that ladder:

1. **Hosted mode is chosen by presence, not by a flag.** The platform supplies
   the three values; nothing has to be toggled.
2. **The dev fallback must announce itself at startup**, loudly, once. A silent
   fallback is how a product ships to a customer with codes going to a log file
   instead of a phone.
3. **A half-configured branch must refuse rather than degrade.** Credentials
   present but malformed should log what is wrong and fall back — not build a
   provider that looks live and fails at send time, where nobody is watching.

> Real failure this is drawn from: a gateway held an API key identifier in the
> field meant for the account identifier. Every send returned
> `401 … "no requested permission"` — an error that reads like a revoked token
> and says nothing about the identifier being the wrong kind. It looked
> configured for weeks.

## Rule 4: the handover has two halves — design both

Getting an instance to a customer is two ceremonies, and products routinely build
one and forget the other.

1. **The platform claim.** ForkLaunch hands over ownership of the *instance*: the
   customer opens a one-time link and sets a backup passphrase. You get this free.
2. **Your app's own first-run.** Creating the first account inside the product.
   This is yours to build, and the platform cannot do it for you.

If your product ships **sealed** — no accounts until a setup ceremony runs — then
your repo owes three things, and missing any one of them makes the instance
impossible to open:

- **A page**, not just endpoints. An API-only ceremony means a customer holding a
  working instance with no way to run it. It must live somewhere the customer can
  reach: your client, or a page the platform serves.
- **A sign-in method that matches the owner you create.** If the ceremony creates
  an owner with no usable password — because they are meant to use a passkey or a
  one-time code — then the client must implement a passkey or one-time code
  sign-in. Shipping only email-and-password creates an owner who can never log in.
- **A way for the setup link to be minted and delivered.** See Rule 2: let the
  platform mint it with `INSTANCE_HMAC_KEY` so the handover can be one continuous
  flow.

> Real failure: a product sealed its vault until a phone-verification ceremony,
> created the owner deliberately without a password, and shipped a client that
> only implemented email-and-password sign-in. The backend advertised both
> intended methods. The only user the product would ever have could not log in,
> and no screen existed for the ceremony at all.

## Rule 5: access levels are part of the design

Every route declares who may reach it. For the handover specifically:

| Route | Level | Why |
|---|---|---|
| setup status ("am I claimed?") | `public` | the page needs it before anyone exists |
| verify contact / complete setup | `public` | the customer has no account yet — the one-time token *is* the credential |
| mint a setup token | `internal` | platform-initiated; sign with `INSTANCE_HMAC_KEY` |
| everything else | `authenticated` / `protected` | |

Two properties the public ones must have, since they are reachable by anyone:
the one-time token is compared in constant time, and attempts are rate-limited
per instance. A ceremony endpoint that leaks whether a token exists, or accepts
unlimited guesses, is a public account-takeover.

## Rule 6: what the platform supplies, and what you must not assume

The platform injects these into every hosted instance. Read them; never declare
them as template constants — two of them differ per instance and one is a secret
the platform mints.

| Variable | Meaning |
|---|---|
| `INSTANCE_ID` | which instance this is; the identity the platform verifies |
| `INSTANCE_HMAC_KEY` | the shared key for calls to the platform |
| `PLATFORM_GATEWAY_URL` | where to reach hosted services (SMS, LLM) |
| `PLATFORM_CALLBACK_URL` | where to report lifecycle events — POST `/claim/claimed` (signed with `INSTANCE_HMAC_KEY`) when your own first-run ceremony completes, so the platform records it and emails the owner |
| `PUBLIC_HOST` | this instance's public front-door host; put it in any OAuth `state` you mint (`r:<host>:<nonce>`) so the product relay can route the callback back to you |
| `HMAC_SECRET_KEY` | this instance's internal service-to-service secret |

Declaring `INSTANCE_ID` as a static template value hands every instance the same
identity. The platform's value takes precedence for exactly this reason, but do
not rely on that — leave them undeclared and read them.

## Rule 7: a provider callback comes through the relay, and you finish it

A provider (Epic, Google, Stripe) is registered once per product with one
callback address, the platform's relay. You declare a **relay route** on the
template — "hand Epic's callback to my `vault` component at `/epic/callback`" —
and your app owes two things:

- when starting the flow, mint `state = r:${PUBLIC_HOST}:<nonce>` and use the
  relay URL (from `forklaunch managed summary`) as the provider's redirect URI;
- serve the route path on that component and finish the protocol yourself
  with what you stored against the state (the PKCE verifier, your own
  secret). For a browser arriving there, send it on to your UI; for your
  SPA's fetch, answer JSON. Publish rejects a route your build does not serve.

The platform never holds your provider's secret. Full contract and the
debugging table: `/managed-relay`.

## Checklist before publishing a template

Answer each with a yes you can demonstrate.

**Secrets**
- [ ] No signing secret appears in any client bundle. Grep the built assets for it.
- [ ] Any credential a client does hold is per-user, scoped, and revocable.
- [ ] Nothing is committed; every secret arrives through the environment.

**Platform channel**
- [ ] Every platform-callable endpoint accepts `INSTANCE_HMAC_KEY`.
- [ ] Shared infrastructure (messaging, mail) goes through the gateway when
      `PLATFORM_GATEWAY_URL` is set, and the fallback announces itself.
- [ ] A half-configured provider refuses loudly instead of failing at send time.

**Handover**
- [ ] The first-run ceremony has a page a customer can reach, not only an API.
- [ ] When it completes, the app POSTs `PLATFORM_CALLBACK_URL/claim/claimed`.
- [ ] The sign-in the client implements matches the owner the ceremony creates.
- [ ] Public ceremony routes are constant-time and rate-limited.
- [ ] Someone who is not you can complete the whole path from the claim link.

**Provider sign-in** (if any)
- [ ] The OAuth `state` carries `PUBLIC_HOST`; the redirect URI is the relay's.
- [ ] A relay route is declared and the component serves its path.

**Prove it**
- [ ] Launch an instance, claim it, and open it — as a stranger would, without
      reading a log or running a command. Anything that requires an operator is
      a gap, not a workaround.

## The test that finds everything

Launch a real instance and hand the link to someone who has not read this repo.
Every gap in this document was found that way, and none of them by reading code.
