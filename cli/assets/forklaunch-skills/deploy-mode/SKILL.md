---
name: deploy-mode
description: "The two decisions a first deploy forces: single-app vs managed-mode template, and where the app's compute runs (platform-shared / org-shared / dedicated). How each is resolved, which is authoritative, and what is not enforced yet."
user-invokable: true
---

# First deploy: the two decisions

## When to Use This Skill

Use this whenever a user wants to **deploy** a ForkLaunch app, especially a
**first deploy**. Read it before running `deploy create`, not after.

A first deploy forces two separate decisions, and they are easy to confuse
because both get called "hosting":

1. **Which pipeline** — is this one app you operate, or a template that launches
   a private copy per customer? Wrong answer, wrong commands entirely.
2. **Which cluster** — where does the compute physically run? Wrong answer costs
   roughly 50× more or less per month, and can rule out compliance frameworks.

Both should already be settled at `forklaunch app create`. This skill covers how
each is resolved when it wasn't.

For the mechanics of the managed path once chosen, defer to `/managed-apps`.

## The two modes (explain this to the user plainly first)

- **Single app** — one running application you operate. `forklaunch deploy create`
  stands up its infrastructure in an environment/region. This is the default for
  most apps: a dashboard, an API, an internal tool.
- **Managed mode** — you publish the app once as a **template**, then launch a
  separate **instance** per customer, each with its own deployment and a one-time
  claim link that hands ownership to that customer. Use this when the product is
  "every customer gets their own private copy" (e.g. a single-tenant health
  vault, a per-clinic portal). Instances are *not* deployed with `deploy create`
  — they go through `forklaunch managed template …` / `managed instance create`.

Concrete example: if you sell a booking tool and all customers share one backend,
that's a **single app**. If each dental practice gets its *own* isolated copy
with its own database and domain, that's **managed mode**.

## Decision 1 — App type is a CONTROL-PLANE fact

**Do not store "managed" in the local `manifest.toml`.** The manifest drifts, gets
hand-edited, and is per-checkout — it is the wrong source of truth for something
the platform owns. Whether an application is single or managed is decided at
**application creation** (`forklaunch app create --managed`) and lives on the
application record.

Resolution order on deploy:

1. **The application record's `managedMode` flag** — the authoritative answer,
   set by `app create --managed`.
2. **Falling back**, an app whose source repository matches a registered template
   (`GET /managed-mode/templates`, comparing `sourceRepo` against the manifest's
   `git_repository`) is managed. This catches templates registered before the
   flag existed.
3. **Neither answers** — ask the user once, record it, and never ask again.

### Ask it like this

> "Does every customer get their own private copy of this — their own database,
> their own web address — or does everyone share one system?"

Concrete example: if you sell a booking tool and all customers share one backend,
that is a **single app**. If each dental practice gets its own isolated copy with
its own database and domain, that is **managed mode**.

### Routing

| Answer | Command path |
| --- | --- |
| Single | `forklaunch deploy create --release … --environment … --region …` |
| Managed | `managed template create → publish → publish-template → managed instance create → instance claim-link` (see `/managed-apps`) |

### ⚠ Not enforced today — check this yourself

`deploy create` is **supposed** to refuse a managed template and redirect to
`managed instance create`. Right now it does not:

- The guard is **not in the shipped CLI** (1.10.0 has no `--force` flag on
  `deploy create`; the check exists only in unreleased CLI source).
- Even in that source it cannot fire, because the template list endpoint does not
  return `sourceRepo`, so the repo comparison never matches.
- `app create --managed` writes `managedMode`, and the deploy path does not read
  it.

**Consequence: nothing stops a managed template from being deployed down the
single-app pipeline.** Until those land, *you* are the guard. Before running
`deploy create` on an app you did not just create, check:

```bash
forklaunch managed template list --include-unpublished
```

If the app you are about to deploy is one of those templates, stop and use
`managed instance create` instead.

## Decision 2 — Which cluster the compute runs on

Separate question, same moment. The first deployment of an application to an
environment × region is **gated**: the platform returns the three placements with
a monthly cost estimate for each, and refuses to proceed until one is chosen.

```
First deploy — choose a cluster
  • Platform shared cluster       ~$2.00/mo
      Packed onto ForkLaunch-managed shared hosts — cheapest to run;
      isolation is per-task, not per-cluster.
  • Organization shared cluster   ~$31.00/mo
      Packed onto your organization's shared hosts.
  • Dedicated cluster             ~$108.00/mo
      Your own cluster, load balancer, and network. Full isolation.
```

| placement | hosts shared with | compliance-capable |
|---|---|---|
| `platform-shared` | **other organizations** | no |
| `org-shared` | your org's other apps | yes — the tenant boundary holds at the org |
| `dedicated` | nothing | yes |

**The disqualifier is sharing hosts with other organizations, not sharing hosts
at all.** An app that declares HIPAA, SOC 2 or PCI-DSS cannot run on
`platform-shared`; `org-shared` is fine, and is the same argument every
dedicated-VPC SaaS makes. GDPR and CCPA are not decided by compute placement.

Placement changes **nothing** about the data-layer controls — per-tenant field
encryption, tenant filters, route access levels, audit logging are identical
everywhere. What varies is whether containers share a host kernel and network
path with another organization's workloads.

### How it resolves

1. `forklaunch app create --cluster-type <…>` — settled at registration; the gate
   does not ask again.
2. `forklaunch deploy create --cluster-type <…>` — settled at deploy.
3. Neither — the gate fires. On a terminal you get a menu; without one the command
   exits telling you which flag to re-run with.

An option can come back **unavailable**, with a reason worth relaying verbatim:
no compute pool enabled in that region, fewer than two applications in the org to
amortize a shared host, or the app's declared compliance frameworks.

### Say the tradeoff out loud

This is the one moment the user is guaranteed to see the cost of the choice.
Do not pick for them, and do not present it as a technical formality:

> "Cheapest puts your app on machines shared with other companies — about $2 a
> month. Your own cluster is about $108. For patient records the shared option
> isn't available, so it's your organization's own hosts at ~$31, or dedicated."

### Local settings do not override it

The control plane owns placement. A component's `[projects.metadata] hostingType`
in `manifest.toml` seeds its **initial** value and is not re-read afterwards —
editing it later does nothing. To change placement after registration, use the
knob (`forklaunch app hosting`), not the file. The knob RECORDS the change; the
next deployment enacts it. Free before a first deploy; afterwards it needs
`--confirm-downtime`, and the next deployment copies the data to the new
substrate and cuts over there instead of deploying directly.

## Plain summary

A first deploy forces two questions, and both belong to the platform rather than
to any file on the user's machine.

**Which pipeline** — one app everyone shares, or a private copy per customer.
That is set by `forklaunch app create --managed` and should be asked once. Be
aware that nothing currently *stops* a managed template from going down the
single-app path, so check `managed template list` yourself before deploying an
app you did not just create.

**Which cluster** — where the compute runs, from cheapest-shared to fully
dedicated, roughly a 50× spread. Set it at `app create --cluster-type` or answer
the gate at first deploy. An app that declares HIPAA, SOC 2 or PCI-DSS cannot use
the cross-tenant option; the platform enforces that and tells you why. Read the
estimates to the user before they choose — it is a real cost decision, not a
technical detail.
