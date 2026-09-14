---
name: score-self-heal
description: "Run the ForkLaunch Score during development and self-heal: fix the lowest-scoring criteria from their fixes, re-score, repeat until the threshold passes."
user-invokable: true
---

# ForkLaunch Score — self-heal loop

Score the service you are building across the five readiness pillars (Compliance, Security,
Governance, Scalability, Observability), then **fix what the card
says is broken** and re-score until it passes. Every checklist criterion carries its own
`score` (/100), `detail` (evidence citing files), and `fix` (a concrete remediation) — the card
is a work queue, not just a grade.

## Run a score

Preferred — the CLI (renders the card; `--json` for machine reading):

```bash
forklaunch score --offline --path . --json > /tmp/score.json
forklaunch score --offline --path . --min-score 70   # exit 1 = does not pass
```

Fallback — the analyzer endpoint directly (studio/dev harness where the orchestrator can read
the workspace; requires a signed-in platform token):

```bash
curl -s -X POST "$FORKLAUNCH_STUDIO_API_URL/studio-orchestrator/analysis/repo" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d "{\"appId\":\"$(basename $PWD)\",\"workspaceRoot\":\"$PWD\"}" > /tmp/score.json
```

Scoring takes ~1–2 minutes. The result's `reportCard.dimensions.<rail>.items[]` is the queue.

## The self-heal loop

1. **Score.** Save the JSON. Note `overall` and each rail's score.
2. **Check construction first.** If the card's `caveat` mentions a failed construction check, or
   Governance carries a critical `Construction` finding — **stop and fix that before anything
   else** (`forklaunch depcheck` shows the conflicts). If it doesn't build, it doesn't pass, and
   other scores are noise until it does.
3. **Pick targets.** Collect items where `status != "met"` and `owner == "agent"`, sorted by
   ascending `score` weighted by the rail's weight (compliance/security 0.25 each;
   observability 0.2; governance/scalability 0.15). Skip
   `owner == "user"` items (agreements, published policies — record them in USER-ACTIONS.md
   instead of trying to code them).
4. **Apply the `fix`.** Each item's `fix` names the concrete mechanism (e.g. "add pgcrypto
   column encryption to users.email in a migration"). Implement it the ForkLaunch way first —
   check `/quick-reference` and `/compliance`: most fixes are a compliance tag
   (`.compliance('pii')`), an `fp` property, a framework primitive, or a config line — not
   hand-rolled infrastructure. The item's `detail` cites where the gap is.
5. **Fix in batches, then re-score once.** Scoring is expensive — remediate 3–8 items per
   iteration, run typecheck/tests, then re-score. Never re-score after every single edit.
6. **Verify movement.** Each targeted item's score should rise and nothing else should regress
   by more than noise (±5). A regression means a fix broke something — inspect that rail's
   `detail`s before continuing.
7. **Stop when** `overall >= threshold` (default 70 unless the user set one), or every remaining
   `unmet` item is `owner: "user"`, or two consecutive iterations moved `overall` by < 3 points
   (report the residuals instead of thrashing).

## Rules

- Never "fix the score" — fix the code. Do not delete checklist-triggering code, suppress
  findings, or game labels; the deterministic CLI findings (`source: "cli"`) re-detect on every
  run and floor the rail regardless of what the LLM sees.
- `pending` items are unverifiable from code (org practices). Don't churn on them: surface them
  to the user once, move on.
- Keep a one-line log per iteration: `iter N: overall 43 → 51; fixed enc-at-rest(15→70),
  audit-log(5→60); construction ✓`.
- PR flows get this automatically (the ForkLaunch Score PR comment); this skill is for the
  inner dev loop *before* the PR, so the comment lands green.
