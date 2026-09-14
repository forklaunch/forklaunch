---
name: managed-relay
description: "The managed-apps relay: a per-product, signed, universal callback acceptor that forwards a verified provider event to the right instance. What a managed app must implement, the one-command install, and the publish check that enforces it. Epic OAuth is the first wired event."
user-invokable: true
---

# Managed Apps Relay

## What this is, plainly

Say you sell "MyHealthVault" as a managed product, and Dr. Chen's practice and
Dr. Osei's practice each run their own copy. Each practice signs in with their
hospital's Epic account. Epic only lets you register **one** "return here after
login" address per app, and MyHealthVault is one Epic app shared by every
practice. So every practice's login has to come back to a single shared address:
the **relay**.

The relay is platform-owned. It holds the product's Epic secret, finishes the
Epic login itself, and then hands the finished session down to the **right**
practice's instance over the internal network. Two things fall out of that:

- **Epic's secret never ships inside an instance.** The relay does the token
  exchange; the instance just receives the result.
- **Your instance never has to be Epic's callback.** It only has to accept the
  handoff the relay sends it.

Your side of the deal is one endpoint (`/relay/session-ingest`) that receives
that handoff, and you do not write it by hand — a CLI module scaffolds it.

## It is a universal callback acceptor, not just an Epic thing

The Epic login above is the first event wired through it, but the relay is
really a **universal, signed callback acceptor**: any event a provider fires
back that has to reach one specific customer's instance can ride the same
mechanism. Think of the relay as one platform-owned front door for a whole class
of "someone outside called back, and it belongs to instance X" problems — an
OAuth callback, a provider webhook (a payment event, a device sync, an EHR push
notification), any signed callback where the outside party only knows one shared
address but the payload belongs to a single instance.

The shape is always the same, which is why one endpoint covers it: the relay
verifies the caller, figures out which instance the event is for, signs the
payload with that instance's key, and forwards it over the mesh; your ingest
endpoint verifies the signature, guards against replay, and dispatches on the
event. Today the wired event is the Epic OAuth session; adding another event
type is a new dispatch branch on the same verified, per-instance channel, not a
new piece of infrastructure. Read the contract below as the general one — Epic
OAuth is the concrete example that exists now.

## When to use this skill

- You are building a managed-app template that logs users in through Epic (or
  another provider fronted by the relay) and need to know what your app must
  implement.
- You are an operator setting up a product's Epic login.
- You are debugging a managed instance where Epic sign-in bounces or 403s.
- Related: [[managed-provisioning]] (templates, instances, the claim handover),
  [[deploy-mode]] (single-app vs managed-mode).

## The flow, end to end

```
browser ── Epic login ──> Epic ── redirect ──> relay-<appId>.<zone>/callback   (ONE per product)
                                                    │  relay holds the Epic client + secret
                                                    │  does the token exchange itself
                                                    ▼  POST /relay/session-ingest  (internal, HMAC, over the mesh)
                                          your instance's iam service
                                                    │  verify HMAC, guard nonce, run your hook
                                                    ▼  returns { redirectPath }
                                          relay 302s the browser to your instance, signed in
```

The relay routes to the correct instance from the OAuth `state` it minted, and
forwards to the **specific component** that hosts the endpoint (your auth
service, `iam` by default), not the instance's front door.

## Installing the endpoint (do not hand-roll it)

The endpoint verifies a signed request and mints a session — security-sensitive
code you should not write by hand. Install it with the CLI module, the same way
you add messaging:

```bash
forklaunch init module -m relay -p <app-path>
```

It injects into your app's existing better-auth `iam` service (it is not a new
service, so there is no `-d/--database` flag). It scaffolds the generic ~80%:

- `POST /relay/session-ingest` on a root-basePath router (so the HMAC-signed
  `req.path` matches exactly), `access: 'internal'` with the per-instance HMAC
  key.
- A single-use **nonce** replay guard (a `RelaySessionHandoff` table + migration
  — no Redis required).
- A one-time handoff ticket plus `GET /relay/handoff` that sets the better-auth
  session cookie and 302s, and a `sanitizeRedirectPath` helper (root-relative
  only; rejects `//`, `/\`, and absolute URLs).
- The `INSTANCE_ID` / `INSTANCE_HMAC_KEY` config and the DI + router wiring.

On a `iam-better-auth` app the session-cookie side is fully pre-wired. On base
iam it scaffolds and warns that session creation stays in your hook.

## The one thing you implement: the hook

The module leaves exactly one app-specific function,
`establishSessionFromRelayTokens(tokens, em)` in
`iam/domain/hooks/relayHooks.ts`. Two responsibilities:

1. **Store the OAuth tokens where your app already looks for them.** The relay
   does the exchange the app used to do at its own callback, so route the tokens
   through your existing token store / sync path. Store them encrypted, under the
   same context your app already uses (see the compliance skill).
2. **Resolve which user to sign in** (for a managed instance, the claimed owner)
   and return a root-relative `redirectPath` (or `/`).

The reference implementation is the Health Vault app (its iam
`relaySession.service.ts` stores tokens through vault's existing Epic path and
resolves the owner from the claim record). Follow that shape.

## The contract (what the relay sends you)

Your endpoint must accept exactly this, because the relay signs it:

- `POST /relay/session-ingest` (`req.path` must equal `/relay/session-ingest` —
  mount at root basePath, not `/relay`, or the HMAC check 403s).
- `authorization` header is `generateHmacAuthHeaders({ secretKey:
  INSTANCE_HMAC_KEY, method: 'POST', path: '/relay/session-ingest', body, keyId:
  INSTANCE_ID })`. Verify with `secretKeys: { [INSTANCE_ID]: INSTANCE_HMAC_KEY }`.
- Body `{ nonce, tokens }` — `tokens` is the Epic token-exchange result. Reject a
  reused `nonce` (single-use).
- Respond `{ redirectPath }` — root-relative only.

## Per-product configuration

- **Forward target** — by default the relay forwards to the `iam` component at
  `/relay/session-ingest`. If your auth service is named differently or the
  endpoint lives elsewhere, set `relayTargetComponent` / `relayTargetPath` on the
  template.
- **Operator setup (per product, once):**
  1. Register **one** Epic app for the product; its redirect is
     `relay-<appId>.<zone>/callback`.
  2. Set its client id/secret on the product via the admin `setRelayCredentials`
     endpoint, so the relay holds the creds. One registration per product, never
     per customer.

## The publish-time check enforces this

When you publish a template version, the platform runs a **deterministic
acceptance check**: if the template uses the relay (detected from its Epic
config), it must expose `POST /relay/session-ingest` with internal/HMAC auth. A
template that opts in but is missing the endpoint is **rejected at publish** with
a message naming what is missing — so "did I wire it right" is answered before a
customer ever hits a broken login, not after.

## Platform vs you, in one line

The **platform** owns the relay, the Epic secret, the token exchange, the DNS
(`relay-<appId>.<zone>`), and the mesh forward. **You** implement the
`establishSessionFromRelayTokens` hook (install the rest with the module), and as
operator you register the Epic app and set its creds.

## Plain summary

Epic allows one callback per app, so every customer's login funnels through a
platform-owned relay that holds the secret, does the exchange, and hands the
session to the right instance. Install the receiving endpoint with `forklaunch
init module -m relay`, fill the one hook that stores the tokens and signs the
user in, register the product's Epic app and set its creds, and the publish
check makes sure you did. Nothing about Epic's secret ever lives in an instance.
