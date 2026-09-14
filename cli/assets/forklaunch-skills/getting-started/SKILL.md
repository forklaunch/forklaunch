---
name: getting-started
description: "Driver skill: fresh Claude Code or Codex setup to a running ForkLaunch app. Plan by conversation, score the plan on the five rails and close gaps by Q&A, scaffold, then check after every pass. Routes to every other skill."
user-invokable: true
---

# Getting Started — the driver

## When to Use This Skill

- A fresh machine, or a fresh Claude Code / Codex session, and no app yet
- "Build me an app that does X"
- You are not sure which skill applies — start here and route from the table below
- You want the planning conversation the studio surface gives, in a terminal

This skill is the **driver**. It owns the loop; the others own the details.

## Before anything: is the environment ready?

Do not start planning on a machine that cannot build. Check in this order and
stop at the first failure:

```bash
node --version        # 22+
forklaunch --version  # the CLI
docker ps             # a container runtime that is actually running
git config user.email # git identity set
```

If any of those is missing, go to **`/prereqs`**. That skill installs them for
you — detection first, one confirmation, then per-platform installs (OrbStack
rather than Docker Desktop on a Mac), and it names the failure modes that look
like broken installs but aren't. **`SETUP.md`** in this pack is the same ground
written for the user to follow by hand; send them there instead if they would
rather do it themselves. Either way, do not improvise install instructions.

Two of these are needed earlier than people expect: `forklaunch init` and
`forklaunch release create` both shell out to `pnpm`, so Node is required to
*build and ship*, not just to run locally. The container runtime is the one
thing you can skip — the platform builds images itself, so it is only needed to
run the app on this machine.

## The loop

```
  0. READY     /prereqs — a machine that can build at all
  1. PLAN      conversation → agreed plan → SCORE it, close gaps by Q&A
  2. SCAFFOLD  forklaunch init application / init service
  3. PASS      generate or edit code
  4. CHECK     forklaunch score --offline   ← after EVERY pass
  5. repeat 3–4 until the plan is delivered
  6. MILESTONE full analyze, compared against the plan's score
  7. REGISTER  forklaunch app create — placement, compliance, managed: once
  8. SHIP      release create → deploy create → first-deploy cluster gate
```

Steps 3 and 4 are one unit. Never run a pass without the check after it.

Steps 7 and 8 are where the session starts costing money. They are covered
below, and in `/deploy-mode` and `/cli` — do not stop at step 6 and leave the
user holding a scored local app with no idea what comes next.

### 1. Plan — the studio planning surface, as a conversation

The studio surface plans by **asking before it builds**. Reproduce that here.
No frontend, no preview pane — the questions and the written plan are the whole
surface.

**Ask before you plan.** One question at a time, plain language, and never more
than the user can answer without opening a file. The studio planner needs these
answered before it can produce operations, so ask them in this order:

1. What does the app do, in one sentence? Who uses it?
2. What are the main *things* it stores? (customers, bookings, invoices…)
3. Who logs in, and are there different kinds of user?
4. Does it take payments?
5. Does it hold anything sensitive — health, financial, or personal data?
6. Does anything need to happen on a schedule, or in the background?

Q5 is not optional and it is not a formality. It decides field classification,
which decides encryption, which is the single most expensive thing to retrofit.
Ask it plainly: *"Will this store anything you'd be uncomfortable seeing in a
data breach — medical details, card numbers, home addresses?"*

**Then write the plan back** and get explicit agreement before scaffolding:

- the modules to create, and why each one exists
- the entities in each, with their fields
- which fields are sensitive, and at what level
- what is deliberately **not** in scope for this pass

A user who says "yes, that's right" to a written plan has given you far more
than one who said "build me a booking app".

**Then score the plan, before writing any code.** This is the part the studio
surface does that a plain planning conversation skips, and it is the point of
planning at all: a readiness problem is nearly free to fix in a plan and
expensive to fix in a codebase.

Score the agreed plan across the same five rails the finished app is judged on,
against the same rubric:

| rail | what it covers |
|---|---|
| Compliance | data classification (PII/PHI/PCI), audit logging, encryption, retention, consent |
| Security | authn/authz/RBAC, tenant isolation, input validation, secrets, least privilege |
| Governance | build and dependency integrity, best practices enforced by construction |
| Scalability | statelessness, pagination, indexing, N+1/caching, queues, background work |
| Observability | logs/metrics/traces, health checks, error tracking, alerting |

You are scoring a **projection**, not code — `phase: "plan"`. Give each rail a
0–100, a one-line summary, and a short checklist of what the plan does and does
not yet account for. Say plainly which rails the plan is weakest on.

**Then use the questions to close the gaps — this is the loop.** For each rail
scoring poorly, ask the user what would raise it. One question at a time, in
plain language, each tied to the rail it affects and what it is worth:

> Compliance is at 45. The plan stores patient names and appointment notes, but
> nothing says how long you keep them. **How long should records be kept after a
> patient leaves?** Answering this is worth about 15 points — it decides the
> retention policy, which is far easier to set now than to backfill.

Then re-score with the answer folded in and show the movement. Repeat until the
score stops moving or the user says it is good enough.

Two rules make this loop work rather than annoy:

- **A decision already made is final.** Never re-ask something the user has
  answered. Carry answers forward verbatim into each re-score.
- **An open question stays worded the same.** Re-asking a question in new words
  reads as though you forgot, and invites a contradictory answer.

Record the final scored plan — the plan text, the rail scores, and the
decisions with their answers. It is the baseline the per-pass checks in step 4
are measured against, and it is what tells you whether the build is tracking
the plan or drifting from it.

For a heavier review — scope challenge, architecture critique — use `/plan`,
`/plan-ceo-review` and `/plan-eng-review`. Use those when the app is large or
the cost of a wrong foundation is high, not for a first small app.

### 2. Scaffold

`/cli` has every command and flag. **Supply all flags** — the CLI drops into an
interactive prompt otherwise, which hangs a non-interactive session. `--path` is
one of them: `--help` calls it optional, but omitting it off a TTY dies with
`Error: EOF`.

Foundational choices (module preset, database, auth provider) are expensive to
undo: changing one usually means re-scaffolding, not editing a file. State the
choice and its consequence before you commit to it.

**Then know where things landed.** The manifest sits at the app root; the
workspace sits inside the modules path:

```
my-app/.forklaunch/manifest.toml    <- run `forklaunch` commands here
my-app/src/modules/package.json     <- run `pnpm` commands here
```

`cd my-app && pnpm install` fails with `ERR_PNPM_NO_PKG_MANIFEST`, which reads
like a broken scaffold and is not one. Application names must be letters only.

### 3–4. Pass, then check — every time

After **every** codegen or edit pass, run the deterministic analysis:

```bash
forklaunch score --offline
```

Read-only, no network, no cost, about a second. **Pass `--offline`**: on newer
CLIs the bare command uploads the workspace for agent scoring, which needs an
account and spends credits — not what you want after every pass. It runs the same checks as
`forklaunch compliance audit` and returns a scored card.

Act on it immediately:

- any `critical` or `high` finding → **fix before the next pass.** Every finding
  carries a `fix` field with a specific instruction.
- score dropped since the last pass → the pass you just ran caused it. Fix it
  now, while the change is one pass wide, not ten.
- score unchanged after a fix you believe landed → the fix did not take effect.
  Check that before concluding the check is wrong.

Why every pass and not at the end: these checks catch defects whose failure mode
is *silent* — code that returns correct-looking results and fails only in
production with real tenant data. Ten passes of drift is a bisect; one pass is a
glance. See `/compliance` and `/score`.

The structural snapshot from the same command is also the index the studio
planner consumes — same `appName` / `modules[]` / `entities` / `schemas` /
`routers` / `services` / `workers` shape — so it is what to re-read before
planning a follow-up change:

```bash
forklaunch analyze --pretty
```

### 6. Milestone — the full analyze

At milestones (before a deploy, before handing to a reviewer, end of a work
session), offer the full analyze. Ask; do not run it silently — it costs money
and time where the per-pass check costs neither.

> "Want me to run the full analysis? It scores all five readiness areas, not
> just the two the fast check covers, and produces a shareable report card."

```bash
forklaunch score --offline                  # fast, deterministic, 2 rails, free
forklaunch score --offline --min-score 70   # same, as a CI gate
forklaunch score                            # all 5 rails, agent-scored (newer CLIs;
                                            # needs auth, costs credits, mints a link)
forklaunch compliance audit --risk-score --dpia   # full compliance surface
```

The fast check scores **compliance** and **security** only. Governance,
scalability and observability need judgement a source read cannot supply and
come back `pending` rather than zero. Quote the card's `caveat` whenever you
report a number. `/score` covers reading one properly.

**Compare the milestone card against the plan's score from step 1.** They use
the same five rails, so they are directly comparable — that comparison is the
whole reason for scoring the plan. A rail that the plan scored 80 and the built
app scores 40 means the build drifted from what was agreed, and it is worth
saying so explicitly rather than reporting the second number alone.

### 7. Register — three decisions, made once

Before anything can be deployed, the app needs a record on the platform. That
record settles **three independent questions**, and the point of settling them
here is that the first deploy then has nothing to ask:

```bash
forklaunch app create \
  --name "Clinic Portal" \
  --cluster-type dedicated \
  --compliance-framework HIPAA
```

| flag | the question | why it is decided here |
|---|---|---|
| `--cluster-type` | **where** it runs | shared hosts are ~50× cheaper than a dedicated cluster |
| `--compliance-framework` | **what rules** its data is under | it removes placements that cannot hold that data |
| `--managed` | **how many** of it run | one shared app, or a private copy per customer |

Ask all three in plain language, and connect them to what the user already told
you in step 1:

> "You said this holds patient records. That means it can't run on the cheapest
> shared option — those hosts are shared with other companies. Your realistic
> choices are your organization's own hosts (~$31/mo) or a cluster just for this
> app (~$108/mo). Which fits?"

> "Will every customer get their own private copy of this — their own database,
> their own web address — or does everyone share one system? Most apps share
> one. Per-customer copies is a different product shape, and it's much harder to
> change later than to choose now."

Leaving `--compliance-framework` off means **"not assessed"**, which is not the
same as "none apply" — an undeclared app is unconstrained, and will be offered
placements a regulated app should never take. So ask rather than defaulting.

`app create` links the local checkout at the same time. If the application
already exists, `forklaunch integrate --app <id>` links without creating.

### 8. Ship

```bash
forklaunch release create --version 0.1.0 --local --yes
forklaunch deploy create --release 0.1.0 --environment staging --region us-west-2 \
  --node-env production
```

**`release create` needs the Node toolchain** — it runs `pnpm install` and
exports OpenAPI specs. **Both flags matter off a TTY**: without `--local` or
`--git` it prompts for a mode and dies with `Error: IO error: not a terminal`.

**A release can fail too, for different reasons than a deploy** — a missing
toolchain, the wrong directory, or a plan limit on component count or instance
size. Nothing is deployed at that point, so there are no logs: read the error.
`/investigator` has the table. A plan limit in particular is not something to
engineer around — say which limit was hit rather than quietly dropping a
component to fit under it.

**If placement was not settled in step 7, the first deploy asks.** The platform
answers with the three options and a monthly estimate for each, and refuses to
proceed until one is chosen. On a terminal you get a menu; without one you get
the flag to re-run with. Do not pick for the user — this is a real cost decision
and it is the one moment they are guaranteed to see it.

Read the estimates out loud, and say what the money buys: shared placements pack
the app onto hosts alongside other workloads; a dedicated cluster gives it its
own load balancer and network path. An option can also come back **unavailable**
— no compute pool in that region, too few apps in the org to amortize a shared
host, or the compliance frameworks the app declared. The reason is printed;
relay it rather than just retrying.

Before running `deploy create`, get explicit agreement on the exact release,
environment and region. It provisions real infrastructure and starts costing
money. `--dry-run` previews without deploying — but note it does **not** exercise
the cluster gate, so a clean dry-run is not evidence that the deploy will go
through unprompted.

If the deploy is refused for plan limits — component count, monthly deploys,
instance size — that is a billing wall, not a bug. Say which limit was hit and
that changing it means upgrading the plan in the dashboard.

### Then confirm it actually came up — this is not optional

`deploy create` returning is not the same as the app running. **Always check,
and go read the logs when it did not work.** Reporting a deploy as done without
looking is the single easiest way to hand someone a broken app.

```bash
forklaunch deploy info -e <environment> -r <region>
```

Two failures need handling rather than reporting:

- **Blocked before deploying, on missing environment variables.** The platform
  lists each component and the keys it needs. On a terminal the CLI prompts; off
  one it bails with "Deployment blocked due to missing environment variables".
  Ask for it as data rather than reading the sentence — `forklaunch deploy info
  -e <env> -r <region> --json` returns a `blocked` array and a `remediation`
  list of the exact `config set` commands. Run them once you have the values
  (ask the user; never invent one) — but never hand-set the platform-injected ones
  (`DATABASE_URL`, `REDIS_URL`, `HMAC_SECRET_KEY`, `BETTER_AUTH_SECRET`,
  `ENCRYPTION_KEY`, inter-service URLs); those come from the deploy itself.
- **Deployed, then the container died.** The deploy reports success and nothing
  is up. Read the logs, and **pass `--source cloudwatch`** — the default source
  is the app's own telemetry, which a container that dies at startup never got
  to send, so it will look misleadingly empty:

  ```bash
  forklaunch observe logs -e <environment> --deployment <id> --source cloudwatch
  ```

  A repeating startup trace is a crash loop; `Invalid API Key` means the value is
  set but wrong.

`/investigator` covers both in full, plus everything else that goes wrong after a
deploy. Go there rather than guessing at logs.

## Which skill, when

| you are… | use |
|---|---|
| setting up a machine from scratch | `SETUP.md` in this pack |
| planning a new app or feature | this skill, then `/plan` for heavyweight review |
| running CLI commands | `/cli` — supply all flags |
| looking something up fast | `/quick-reference` |
| writing handlers, services, entities | `/backend-patterns`, `/common-tasks` |
| classifying fields, encryption, tenants | `/compliance` ← read before any entity with personal data |
| reading or gating on a readiness score | `/score` |
| iterating until a score passes | `/score-self-heal` |
| building UI | `/frontend-patterns`, `/design-system`, `/tanstack` |
| deploying a frontend | `/vercel-frontend` |
| registering the app on the platform | `/deploy-mode`, then `/cli` |
| first deploy, or choosing where it runs | `/deploy-mode` ← read before the first deploy |
| selling one private copy per customer | `/managed-apps` |
| connecting GitHub, Stripe, any provider key | `/integrations` |
| a machine with nothing installed | `/prereqs` |
| deploying or operating infra | `/infra`, `/platform-architecture` |
| a release or deploy failed, or deployed and didn't come up | `/investigator` ← read the error, don't guess |
| something is broken in a running app | `/investigator` ← start here for "it's down" |
| auth, secrets, rate limits | `/security` |
| logs, metrics, alerts | `/observability` |

Three are worth reading unprompted: **`/compliance`** before defining any entity
that holds personal data, **`/deploy-mode`** before the first deploy, and
**`/investigator`** the moment something deployed misbehaves.

## Keep the pack current

`forklaunch` self-updates its binary; the skill pack on disk does not. After a
CLI version change, or if an instruction stops matching what a command actually
does (a flag that no longer exists, a different error message), re-run:

```bash
forklaunch context
```

If a command you were told to run is missing or renamed, re-sync before
guessing at flags — the instructions you are holding may simply be stale.

## Close in plain language

Every session ends with a summary a non-technical person could read: what you
built, what the check said, whether the deploy actually came up, and what still
needs their input. No paths, no stack
traces, no jargon — those belong above it. If something is uncertain or
half-done, say so; "I couldn't confirm the deploy finished" beats silence.

## Related

- `SETUP.md` — installing everything, for a non-technical reader
- `/score` — generating and reading a readiness score
- `/compliance` — classification, encryption, tenant isolation
- `/cli` — every command
- `/plan` — heavyweight planning pipeline
