---
name: infra
description: "fl infra: add/remove a capability in a service's code (object store, cache); list, status, resize, config-set, stop, delete provisioned database/cache/queue/object-store resources."
user-invokable: true
---

# ForkLaunch Infra Skill

## Prerequisites

```bash
forklaunch --version   # must be >= 1.3.3 — `list`/`status`/`resize`/`config-set`/`stop`/`delete`
                        # all fail with "invalid type: map, expected a sequence" on 1.3.0–1.3.2
                        # (resource-management wraps its list response in {"resources": [...]},
                        # earlier CLI builds expected a bare array)

forklaunch login                        # JWT/session only — see "HMAC not supported" below
forklaunch integrate --app <app-id>     # required; writes platform_application_id into manifest.toml
```

Every `fl infra` command except `add`/`remove` fails fast with `"Application not integrated with platform"` if `integrate` hasn't run.

**`fl infra add|remove` edits code; everything else manages what the platform provisioned.**
`forklaunch infra add <service> object-store|cache` writes the capability into
the service (registration, docker-compose, env, dependency, manifest resource,
test utilities) and `infra remove` undoes it. No login or `integrate` is needed.
The platform provisions the real resource on the next deploy: for an object
store, a private encrypted bucket, CORS for the app's own domains, and a task
role scoped to that bucket, with no keys. `infra add <service> payments` wires
Stripe Connect through the platform instead of a Stripe key (see `/integrations`).
See `/cli` for flags.

`forklaunch infra add <service> email` registers the platform-held
`EmailClient` (SES through the instance gateway, no mail key) and an email
events handler; see `/integrations`.

`forklaunch infra add <service> sms` wires platform-held text messages the
same way: an `SmsClient` registration, an SMS event handler and the local
gateway mock; the platform sends them (End User Messaging SMS), so the service
holds no vendor key. See `/integrations`.

`forklaunch infra add <service> whatsapp` adds a platform-held capability instead:
a keyless `WhatsAppClient`, a WhatsApp event handler and the local gateway mock
(see `/integrations`).

`forklaunch infra add <service> voice` adds outbound phone calls for a managed
app: the platform places them through its Amazon Connect instance, so the
service gets a keyless `VoiceClient` and a call-events handler, not AWS
credentials. See `/integrations` (*Voice calls*).

**The remaining `fl infra` commands never provision anything.** It only inspects and manages resources that already exist on the platform for the target environment (created through the normal platform provisioning flow). Running any command against an environment with no provisioned resources yet will correctly report zero results, not an error.

## When to Use This Skill

Use this skill when the user asks to:

- List, inspect, resize, reconfigure, stop, or delete a provisioned database/cache/queue/object-store resource
- Check the live status or configuration of a resource without opening the platform dashboard
- Script a resource change (CI, automation) — note the JWT-only restriction below before assuming this is possible

## HMAC/CI mode is NOT supported (v1)

`fl infra` requires interactive JWT/session auth (`forklaunch login`). If `AuthMode::Hmac` is detected (e.g. `FORKLAUNCH_API_TOKEN`/CI context), every subcommand fails immediately with a clear message rather than attempting the call — there is no way to script `fl infra` in CI today. This is a deliberate v1 scope cut, not a bug: resource-management's routes use a discriminated `AccessLevel` (`protected` XOR `internal`) that forbids RBAC on HMAC-only routes, so adding CI support safely needs its own design.

## Resource identifier syntax

```
<project-name>:<resource-type>
```

`resource-type` is one of: `database`, `cache`, `queue`, `object-store` (matches the manifest's `[projects.resources]` field names — NOT the platform's raw `messagequeue`/`objectstore` strings).

```bash
fl infra status billing-service:database --environment staging
fl infra status auth:cache --environment production
```

**Resolution requires an exact match** between `<project-name>` and the platform resource's `serviceName` — and these can legitimately drift apart. Observed in the wild: a manifest project named `agents` had a platform `serviceName` of `agents-service`, so `agents:database` resolved to zero matches even though the resource existed. When resolution fails or is ambiguous, run `fl infra list` first to see real `serviceName`s, then either fix the mismatch at the source or use the escape hatch:

```bash
fl infra status --resource-id <id> --environment staging <anything>:<type>
# --resource-id bypasses name resolution entirely — the <project>:<type> positional
# arg is still required by clap but its value is ignored once --resource-id is set
```

## Commands

### 1. `fl infra list` — list all provisioned resources

```bash
fl infra list --environment <env> [--json]

# Required: -e, --environment <env>
# Optional: -p, --path <app-root>   (defaults to cwd)
#           --json                  raw JSON instead of the formatted table

# Examples:
fl infra list --environment staging
fl infra list -e production --json
fl infra list --environment dev --path ./my-app
```

No identifier resolution — just fetches everything for the app in that environment. Good first command to run: confirms auth + manifest + integration + the resource-management round-trip all work, with zero mutation risk.

### 2. `fl infra status` — inspect one resource

```bash
fl infra status <project>:<type> --environment <env> [--config] [--json] [--resource-id <id>]

# Required: <resource> positional, -e/--environment
# Optional: --config        show only manifestConfig (instance_class, engine, port, etc.)
#           --json          raw JSON (full detail, or manifestConfig only if --config is also set)
#           --resource-id   bypass name resolution
#           -p/--path

# Examples:
fl infra status billing:database --environment staging
fl infra status billing:database --environment staging --config
fl infra status billing:database --environment staging --json
fl infra status billing:database --environment staging --config --json
fl infra status --resource-id 9d21a314-44e9-4d8d-a006-9e0cafe80689 --environment staging billing:database
```

Read-only — a `GET`, safe to run against real resources anytime.

### 3. `fl infra resize` — change sizing fields

```bash
fl infra resize <project>:<type> --environment <env> [sizing flags] [--snapshot-before-change] [--yes|-y] [--dry-run] [--resource-id <id>]

# Sizing flags (pass at least one, or it fails fast before any network call):
--instance-class <class>         # database, e.g. db.t3.small
--allocated-storage <GB>         # database
--node-type <type>               # cache
--num-cache-nodes <n>            # cache
--number-of-broker-nodes <n>     # queue (Kafka/MSK)
--ebs-storage-size <GB>          # queue

# Safety flags:
--snapshot-before-change         # database only
-y, --yes                        # skip the confirmation prompt (CI/scripted)
--dry-run                        # resolve + fetch current config + print diff, then STOP —
                                  # no confirm prompt, no PATCH/deploy call, no AWS mutation

# Examples:
fl infra resize billing:database --environment staging --instance-class db.t3.small --dry-run
fl infra resize billing:database --environment staging --allocated-storage 50 --snapshot-before-change
fl infra resize auth:cache --environment staging --node-type cache.t3.small --num-cache-nodes 2 -y
fl infra resize payments:queue --environment staging --number-of-broker-nodes 3 --ebs-storage-size 100
```

**`--dry-run` is the safe way to validate flags against real data** — it does a real resolve + `GET` (read-only), prints a current-vs-requested diff, and returns before the confirmation prompt or any mutating call. No tier presets exist (`small`/`medium`/`large`) — pass the raw AWS-shaped values directly.

Without `--dry-run` or `--yes`, it drops into an interactive confirm prompt (`dialoguer::Confirm`) before calling `POST /:id/deploy` — this has not been exercised in a non-TTY context, but based on this CLI's other interactive prompts (see `/cli` skill's version-gate and `delete` notes), expect it to fail with a bare I/O error rather than hang, if stdin has nothing to read. Always pass `-y` when scripting.

**Not yet live-verified this session:** the actual (non-dry-run) mutation path — confirmed via `--dry-run` and unit tests only, not against a real AWS resize.

### 4. `fl infra config-set` — change engine/behavior config

```bash
fl infra config-set <project>:<type> --environment <env> [config flags] [--distribution-strategy <s>] [--primary-region <r>] [--snapshot-before-change] [--yes|-y] [--dry-run] [--resource-id <id>]

# Config flags (pass at least one, or it fails fast before any network call):
--engine <engine>                        # database/cache engine
--multi-az                               # flag, no value — enables Multi-AZ (database)
--queue-type <type>                      # queue
--visibility-timeout <seconds>           # queue
--message-retention-seconds <seconds>    # queue
--encryption <setting>                   # database/cache/queue
--kafka-version <version>                # queue (Kafka/MSK)
--port <port>                            # database/cache

# Distribution metadata (routes through a fast synchronous PATCH, not POST /deploy,
# if set WITHOUT any of the config flags above):
--distribution-strategy <centralized|distributed>
--primary-region <region>

# Same safety flags as resize: --snapshot-before-change, -y/--yes, --dry-run

# Examples:
fl infra config-set billing:database --environment staging --engine postgres15 --dry-run
fl infra config-set billing:database --environment staging --multi-az -y
fl infra config-set payments:queue --environment staging --visibility-timeout 60 --message-retention-seconds 1209600
fl infra config-set billing:database --environment staging --primary-region us-east-2 --dry-run
# ^ metadata-only path — dry-run message differs ("would PATCH ...") since it skips POST /deploy entirely
```

Same `--dry-run` safety guarantee as `resize` — read-only resolve + fetch + diff print, no mutation. The metadata-only branch (only `--distribution-strategy`/`--primary-region` set) is instant (`PATCH`, synchronous, no deployment polling); any other config flag routes through `POST /:id/deploy` and polls to completion, printing the dashboard URL before polling starts.

**Not yet live-verified this session:** same caveat as `resize` — dry-run and unit tests only, no real AWS config change exercised yet.

### 5. `fl infra stop` — stop a resource

```bash
fl infra stop <project>:<type> --environment <env> [--yes|-y] [--resource-id <id>]

# No --dry-run exists for this command — running it without --yes goes straight to an
# interactive confirm prompt, and without a TTY that prompt has no safe preview step.

# Examples:
fl infra stop billing:database --environment staging
fl infra stop billing:database --environment staging -y
```

Confirm prompt: `"Stop <name> (<type>)? This may cause downtime for anything depending on it."` — declining aborts with no API call. **Not yet live-tested this session** — treat as implemented-but-unverified against a real resource.

### 6. `fl infra delete` — permanently delete a resource

```bash
fl infra delete <project>:<type> --environment <env> [--yes|-y] [--resource-id <id>]

# --yes skips the confirmation entirely, INCLUDING the type-to-confirm step.
# Without --yes: you must type the exact resource name back to proceed.

# Examples:
fl infra delete billing:database --environment staging
fl infra delete old-service:cache --environment staging -y
```

**Irreversible — no automatic snapshot is taken.** The confirmation requires typing the resource's exact `name` (not the `<project>:<type>` identifier) back at a prompt; any mismatch aborts with nothing deleted. `-y` bypasses this entirely — treat it as equivalent to confirming blind, never use it against a real resource without a separate `fl infra status` check first. **Not yet live-tested this session.**

## Known gotchas (verified live against a real staging app, July 2026)

- **Missing prod DNS record for `resource-management.forklaunch.com`** — unlike `iam`/`billing`/`platform-management`, this hostname currently has no DNS record at all (fails at DNS lookup, not connection). Workaround until fixed:
  ```bash
  export FORKLAUNCH_RESOURCE_MANAGEMENT_API_URL=https://resource-management.forklaunch-production-us-west-2-0aef7f56.app.forklaunch.com
  ```
  (Same env-var-override pattern as `FORKLAUNCH_OBSERVABILITY_API_URL` in the `/cli` skill.) Confirm with whoever owns Route53 for forklaunch's own AWS account whether this has since been added before assuming the override is still needed.
- **`serviceName` vs manifest project name can drift** (see Resource identifier syntax above) — don't assume `<project>:<type>` will always resolve; fall back to `fl infra list` + `--resource-id` when it doesn't.
- **Version-gate EOF** — same class of issue documented in `/cli` for the main CLI: if the app's `.forklaunch/manifest.toml` `cli_version` doesn't match the installed CLI, every command (including all of `fl infra`) prints a version-mismatch warning and then tries an interactive "install the required version?" prompt. In a non-TTY context this dies with a bare `Error: EOF`. Fix by bumping `cli_version` in the manifest to match the installed CLI, not by trying to answer the prompt.
- **`fl infra creds` does not exist yet** — blocked on a new platform route (`GET /platform-resources/:id/credentials`) plus security sign-off on role-gating. Don't assume it's available.
- Two resources can show up in `fl infra list` with `serviceName: ?` (rendered as `?` when absent) — these are orphaned or platform-level resources with no `serviceName` set, and cannot be addressed via `<project>:<type>` at all; `--resource-id` is the only way to reach them.

## Related Documentation

- CLI scaffolding/change/delete/deploy/release commands → `/cli`
- Manifest structure and `.forklaunch/manifest.toml` conventions → `/cli`
