---
name: investigator
description: "Diagnose a deployed ForkLaunch app: is it up, why did a deploy fail, why isn't a change showing, where are the errors. Start here for 'something's wrong' or 'why isn't this working'."
user-invokable: true
---

# ForkLaunch Investigator Skill

Covers the gap between "something's wrong with my deployed app" and "here's what's actually
happening and what to do about it" — using only the `fl` CLI and the ForkLaunch control-plane
APIs it wraps. No AWS console access, no reading platform source, no manual dashboard clicking
required for diagnosis (the dashboard is where a human fixes things afterward, not where the
agent looks first).

This skill is read-only by default. Every command it recommends for the *diagnosis* phase is a
`GET`/list/status call. Anything that changes state (restarting a worker, retrying a DLQ job,
rolling back a deployment) is called out explicitly as a fix step, separate from diagnosis — do
not run a fix step without telling the user what you're about to do and why.

## Prerequisites

```bash
forklaunch --version                    # confirm the CLI is present and self-updates on next run
forklaunch login                        # opens a browser device-code flow; --token <jwt> for CI
```

Every command below needs to run from inside the project directory (it reads
`.forklaunch/manifest.toml` for `platform_application_id`), or pass `-p/--path <dir>`.

If `.forklaunch/manifest.toml` has no `platform_application_id`, the project was never linked to
a platform application — run `forklaunch integrate --app <app-id>` first (get `<app-id>` from the
dashboard's application overview page, or `forklaunch app create` if the application doesn't
exist yet). Nothing else in this skill works without that link.

Most commands take `-e/--environment <env>` (e.g. `production`, `staging`) — ask which
environment if it's not obvious from context, don't guess.

## Start here: one-screen health check

```bash
forklaunch observe status -e <environment>
```

This is the single entry point — it pulls request rate, error rate, p95 latency, uptime, and an
overall status derived from metrics/logs/traces signals in one call. Read this first, always,
before reaching for anything more specific. It tells you *which* of the deeper tools below is
worth reaching for next:

- **Overall unhealthy / high error rate** → go to [Logs](#logs-what-actually-happened) for the
  specific error, then [Traces](#traces-where-time-is-going) if it's a latency/timeout pattern
  across services.
- **Everything "unknown"** (not "healthy" or "unhealthy") → the app most likely has no traffic in
  this environment/time window, or nothing is deployed there yet. Check
  [Deployment status](#deployment-status) before assuming something's broken.
- **Healthy overall, but there is no recent telemetry at all** (metrics stop at a
  timestamp, an `INCIDENT` issue names a monitoring container) → the instrument
  broke, not the app. Go to
  [Your telemetry went quiet](#your-telemetry-went-quiet-or-you-got-a-capacity-incident).
- **Healthy overall, but the user says a specific thing is wrong** (a feature not showing up, a
  config change that doesn't seem to have taken effect) → go to
  [Did my change actually land?](#did-my-change-actually-land) — this is the single most common
  "it looks fine but it's not" case.

## A release just failed — start here

`release create` runs before any deploy and fails for reasons that have nothing
to do with the cloud. Nothing is deployed, so there are no logs — read the error.

| what it says | what it means | what to do |
|---|---|---|
| `Error: IO error: not a terminal` | it needs a release mode and has no TTY to prompt on | re-run with `--local --yes` (or `--git --yes`) |
| `tsx not found ... Run pnpm install first` | the Node toolchain is missing or deps are not installed | `/prereqs`, then `pnpm install` from `<app>/<modules-path>` — **not** the app root |
| `Component limit exceeded (N/M)` | a plan limit, not a bug | the message names the limit and links the pricing page; the org owner has to change the plan |
| `Instance size "X" exceeds the maximum allowed` | a plan limit on component size | size down, or upgrade |
| `ERR_PNPM_NO_PKG_MANIFEST` | you are in the app root; the workspace is inside the modules path | `cd <app>/<modules-path>` |
| a lockfile / minimum-release-age error | a dependency published in the last 24h fails the freshness gate | retry, or `--no-frozen-lockfile` once |

**A plan limit is not something to engineer around.** Say which limit was hit and
that changing it needs a plan change — do not quietly drop a component or shrink
an instance to squeeze under it without asking.

## A deploy just failed — start here

This is the most common way you get here, and it has its own path because a
failed deploy has **two different failure shapes** that look nothing alike.

**Never report a deploy as finished without checking it.** `deploy create`
returning is not the same as the app running.

```bash
forklaunch deploy info -e <environment> -r <region>     # what state is it in?
```

### Shape 1 — the platform refused before deploying (missing env vars)

The deploy is blocked pre-flight with a message naming each component and the
keys it needs. Nothing was deployed, so there are no logs to read yet.

**Get it as data, not prose.** The failure is stored as one flattened sentence,
so ask for JSON and let the CLI recover the structure:

```bash
forklaunch deploy info -e <environment> -r <region> --json
```

```json
{ "deployments": [ {
    "status": "failed",
    "errorMessage": "Deployment blocked due to missing configuration: worker 'managed-apps-worker' missing keys: TEMPLATE_BUILD_ORG_ID",
    "blocked": [ { "componentType": "worker", "name": "managed-apps-worker",
                   "missingKeys": ["TEMPLATE_BUILD_ORG_ID"] } ],
    "remediation": [ "forklaunch config set TEMPLATE_BUILD_ORG_ID=<value> -e production -r us-west-2 -s managed-apps-worker" ]
} ] }
```

`remediation` is the command to run once you have the value — it is already
scoped to the component that needs it. **You still have to find the value.** Do
not invent one: ask the user, or read it off the component that already has it
if the message says the key is set elsewhere. A wrong value turns a blocked
deploy into a crash loop, which is harder to diagnose than the block was.

**This is also how you act on a "Deploy failed" email.** Those notifications go
to a person, and their only call to action is a dashboard button — there is no
agent-readable payload. When a user forwards or pastes one, do not work from the
screenshot: take the application, environment and region off it, run the command
above, and act on the structured answer. An autodeployed failure (a push-triggered
release) has no CLI session watching it, so nothing will tell you unless you ask.

On a terminal the CLI prompts for the values and retries. **Off a terminal — CI,
or any non-interactive agent session — it bails**, with:

> Deployment blocked due to missing environment variables.

Fix by setting them, then re-running the deploy:

```bash
forklaunch config set KEY=value -e <environment> -r <region> -s <service>
```

Two things to get right before you start typing:

- **Do not set platform-injected variables by hand.** `DATABASE_URL`,
  `REDIS_URL`, inter-service URLs, `HMAC_SECRET_KEY`, `BETTER_AUTH_SECRET`,
  `ENCRYPTION_KEY` and friends are resolved from Pulumi outputs at deploy time.
  The CLI filters most of these out of the blocked list already; if one survives
  into the prompt, that is a signal to check the manifest, not to invent a value.
- **Scope provider keys to the service that needs them** with `-s`, rather than
  letting them land in every container.

### Shape 2 — it deployed, then the container died

Far more confusing, because the deploy reports success and the app is simply not
up. Almost always a variable that was *present but wrong*, or one only read at
runtime.

**Read the logs with `--source cloudwatch`.** This is the important part: the
default `otel` source only has what the app's telemetry pipeline managed to
flush, and a container that dies during startup never flushes anything. The
OTel view will look empty and you will conclude there is nothing to see.

```bash
forklaunch observe logs -e <environment> --deployment <deployment-id> \
  --source cloudwatch --limit 200
```

Signatures worth recognising:

| what you see | what it means |
|---|---|
| the same startup trace repeating every few seconds | crash loop — the task restarts, dies, restarts |
| `getEnvVar` / "is not defined" / "required" at startup | a variable is missing at runtime that the pre-flight gate did not require |
| `Invalid API Key provided` | the variable is set but wrong — a placeholder, or a test key in production |
| nothing at all in `otel`, plenty in `cloudwatch` | it died before telemetry started; trust cloudwatch |
| healthy start then errors under traffic | not an env-var problem — go to [Logs](#logs-what-actually-happened) |

Then check whether the platform already noticed:

```bash
forklaunch observe issues -e <environment>     # the platform's own findings
forklaunch dlq stats                           # background jobs that died
```

### Report it in plain language

When you come back to the user, say which shape it was and what you changed —
"the billing service had no Stripe key, so it restarted in a loop; I've set it
and redeployed" — not "deploy failed, see logs". If you set a variable, say
which one and at what scope. If you could not confirm it recovered, say that too.

## Deployment status

```bash
forklaunch deploy info -e <environment>              # latest deployment per environment/region
forklaunch deploy info -d <deployment-id>             # one specific deployment, by id
```

Confirms whether a deployment is actually running, still in progress, or failed outright — before
assuming an application-level bug. A failed or stuck deployment explains "nothing is happening"
far more often than application code does.

**`deploy info` showing `completed` does not mean the service is healthy yet.** There's a real gap
between the infrastructure update finishing and the service actually reaching steady state — the
new task still has to start, pass its health check, and register healthy. Checking `app services`
or `observe status` in that window can show an `error`/unhealthy service that resolves itself
within a minute or two with no action taken. If you see this right after a deploy completes, wait
briefly and recheck before treating it as a real failure — don't chase logs or roll back based on a
status snapshot taken seconds after `completed`.

**Critical known gap: `completed` + `running` can both be lying — a deploy can silently roll back
and nothing here detects it.** This is different from the transient window above and does *not*
resolve itself. If a new task never becomes healthy, ECS's own deployment circuit breaker rolls
back to the previous task automatically — but `deploy info` still reports the deployment as
`completed` (it's reporting on the infrastructure update succeeding, not on the resulting task
staying healthy), and `app services` still reports the new version as `running`. The app is
actually still serving the *old* code. Confirmed by direct reproduction: a release whose container
crashed at boot showed `completed`/`running` throughout, with zero trace of the crash in any
`observe logs` query (default or `--source cloudwatch`, with or without `--since`) — the only way
to catch it was hitting the live URL directly and noticing the response didn't match what the new
code should do. If the user says "I deployed a fix and it's not showing up" and everything above
reports healthy, don't stop there — but don't unilaterally send requests to the live service either:
there's no `fl` command for this (see [Did my change actually
land?](#did-my-change-actually-land)), and probing an arbitrary endpoint yourself risks real side
effects on the customer's actual app (a `POST` isn't guaranteed side-effect-free). Ask the user to
check the specific behavior the new release should show, or to confirm a safe read-only endpoint
you can call together. Say this plainly if the reported status and the actual behavior disagree:
"the platform reports this deploy as successful, but the running behavior doesn't match — this
looks like a silent rollback the tooling can't currently detect."

**Known CLI gap: no way to see a deployment's own build/infra log stream.** The dashboard has a
live "Deployment Logs" panel (the full Pulumi/build output, with info/warn/error counts) for every
deployment, successful or not. The CLI has nothing equivalent — `deploy info` on a **successful**
deployment returns only status and timestamps, no logs at all. It only surfaces log content when a
deployment *failed* (the error field happens to include a tail of stdout/stderr), and even then
it's a snapshot, not the full stream. If you need to see what actually happened during a successful
deploy, or need the full unabridged output of a failed one, say so plainly: "the CLI doesn't expose
deployment logs — check the dashboard's Deployment Logs panel for this."

If a deployment failed and the user wants to go back to a known-good version:

```bash
forklaunch deploy info -e <environment>    # find the deployment id and the release id to roll back to
forklaunch deploy rollback -d <deployment-id> -t <target-release-id> --reason "<why>"
```

This is a state change — confirm the target release with the user before running it, `deploy
info` doesn't pick one automatically.

## Logs: what actually happened

```bash
forklaunch observe logs -e <environment> --level error         # start narrow: errors only
forklaunch observe logs -e <environment> -s <service-name>      # narrow to one service
forklaunch observe logs -e <environment> -q "<text>"            # grep-style text filter
forklaunch observe logs -e <environment> -f                     # live-tail, for "watch it happen"
```

Two log sources exist, controlled by `--source` (default `otel`):

- `otel` — the app's own OpenTelemetry pipeline. Structured, filterable, but **silent for
  anything that crashes before the app's own OTel exporter initializes** (e.g. a boot-time
  config-validation failure). If `observe status` shows unhealthy but `--source otel` logs show
  nothing useful, this is why.
- `cloudwatch` — raw container stdout/stderr, captured independently of the app's own telemetry.
  Use `--source cloudwatch` specifically when you suspect a crash-on-boot, since it captures
  output the OTel pipeline never got to flush.

```bash
forklaunch observe logs -e <environment> --source cloudwatch --level error
```

## Metrics and traces: where time is going

```bash
forklaunch observe metrics -e <environment> --time-range 1h     # request rate, error rate, latency
forklaunch observe traces -e <environment> --time-range 1h      # recent trace list
forklaunch observe traces -e <environment> --trace-id <id>      # full span tree for one trace
```

Use traces when logs show *that* something failed but not *where* — a trace's span tree shows
which service/call in a request chain actually took the time or threw, across service
boundaries. `observe status`'s summary numbers come from the same underlying metrics; `observe
metrics` is the same data with more time-range control.

`forklaunch observe query "<promql>"` is a raw PromQL escape hatch for anything the above doesn't
cover — application metrics only (request rate/latency/error rate), **not** CloudWatch-sourced
infrastructure metrics (see [Infrastructure health](#infrastructure-health-database-cache-queue)
for those).

## Issues: the platform's own findings

```bash
forklaunch observe issues -e <environment>                          # list active issues
forklaunch observe issues -e <environment> --severity ERROR         # filter by severity
```

The platform surfaces its own correlated findings here (not just raw log lines) — check this
alongside logs, not instead of them. Two distinct actions once you've found the relevant issue:

```bash
forklaunch observe issues ack <issue-id>       # mark as seen, stays open — you're still working it
forklaunch observe issues resolve <issue-id>   # mark as closed/fixed — only after it's actually fixed
```

These are state changes — only run them once you've confirmed the underlying situation with the
user, not automatically as part of diagnosis.

## Your telemetry went quiet, or you got a capacity incident

The instrument itself can fail. When it does, every other section here reads
"healthy" — because the thing that would have told you otherwise is what broke.

The notification looks like this:

```
New Issue Detected
Title: tempo was OOM-killed at its 768 MiB limit
Severity: INCIDENT
Service: monitoring-stack
```

### Why the obvious alarm stayed silent

These are **per-container** limits. The `AWS/ECS MemoryUtilization` alarm is
**task**-level, and a task holds six containers. One container can sit at 100% of
its own limit and be killed while the task it lives in never crosses 80% — so the
alarm is, correctly, quiet. No threshold on a task-level metric can catch this;
that is why a separate evaluator exists, reading per-container memory out of
Container Insights.

So: **an application that looks completely healthy can have lost its telemetry.**
Treat "no metrics since <time>" as a symptom, not as calm.

### The trap that keeps this recurring

Every measurement of tempo has been taken at a limit tempo was already
saturating: peak 383 against a 384 limit, then an OOM at 768. **A peak equal to
the limit is a censored observation, not a measurement.** Raising the limit and
reading the new peak reproduces the same censored number one tier up.

When you report a peak, say what the limit was. If they are equal, say the real
figure is unknown and higher.

### Fixing it

Sizing now scales with the application: components are sized from
`sqrt(workload / one micro component)`, so a nine-component app gets roughly
three times a one-component app. Most stacks need nothing done.

When one still needs more, **the per-app override is per-component**:

```bash
PUT /applications/:id/observability/:environment/:region
{ "providers": { "tempo": { "memory": 1344 } } }
```

**The task-level `resources` override cannot fix a container OOM.** It sets total
task cpu/memory; the limit that killed the container is its own. You can make the
task 4 GiB and tempo will still die at its container cap. This is the single most
common wrong turn here.

### Check for stranded memory before paying for more

Container hard limits must sum to at most task memory — and often sum to less.
That difference is unusable by any container, because caps are hard while task
memory is shared. Reassigning it costs nothing; growing the task costs money.

The existing-stack set, for example, allocates 1472 MiB of a 2048 MiB task: 576
MiB stranded, free to give away. Add the per-container limits, compare to the
task total, and spend the gap first.

### What to actually run

```bash
forklaunch observe issues -e <environment>     # the finding, and its severity
forklaunch observe status -e <environment>     # will look healthy — that is the point
```

Then read per-container `MemoryUtilized` for the monitoring task in Container
Insights (`/aws/ecs/containerinsights/<cluster>/performance`). It is a CloudWatch
Logs group, not a Prometheus series, so `observe query` cannot reach it.

### Report it in plain language

"Your monitoring stack ran out of memory and stopped recording traces for about
an hour. The app itself was fine the whole time — which is why nothing else
alerted. I've raised the limit using memory the task was already paying for, so
this costs nothing." Not "tempo OOMKilled, resized to 1344".

## Infrastructure health (database, cache, queue)

Application-level metrics (`observe metrics`) do **not** include database/cache CPU, memory, or
connection count — those live on the resource itself, not the application's OTel pipeline:

```bash
forklaunch infra list -e <environment>                                    # what's provisioned
forklaunch infra status <project>:<type> -e <environment>                 # e.g. billing-service:database
forklaunch infra status <project>:<type> -e <environment> --metrics       # CPU%/memory%/connections
forklaunch infra status <project>:<type> -e <environment> --config        # just the manifest config
```

`<project>:<type>` is `<project-name>:database|cache|queue|object-store` — get the exact project
name from `forklaunch app services` if unsure. Reach for `--metrics` specifically when the
application looks healthy but is slow, timing out, or erroring against its own data layer —
that's a strong signal to check whether the database/cache itself is saturated before assuming
application code.

**Known platform gap: this can come back completely broken, even for a real, working database.**
`infra list` can show a resource with `serviceName: null`, `region: "_TEMPLATE_"`, and
`status: "template"` — a placeholder record that was never linked to the actual provisioned
resource after deploy. When that happens, `infra status <project>:<type>` fails to resolve the
project by name at all ("no resource found"), and even bypassing that with `--resource-id
<id-from-infra-list>` returns no real metric data — the record itself is just never populated,
independent of whether the underlying database is actually fine. Don't take this as evidence the
database is broken; it means the platform can't report on it right now. Say so plainly rather than
guessing at database health from a template placeholder.

## Dead-letter queue (failed background jobs)

```bash
forklaunch dlq --limit 20                    # list stuck jobs (default limit 100)
forklaunch dlq stats                          # counts: total / waiting / processed
forklaunch dlq retry <job-id>                 # re-run a job — state change, confirm first
```

A growing DLQ is often the first visible symptom of a worker problem before anything else shows
it — check this if the user mentions background jobs, emails, or async processing not happening.

## Workers (background job processors)

```bash
forklaunch worker pause <worker-id>
forklaunch worker resume <worker-id>
forklaunch worker restart <worker-id>          # forces a new deployment of the worker
```

All three are state changes. Use `restart` when a worker is deployed but not actually picking up
jobs (a common symptom: DLQ growing, but the worker's own logs show nothing) — this is usually
the same class of problem as [Did my change actually land?](#did-my-change-actually-land) below,
just for a worker instead of a service.

## Hosting-configuration drift

```bash
forklaunch drift check
```

Flags services/workers whose actual hosting tier no longer matches what the plan allows (e.g.
something running on a tier that got downgraded or deprecated). Worth a quick check if nothing
above explains a persistent issue — this is infrastructure-level, not application-level, so it
won't show up in logs or metrics at all.

## Alerts and notifiers

```bash
forklaunch alerts -e <environment>                       # list configured alert rules
forklaunch notifiers                                      # list configured notification targets
```

If the user says "I should have been alerted about this and wasn't," check these before assuming
the platform missed something — it's just as often a missing alert rule or a notifier with no
working webhook/email.

## Did my change actually land?

The single most common false alarm: a deploy reports success, but a config change (an env var, a
feature flag) doesn't seem to be reflected in the running app. Diagnosis order:

1. `forklaunch deploy info -e <environment>` — confirm the deployment you expect is actually the
   *latest* one, not superseded or still in progress.
2. `forklaunch observe logs -e <environment> -s <service> --source cloudwatch` — check the
   service's actual boot logs for the config value in question, if it's ever logged at startup.
2.5. `forklaunch config pull -e <environment> -r <region> -s <service> -o <file>` pulls the
   **actual, unmasked** current values (secrets included) for that scope, not just the ones the
   user set explicitly — useful because the value a service actually uses may not be the one
   you'd expect (a connection-string var like `DB_URL` can embed its own credential, independent
   of a discrete var like `DB_PASSWORD` sitting right next to it — confirmed empirically:
   overriding `DB_PASSWORD` alone had zero effect because the app's ORM used `DB_URL` instead).
   **This writes real secrets to a plain file — treat it as a mutation, not a free read.** Get the
   user's explicit go-ahead before running it, same as any other sensitive action. Write it to a
   private temp location (e.g. `mktemp`, not a path inside the project directory), and delete it
   immediately after you've read what you needed — never print its contents into chat or logs.
   Note what `config pull` does **not** give you: there's no timestamp field anywhere in its
   output, only current values — it cannot tell you *when* a value was last changed (see the gap
   below).
3. **Known CLI gap:** the platform has a dedicated API
   (`GET /services/:id/runtime-status`) that directly answers "does the config the platform has
   pushed match what's running on the live task" (`configPushedAt` vs. the running task's start
   time), but **no CLI command wraps it yet**, and `config pull` cannot substitute for it — its
   output has no push timestamp to compare against a deployment time. Until a real command exists,
   there is no reliable CLI-only way to answer "did this specific config change land." Say this
   plainly to the user rather than guessing: "the platform can answer this more directly than I
   currently can — if you're unsure, a fresh deploy will pick up the latest config regardless."

## Reference: command → what it answers

| Question | Command |
|---|---|
| Is anything wrong right now? | `observe status -e <env>` |
| Did the deployment actually succeed? | `deploy info -e <env>` |
| What error actually happened? | `observe logs -e <env> --level error` |
| Did it crash before logging anything? | `observe logs -e <env> --source cloudwatch` |
| Where in the request chain did it fail/slow down? | `observe traces -e <env>` |
| What has the platform already flagged? | `observe issues -e <env>` |
| Is the database/cache/queue itself the problem? | `infra status <p>:<t> -e <env> --metrics` |
| Are background jobs stuck? | `dlq stats` / `dlq --limit 20` |
| Is a worker actually processing jobs? | `worker restart <id>` (after confirming it's stuck) |
| Is this a hosting-tier/plan problem, not code? | `drift check` |
| Would I have been alerted about this? | `alerts -e <env>` / `notifiers` |
