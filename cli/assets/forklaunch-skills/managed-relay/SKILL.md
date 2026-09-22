---
name: managed-relay
description: "The managed-apps relay: one callback address per product that the platform routes to the right customer's instance. Relay routes (redirect for browser OAuth such as Epic with PKCE, forward for webhooks), what a product declares, what its app must expose, and how to debug a bounced callback."
user-invokable: true
---

# Managed Apps Relay

> Companion to `/managed-apps` (the operator runbook) and `/managed-app-repo`
> (what a product repository must implement). This skill is the relay alone.

## What this is, plainly

Say you sell "Health Vault" as a managed product. Priya's copy and Dr. Chen's
copy each run on their own address. Both sign in to their hospital through
Epic — but Epic lets you register **one** "come back here after login"
address per app, and Health Vault is one Epic app shared by every customer.
So every customer's login has to come back to one shared address, and
something there has to work out whose login it was and send it on. That
shared address is the **relay**, and the platform runs it:

```
https://relay-<templateId>.<platform zone>/callback
```

The relay does exactly the part only the platform can do: it reads the
`state` the instance minted (`r:<instance host>:<nonce>`), checks that host
really is an eligible instance of *this* product, burns the nonce so the
callback can't be replayed — and then hands the callback to the instance
according to a **route** the product declared. It never learns which provider
is on the other end and never holds a provider secret.

## Routes: the one per-provider thing, and the product declares it

A route is `{ name, component, path, mode }` on the template:

| | `redirect` | `forward` |
|---|---|---|
| for | browser-redirect OAuth (Epic, Google, …) | callbacks with no browser (webhooks) |
| what the relay does | 302 the browser to `https://<prefix>-<component>.<zone><path>?<provider query verbatim>` | POST the callback to the component over the mesh, HMAC-signed with the instance key |
| who finishes the protocol | the instance, with whatever it started (PKCE verifier, its own secret, a JWT assertion) | the instance's handler |
| what the app must expose | `GET <path>` on `<component>` (public is fine) | `POST <path>` on `<component>` with internal/HMAC auth |

The route named **`default`** is served at the bare `/callback`; every other
route at `/callback/<name>`. So a product can have several
(`default` for Epic, `google`, `stripe-webhook`), each with its own URL to
register with its provider, and a redirect URI registered before routes
existed keeps working.

Why `redirect` is the mode for Epic specifically: Epic issues **no client
secret** to a patient app, and a code minted against a PKCE challenge can
only be exchanged by whoever holds the verifier — the instance. A relay that
tried to exchange the code itself could never succeed; it just has to get the
browser back to the instance with Epic's query intact.

## Operator setup, per product, once

```bash
forklaunch managed summary                        # relayConfigs[].callbackUrl → register with the provider
```

Declare the route (CLI `managed template relay set` is pending; the API is the
control plane's front door):

```http
PUT /managed-mode/templates/<slug>/relay-routes
{ "routes": [ { "name": "default", "component": "vault", "path": "/epic/callback", "mode": "redirect" } ] }
```

The response (and `GET /managed-mode/summary`) lists every route with its
`callbackUrl`. Whole-list replace; an empty list clears the routes.

Validation (`normalizeRelayRoutes`): names `^[a-z0-9][a-z0-9-]{0,63}$` and
unique; `component` a DNS label (it becomes a hostname label, so a route can
only point at the instance's own components); `path` root-relative with no
query or fragment (the provider's query is appended at relay time).

## What the platform enforces at publish

Publishing a template version runs the managed-app contract check
(`managed-app-contract.ts`) against the built manifest. For each declared
route it verifies the named component exists in the build and exposes the
path with the right method — `GET` for redirect, `POST` with internal/HMAC
auth for forward. A template that declares a route its app does not serve is
rejected at publish with a message naming the component and path, not by a
customer at first login.

## What the instance must do (redirect mode)

1. **Mint the state** as `r:<its public host>:<nonce>` when starting the flow
   and send the provider to the relay URL as `redirect_uri`. The host is the
   platform-injected `PUBLIC_HOST`; the redirect URI is whatever the operator
   set (health-vault: `EPIC_REDIRECT_URL`). Health Vault's
   `epic-sync.service.ts login()` is the reference.
2. **Serve the route path** on the named component, accepting the provider's
   query (`code`/`state`, or `error`/`error_description`), finishing the
   exchange with the verifier it stored against the state, and sending the
   browser somewhere useful. Health Vault answers JSON to its own SPA's
   fetch and, for a browser navigation (`Accept: text/html`), 302s to
   `FRONTEND_URL` with `?connected=1` or `?epic_error=…`.
3. Nothing else. No secret arrives from the platform in this mode.

## The legacy path (no routes declared)

A template with **no** routes gets the original Epic-specific behaviour: the
relay exchanges the code with a client id/secret stored on the template
(`PUT /instances/relay-config/:slug/credentials`, admin) and HMAC-POSTs the
tokens to `relayTargetComponent`/`relayTargetPath` (default `iam`,
`/relay/session-ingest`, scaffolded by `forklaunch init module -m relay`).
It exists for products whose provider issues a confidential secret; new
products should declare a route. Generic forwarding of an arbitrary provider
payload (`forward` for webhooks) reuses this mesh path today and gets its own
body contract in the next slice.

## How the relay host exists

`relay-<templateId>.<platform zone>` is a Route53 CNAME onto
`managed-apps.<zone>` plus a host-header rule on the platform ALB to the
managed-apps target group (`relay-routing.service.ts`). It is ensured at
template publish, at managed-apps boot, and on a timer, so a rebuilt balancer
or a hand-deleted record heals itself; `POST /app-templates/:slug/relay/ensure`
(admin) forces it. Always the **platform** zone, even for a product with its
own `baseDomain`: the balancer only carries the platform zone's certificate,
and the relay host is a redirect target registered with a provider, not an
address a customer sees.

## Debugging a bounced callback

The relay answers a plain "please restart sign-in" page and logs the reason
under `[Relay]` (`forklaunch observe logs` on the platform side):

| log line | meaning |
|---|---|
| `refused: unparseable state` | the instance did not mint `r:<host>:<nonce>` (Health Vault: `PUBLIC_HOST` unset, or the stored `redirect_uri` already contains the host so it skipped the prefix) |
| `refused: host not allowlisted` | the host is not an instance in `awaiting_claim`/`active`/`suspended` — e.g. a destroyed or `resetting` instance, or a typo'd `PUBLIC_HOST` |
| `refused: state replay` | the same state came back twice (the 10-minute single-use claim in Redis) |
| `refused: no such route` / `product declares no routes` | `/callback/<name>` for a name the template does not declare |
| `handing callback to instance` | success — the next stop is the instance's own path |

Provider-side errors (`error=3`/`error=4` at Epic) never reach the relay:
they mean the registered redirect URI does not equal `callbackUrl` byte for
byte, or has not propagated yet.

## Plain summary

The platform owns one callback address per product. A product tells the
platform where callbacks should go — "hand Epic's to my `vault` component at
`/epic/callback`" — and the relay checks the callback belongs to a real
customer instance, kills replays, and passes it on. The instance finishes
its own protocol with its own keys; the platform holds no provider secret and
knows nothing about Epic. Publish refuses a route the app does not serve.
