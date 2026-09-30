---
name: score
description: "forklaunch score: generate an Enterprise-Readiness Report Card, read the five rails, gate CI on a minimum score, and know what deterministic checks can and cannot judge."
user-invokable: true
---

# Enterprise-Readiness Report Card

## When to Use This Skill

- "How ready is this app?" / "score my app" / "is this production-ready?"
- Generating a report card locally, or in CI
- Reading a card someone handed you and deciding what to fix first
- Gating a pipeline on a readiness score
- Understanding why a rail says "not assessed" instead of a number

## The one-sentence version

A report card scores an app across five areas and shows the evidence behind
each score. The CLI produces the part that can be proved from your source code,
offline, in about a second — and is honest about the part that needs judgement.

## Generate one

```bash
forklaunch score
```

That is a **read-only** command — it never writes to your workspace. Whether it
touches the network depends on the version: on CLI 1.10.0 and earlier it is
purely local; on newer CLIs the bare command uploads the workspace for agent
scoring unless you pass `--offline`. Either way it runs the same deterministic
checks as `forklaunch compliance audit` and presents them on the shared
report-card contract.

| flag | what it does |
|---|---|
| *(none)* | a readable terminal summary — the default |
| `--offline` | deterministic checks only: no upload, no auth, no cost (newer CLIs) |
| `--no-share` | score online but skip minting the share link (newer CLIs) |
| `--json` | the raw report card, for tooling (includes `remediationPrompt`) |
| `--prompt` | print the remediation prompt for an agent instead of the summary |
| `--pretty` | pretty-print the JSON |
| `--min-score N` | exit non-zero if `overall` is below N — for CI |
| `-p, --path <dir>` | app root (defaults to the manifest in the current directory) |

**In a build loop, use `--offline`.** The per-pass check is supposed to be free
and instant; on a newer CLI the bare command uploads and bills. `--offline` is
also the only form that works with no network or no account.

Gate a pipeline:

```bash
forklaunch score --offline --min-score 70
```

`--offline` matters in CI: without it a newer CLI needs credentials and spends
credits on every pipeline run.

Exit code 1 with `report card overall score N is below the required minimum of 70`.

## The five rails

| rail | weight | scored by `--offline`? |
|---|---|---|
| Compliance | 0.25 | **yes** — encryption at rest, data classification |
| Security | 0.25 | **yes** — tenant isolation |
| Governance | 0.15 | **yes** on newer CLIs — data retention, GDPR erasure (CLI 1.10.0 and earlier: no) |
| Scalability | 0.15 | no — needs judgement |
| Observability | 0.20 | no — needs judgement |

Offline checklist items use the platform's criterion ids and exact labels
(`cmp-encryption-at-rest`, `sec-tenant-isolation`, `gov-data-retention`, …),
so they are the same lines the website shows in its "Checked" half. Each item
carries its `criterion` id and the `checks` that bear on it.

### Why the other rails say "not assessed"

Because a source read genuinely cannot decide them. Whether the system
survives its actual load, whether the instrumentation covers what matters —
none of that is visible in the code.

Those rails come back with `pending: true` and are **excluded from the average**
rather than scored zero. That distinction matters: zeroes would cap every card
at 50 and read as *failure* when the truth is *absence*. `overall` is the
weighted mean of the rails that were actually scored.

**How you get the other three depends on your CLI version, so check it.**

- **CLI 1.10.0 and earlier**: there is no way. The command has exactly four
  options — `--json`, `--pretty`, `--min-score`, `--path` — all offline, two
  rails, full stop. (Older docs mention `--upload`; that flag never existed.)
- **Newer CLIs**: `forklaunch score` **uploads by default** and scores all five
  with an agent, which needs auth and costs credits, and mints a shareable link.
  `--offline` is the opt-out that gives you the old deterministic-only behaviour,
  and `--no-share` keeps the upload but skips the link.

Run `forklaunch score --help` before assuming which one you have — the default
changed from free-and-offline to paid-and-online, which is not a difference to
discover by accident. The card's own `caveat` field says which mode produced it;
read it before quoting a number at anyone.

## Reading a card

```json
{
  "schemaVersion": 2,
  "overall": 20,
  "headline": "acme: 27 deterministic finding(s) across 8 module(s) …",
  "caveat": "Deterministic checks only. …",
  "phase": "audit",
  "dimensions": {
    "compliance": {
      "score": 40,
      "items": [ {
        "label": "Sensitive fields are encrypted at rest",
        "criterion": "cmp-encryption-at-rest",
        "status": "unmet",
        "detail": "…",
        "checks": [ { "id": "encryptor-registration", "status": "fail", "detail": "…" } ]
      } ],
      "findings": [ { "severity": "high", "title": "…", "fix": "…", "source": "cli", "criterion": "cmp-encryption-at-rest" } ]
    },
    "observability": { "score": 0, "pending": true, "summary": "Not assessed. …" }
  }
}
```

Three fields carry the weight:

- **`items`** — the checklist. `met` / `unmet` / `pending`, one line per platform
  criterion (`pending` = a check asks for review, e.g. possible
  misclassification). This is what to show a human; it reads as work done, not
  just work missing.
- **`findings`** — the detail behind a score. Every finding carries a `fix`.
- **`source`** — `cli` means deterministic and reproducible; `ai` means a model
  judged it. Never present the two as equally certain.

### Severity

`critical` costs 30 points, `high` 15, `medium` 8, `low` 4, `info` nothing. A
rail floors at 0.

Two checks are `critical`:

- **`tenant-context-half-wired`**, promoted on evidence because its failure is
  silent: rows filter correctly, tests pass, and encrypted columns quietly use
  the wrong key until a real tenant exists in production. See `/compliance`.
- **`ai-provider-direct`**: a service whose entities hold `phi` imports an AI
  provider SDK (`openai`, `@anthropic-ai/sdk`, `@ai-sdk/*`, Bedrock, Gemini, …)
  or reads a provider key. Health data may be reaching a vendor with no BAA.
  Fix: call models through `createModelGatewayClient()` (the platform offers
  HIPAA products only BAA-covered models), or confirm the provider's BAA.

`managed-provider-credentials` (high) fires when a service written to run as a
managed instance (it reads `INSTANCE_HMAC_KEY` / `PLATFORM_GATEWAY_URL`, or has
the relay) reads Twilio, SendGrid or AI provider credentials. Hosted instances
never get them: use the platform's instance gateway for one-time codes and
`createModelGatewayClient()` for models.

On the website a failed wiring check also marks its report-card requirements
not met, overriding a "Guaranteed by ForkLaunch" pass: an unregistered
encryptor fails "sensitive fields are encrypted at rest" even though the
framework guarantees it for apps that wire it.

## Fixing what it finds

The fastest way to hand the card to an agent is the prompt:

```bash
forklaunch score --prompt --no-share   # full, agent-scored card
forklaunch score --offline --prompt    # deterministic checks only, free
```

Online, this is **the same prompt the website's "Copy prompt" button gives**
(the platform builds it once and returns it with the job): every failing
criterion and finding with its evidence and fix, deterministic check failures
marked "verified by analyzer", plus the rescoring loop. Offline, the CLI builds
an equivalent prompt from the deterministic card. `--json` carries the same text
as `remediationPrompt`.

The loop it tells the agent to run:

1. Fix findings in severity order, with a test for each.
2. After each batch, `forklaunch score --offline --json` — free and instant;
   confirm the finding is gone.
3. When the offline checks are clean, `forklaunch score --prompt --no-share`
   for the full assessment, and continue from the new prompt.
4. Never weaken a control or suppress a finding to raise the score.

By hand: work the `findings` in severity order and apply each `fix` verbatim —
they are specific instructions, not categories. Then re-run:

```bash
forklaunch score --offline
```

The score moves immediately, because these checks are deterministic. If a score
does **not** move after a fix you believe landed, the fix did not take effect —
check that before assuming the check is wrong.

For an iterative fix-until-threshold loop, `/score-self-heal` covers the same
motion against the full ForkLaunch Score.

## In the build loop

Run the fast, deterministic pass after **every** codegen pass — it is cheap and
catches drift immediately:

```bash
forklaunch score --offline
```

Run the full agent-scored analyze at milestones, not every pass. See
`/getting-started` for where each fits in the loop.

## A 100 is not necessarily clean

A freshly scaffolded app scores **100/100** while its own checklist shows an
unmet item and its headline reports findings:

```
Enterprise Readiness  100/100
linkjar: 7 deterministic finding(s) across 7 module(s)

Compliance  100/100  (1 item outstanding)
    + Sensitive fields are encrypted at rest
    ? Sensitive data is identified and handled according to its sensitivity   <- review, still 100
```

The arithmetic is right — `info` findings cost 0 points — but the number on its
own is misleading, and it is the number a non-technical reader will take away.

**Never report the score alone.** Report it with the unmet items:

> "It scores 100 on the areas the fast check can judge. One item is still
> open — no fields have been marked as sensitive yet, which matters as soon as
> we add anything personal."

The same applies to using it as a baseline: a 100 at scaffold time is a 100 on an
app with no data model, so a later drop is expected rather than alarming.

## Honest limits

- **Absence of findings is not proof of readiness.** Eight checks, over five
  checklist items on three rails. A clean card means those eight passed; the
  platform treats these passes as partial evidence, not proof.
- **A perfect rail can still have open items.** See above — check `items` for
  `unmet` and `pending`, not just `score`.
- **`frameworks` is empty from the CLI.** Deciding whether HIPAA or SOC 2
  applies to a domain is judgement; guessing would be worse than saying nothing.
- **The overall score is renormalised.** A 100 from `--offline` is "100 on the
  rails it can judge", not "100 across five".

Always quote the caveat alongside the number. A deterministic 100 presented as
a full readiness score is the one genuinely misleading thing you can do with
this command.

## Related

- `/compliance` — what the checks mean and how to fix them properly
- `/score-self-heal` — iterate until a threshold passes
- `/cli` — every other CLI command
- `/getting-started` — where this sits in the build loop
