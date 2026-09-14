---
name: deployment-approvals
description: "Deployment approval gating: why it is opt-in, the resolution order, four-eyes, and the two gates that must agree."
user-invokable: true
---

# Deployment Approvals

## What this is, plainly

Some teams want a second person to sign off before anything reaches production.
This is that: a deploy can be blocked until someone *other than the person
deploying* approves it.

**It is off unless you turn it on.** That is the single most important fact
here, and it is a deliberate reversal of how it used to work.

## Why it is opt-in — the trap it used to be

Gating used to default **on** for any environment named "production". That made
it a trap rather than a control.

Approving requires a second person: `ApprovalFourEyesError` rejects
self-approval. So any organization with **one member** had its production
deploys permanently blocked — by a setting nobody chose, with no way to satisfy
it. The control was indistinguishable from an outage.

Now: an organization that wants the control turns it on per environment. One
that never opts in is never gated. If you are tempted to make this default-on
again for safety, you are re-creating a state where a solo operator cannot ship
at all.

## Resolution order

`DeploymentApprovalService.isApprovalRequired` is the **single decision
function**. Do not add a second one.

1. **Autodeploy bypass.** An application whose effective deploy mode is
   `auto_release_and_deploy` has opted into fully automated deploys — approvals
   are never required, and this overrides everything below. The mode is read
   from persisted config (`application.metadata.cicd`, falling back to the org's
   GitHub-installation `cicdDefaults`), **never from a caller-supplied flag** —
   otherwise the bypass would be self-service for whoever is deploying.
2. **Explicit environment setting.** `environment.requireDeploymentApproval`
   wins when it is `true` or `false`.
3. **Default: not required.**

`null` means "unset", not "false" — it falls through to the default. The toggle
endpoint reports which of the three cases you landed in, which is worth keeping:
"reset to default" and "no longer required" are different states.

## The two gates must never disagree

Approval is checked twice:

- the **dispatch gate** in `DeploymentService.dispatchDeployment`
- the worker's **defense-in-depth** check, `checkDeploymentApprovalInternal`

Both call the same `isApprovalRequired`. That is the point — if they ever
resolved the requirement differently, a deploy could pass the front door and
stall in the worker, or worse, skip the gate entirely. When changing resolution
logic, change it in the one function and verify both callers still route
through it.

## Approval lifecycle

```
pending ──► approved ──► consumed
   │            │
   ├──► rejected│
   └──► expired ◄── lazily, when expiresAt has passed
```

- **`consumedAt`** means the approval was spent on a deployment. An approval is
  not reusable; a second deploy needs a second approval.
- **Expiry is lazy** — a request whose `expiresAt` has passed is marked expired
  when next examined, not by a sweeper. Do not assume `pending` rows are live.
- **Four-eyes is enforced at approve time**, comparing `approvedBy` against
  `requestedBy`. It is not advisory.

## Routes

```
GET    /deployment-approvals          list
POST   /deployment-approvals          request
POST   /deployment-approvals/:id/approve
POST   /deployment-approvals/:id/reject
GET    /internal/deployment-approvals/active   (worker gate)
```

## Turning it on

Per environment, via the environment settings endpoint:

```
requireDeploymentApproval: true    // gated
requireDeploymentApproval: false   // explicitly not gated
requireDeploymentApproval: null    // unset → default (not gated)
```

Before enabling it, confirm the organization has **at least two members who can
approve**. A single-member org that turns this on reproduces exactly the trap
the default-off change was made to fix.
