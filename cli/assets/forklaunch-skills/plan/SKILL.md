---
name: plan
version: 1.0.0
description: "Plan pipeline: 4-phase (CEO review → eng review → diagrams → plan doc). Triggers on plan/design/architect/RFC."
allowed-tools:
  - Read
  - Grep
  - Glob
  - Bash
  - AskUserQuestion
---

# Master Plan Orchestrator

You are running a 4-phase plan review pipeline. Execute each phase in order. Do NOT skip phases. Carry forward all decisions, diagrams, and outputs from prior phases into subsequent ones.

## Phase Overview
```
  PHASE 1              PHASE 2              PHASE 3              PHASE 4
  CEO Review    --->   Eng Review    --->   Diagrams      --->   Final Plan
  (vision,             (architecture,       (ASCII art,          (complete,
   scope,               code quality,        data flows,          implementable
   ambition)            tests, perf)         state machines)      plan document)
```

---

## Phase 1: CEO / Founder Review

Read and follow the full instructions in `~/.claude/skills/plan-ceo-review/SKILL.md`.

Execute the CEO review completely — system audit, Step 0 (scope challenge + mode selection), and all 10 review sections. Resolve all AskUserQuestion rounds before proceeding.

**When Phase 1 is complete**, produce a Phase 1 Summary:
```
PHASE 1 COMPLETE — CEO Review
  Mode selected: ___
  Key scope decisions: [list]
  Deferred items: [list]
  Dream state delta: [summary]
  Unresolved decisions: [list or "none"]
```

Confirm with the user: **"Phase 1 (CEO Review) is complete. Proceeding to Phase 2 (Eng Review). Any adjustments before I continue?"**

---

## Phase 2: Engineering Review

Read and follow the full instructions in `~/.claude/skills/plan-eng-review/SKILL.md`.

**Important:** Carry forward the scope mode and all decisions from Phase 1. Do NOT re-challenge scope — that was settled in Phase 1. If Phase 1 selected SCOPE EXPANSION, the eng review optimizes within that expanded scope. If REDUCTION, optimize within the reduced scope.

Skip Step 0 (scope challenge) in the eng review — jump directly to the review sections (Architecture > Code Quality > Tests > Performance). The eng review's job is to make the CEO-approved plan bulletproof.

Execute all review sections. Resolve all AskUserQuestion rounds before proceeding.

**When Phase 2 is complete**, produce a Phase 2 Summary:
```
PHASE 2 COMPLETE — Eng Review
  Architecture issues: ___ found, ___ resolved
  Code quality issues: ___ found, ___ resolved
  Test gaps: ___ identified
  Performance issues: ___ found, ___ resolved
  Unresolved decisions: [list or "none"]
```

Confirm with the user: **"Phase 2 (Eng Review) is complete. Proceeding to Phase 3 (Diagrams). Any adjustments before I continue?"**

---

## Phase 3: Diagrams

Produce comprehensive ASCII art diagrams for the final plan. These diagrams are deliverables — they will be included in the final plan document and may be embedded in code comments.

### Required diagrams (produce ALL that apply):

1. **System Architecture** — Component boundaries, module dependencies, external services.
2. **Data Flow** — Every new data flow with all 4 paths (happy, null/undefined, empty, error). Show handler → service → entity → flush.
3. **State Machine** — For every new stateful object (deployments, jobs, provisioning steps). Include impossible/invalid transitions.
4. **Dependency Graph** — Cross-module DI, HMAC call graph, worker queue dispatch.
5. **Error Flow** — From Section 2 error map: where errors originate, where they're caught, what the user sees.
6. **Deployment Sequence** — Migration order, service deploy order, rollback steps.
7. **API Contract** — New endpoints with request/response shapes, auth requirements.
8. **Frontend Component Tree** — New pages/components, data fetching, state management. Note: frontend lives in `apps/`, NOT `modules/`.

### Diagram quality rules:
- Use box-drawing characters for clean diagrams (`+--+`, `|`, `-->`, `==>`)
- Label every arrow with what flows along it
- Mark failure points with `[!]` or `<!>`
- Include legends for non-obvious symbols
- Each diagram must be self-contained — understandable without reading the full plan

**When Phase 3 is complete**, present all diagrams and confirm: **"Phase 3 (Diagrams) is complete. Proceeding to Phase 4 (Final Plan). Any diagram corrections before I continue?"**

---

## Phase 4: Final Plan Document

Synthesize everything from Phases 1-3 into a single, complete, implementable plan document. This is the artifact the implementer will follow.

### Structure:

```markdown
# Plan: [Feature/Change Name]

## Overview
[1-2 paragraph summary: what, why, scope mode chosen]

## Scope Decisions (from Phase 1)
[Key decisions made during CEO review, with rationale]

## Architecture
[System architecture diagram from Phase 3]
[Component boundaries, new modules/services, DI wiring]

## Implementation Steps
[Ordered, numbered steps. Each step should be a single PR or logical unit of work.]
[For each step:]
  1. What files to create/modify
  2. What the code should do (specific enough to implement, not pseudocode)
  3. Dependencies on prior steps
  4. Test requirements for this step

## Data Model Changes
[New entities, modified entities, migrations needed]
[Data flow diagram from Phase 3]

## API Changes
[New endpoints, modified endpoints, HMAC signing details]
[API contract diagram from Phase 3]

## Frontend Changes
[New pages/components in `apps/`]
[Workspace setup: pnpm-workspace.yaml or package.json workspaces depending on runtime]
[Component tree diagram from Phase 3]

## Error Handling
[Error registry from Phase 2]
[Error flow diagram from Phase 3]

## Test Plan
[Test diagram from Phase 2]
[For each new codepath: what type of test, happy path, failure path, edge cases]

## Performance Considerations
[From Phase 2 performance review]
[N+1 risks, batch operations, caching, indexes]

## Deployment Plan
[Migration order, deploy sequence, rollback steps]
[Deployment sequence diagram from Phase 3]

## Observability
[New logs, metrics, traces, alerts]

## NOT in Scope
[Deferred items with rationale, from Phases 1 and 2]

## Failure Modes Registry
[Complete table from Phase 2, cross-referenced with tests]

## Unresolved Decisions
[Any decisions left open across all phases]
```

### Final plan rules:
- Every implementation step must reference specific files by path
- No vague instructions — "add error handling" is not acceptable; "catch `UniqueConstraintViolation` in `DeploymentService#create`, return 409 with message 'Deployment already exists'" is
- Include all diagrams from Phase 3 inline where they're most relevant
- Cross-reference test requirements with the test plan section
- The plan must be implementable by someone who did NOT attend the review

**When Phase 4 is complete**, present the final plan and the master completion summary:

```
+=======================================================================+
|                    PLAN PIPELINE — COMPLETE                            |
+=======================================================================+
| Phase 1 (CEO Review)    | Mode: ___, ___ scope decisions              |
| Phase 2 (Eng Review)    | ___ arch, ___ quality, ___ test, ___ perf   |
| Phase 3 (Diagrams)      | ___ diagrams produced                       |
| Phase 4 (Final Plan)    | ___ implementation steps, ___ files touched  |
+-------------------------+---------------------------------------------+
| Total issues resolved   | ___                                          |
| Unresolved decisions    | ___                                          |
| Critical gaps remaining | ___                                          |
+=======================================================================+
```

