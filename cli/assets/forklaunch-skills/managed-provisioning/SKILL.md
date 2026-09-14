---
name: managed-provisioning
description: "Managed apps: templates, instance provisioning, the claim handover, teardown, and where each surface stops."
user-invokable: true
---

# Managed Provisioning

## What this is, plainly

You built an app. You want to sell it so **each customer gets their own private
copy** — one dental practice's patient records never sharing a database with
another's.

A **template** is the blueprint (your git repo). An **instance** is one
customer's copy. **Claiming** is the handover: the customer sets a passphrase
and the instance becomes theirs.

## The flow, end to end

```
template create ──► template publish (a VERSION) ──► template update --status published
                                                              │
                                          instance create ─────┘
                                                  │  returns the address immediately
                                                  ▼
                               row in `provisioning` + queued job
                                                  ▼
      worker claims it (FOR UPDATE SKIP LOCKED) ──► provisioner:
        creates a backing Application → deploys the pinned version
        → mints smsHmacKey → mints a one-time claim link → `awaiting_claim`
                                                  ▼
                    operator reveals the link ONCE and sends it
                                                  ▼
              customer opens /claim/:token on ForkLaunch, sets a passphrase
                                                  ▼
                   `active` + "your instance is ready" email
```

Teardown runs the same way: `destroying` is enqueued, the worker tears down the
backing application, then `destroyed` (terminal).

## Three traps that have already bitten

**A template is created `draft` and cannot be launched until published.**
`createInstance` requires `PUBLISHED`. Publishing a *version* does not publish
the *template* — they are different operations with confusingly similar names.
If `instance create` 404s with "no published template", this is why.

**Everything is idempotent because a retry costs money.** Each provisioning
step is skipped when its output already exists, and `applicationId` is flushed
*before* the deploy. Without that, a failed deploy followed by a retry creates a
SECOND backing application — infrastructure nobody points at and everybody pays
for. This is mutation-tested; do not "simplify" the guard away.

**The claim link is genuinely one-time.** It is purged on reveal, and its hash
is cleared on claim. Losing it means destroying and relaunching the instance.
Surface it in something the operator must dismiss, never a toast.

## The claim handover, and the one property that matters

The claim link points at **ForkLaunch**, not at the customer's instance:

```
https://<forklaunch>/claim/<token>
```

That is deliberate. Pointing it at the instance would require the template
author to build a claim page *and* the platform to inject the token into the
instance so it could verify one. Pointing it at us deletes both requirements.

**The passphrase must be turned into the backup key IN THE BROWSER**
(`client/lib/backup-key.ts`). ForkLaunch hosting the page is only safe while the
passphrase never leaves the customer's machine and only the derived public half
is posted. A server-side derivation would look identical from the outside and
would silently end the "we can never decrypt your backups" guarantee.

If you touch that file, that is the review gate. Nothing else about the page
would change to signal the property was lost.

Consequence, which belongs in the product and not only in a comment: **a lost
passphrase means unrecoverable backups.** No reset, no override, no support
path.

## Where each surface stops

| | CLI | Control plane | Dashboard |
|---|---|---|---|
| List templates | ✅ | ✅ | ✅ (launch dialog) |
| Create template | ✅ | ✅ | ❌ |
| Publish a version | ✅ | ✅ | ❌ |
| Publish the template | ✅ | ✅ | ❌ |
| List / launch instances | ✅ | ✅ | ✅ |
| Reveal claim link | ✅ | ✅ | ✅ |
| Destroy | ✅ | ✅ | ✅ |
| Claim (customer) | ✅ | ✅ public | ✅ `/claim/:token` |

**Template management is CLI-only.** The control-plane routes exist; the
dashboard has no UI for them. An operator can launch and tear down from the
dashboard but must drop to the CLI to create or publish a template.

## Architecture rules

- **managed-apps is never called directly.** The CLI and dashboard call
  platform-management, which proxies. Adding a capability means adding it in
  three places: the managed-apps handler, the managed-apps SDK export, and the
  `/managed-mode` proxy. A handler that is not in the SDK is unreachable — that
  is exactly how template publishing went missing.
- **Proxies forward the caller's `Authorization` header** rather than acting
  with ambient credentials, so managed-apps still applies its own tenancy
  checks and the control plane cannot reach across organizations.
- **Gating:** reads are `PLATFORM_VIEWER`, launching and destroying are
  `PLATFORM_EDITOR` (they provision billable infrastructure), template
  management is `PLATFORM_ADMIN` (a template defines what the whole org can
  deploy). Claim is **public** — the customer has no account.
- **Ambiguous failures are collapsed on purpose.** Bad token, expired,
  already-claimed and unknown-id all return one identical 404. Distinguishing
  them lets an attacker probe which links are live.

## Fields that look wired but are not

Check before relying on any of these:

- `imageUri` — read by the provisioner, **never written** by anything.
- `stripeProductId` — settable, but **billing never reads it**.
- `baseDomain` — read for host allocation, but **not in any create or update
  schema**, so it is settable only by writing the column directly.
- `Application.managedMode` — written by the dashboard at app creation,
  **never read**.

The schema was designed ahead of the code. Assume a field is inert until you
have found the line that writes it *and* the line that reads it.

## Not yet verified

**None of this has run against a live server.** `PlatformControlPlaneClient`
has never made a real call, and the claim path has never been exercised end to
end. The `age1…` recipient is structurally valid bech32 (checked against
BIP-173 vectors) but has never been handed to a real age implementation —
verify one against the age CLI before anything encrypts a backup to it.
