---
name: plan-eng-review
version: 1.0.0
description: "Plan Phase 2: eng review — architecture, code quality, tests, performance. Auto-invoked by /plan."
allowed-tools:
  - Read
  - Grep
  - Glob
  - Bash
  - AskUserQuestion
---

# Plan Review Mode

Review this plan thoroughly before making any code changes. For every issue or recommendation, explain the concrete tradeoffs, give me an opinionated recommendation, and ask for my input before assuming a direction.

## Priority hierarchy
If you are running low on context or the user asks you to compress: Step 0 > Test diagram > Opinionated recommendations > Everything else. Never skip Step 0 or the test diagram.

## Engineering preferences (use these to guide your recommendations):
* DRY is important — flag repetition aggressively.
* Well-tested code is non-negotiable; I'd rather have too many tests than too few.
* I want code that's "engineered enough" — not under-engineered (fragile, hacky) and not over-engineered (premature abstraction, unnecessary complexity).
* I err on the side of handling more edge cases, not fewer; thoughtfulness > speed.
* Bias toward explicit over clever.
* Minimal diff: achieve the goal with the fewest new abstractions and files touched.

## ForkLaunch-specific rules (apply these throughout):
* Imports: `@forklaunch-platform/core` is the single source for schema primitives, handlers, routers, validators. NEVER import from `@forklaunch/validator/*` or `@forklaunch/express` directly.
* Schemas: natural object notation `{ name: string }` — NEVER `z.object()`.
* Enums: `const X = {} as const; export type X = ...` — NEVER TypeScript `enum`.
* Handlers: `handlers.get/post/put/patch/delete(schemaValidator, path, config, handler)`.
* Services: params object with `em: EntityManager`, return entities (NO mappers).
* DI tokens: must NOT shadow imported class names (`tokens.Orm` not `tokens.MikroORM`).
* Handler `name` field: no forward slashes (use PascalCase).
* MikroORM: ALWAYS use `strategy: 'select-in'` for 2+ OneToMany/ManyToMany populates. NEVER query inside loops — batch with `{ $in: ids }`.
* Tenant-encrypted PII/PHI/PCI: NEVER hydrate `UserEntity`, `InvitationEntity`, organization display records, or other encrypted rows from an unscoped EM just to discover ownership. First raw-select the unencrypted owner FK or use a lookup-hash column, then fork `EntityMgr` with `{ context: { tenantId } }` before hydration.
* HMAC: sign the route path (NOT full URL, NOT including query params). Body MUST be passed to `generateHmacAuthHeaders` for POST/PUT/PATCH/DELETE.
* Static routes BEFORE parameterized `/:id` routes (Express ordering).
* Controller exports in `api/controllers/index.ts` for SDK auto-generation.
* **Frontend placement:** Frontend code (React/Next.js) MUST live in `apps/` (top-level, sibling to `modules/`), NOT inside `modules/`. Each frontend app gets its own directory under `apps/` with its own `package.json`. Workspace config depends on runtime: for pnpm projects, add to `pnpm-workspace.yaml`; for bun projects, add to the root `package.json` `workspaces` array. If a plan introduces new frontend code inside `modules/`, flag it and recommend moving it to `apps/` with the appropriate workspace entry.

## Documentation and diagrams:
* I value ASCII art diagrams highly — for data flow, state machines, dependency graphs, processing pipelines, and decision trees. Use them liberally in plans.
* **Diagram maintenance is part of the change.** When modifying code that has ASCII diagrams in comments nearby, review whether those diagrams are still accurate. Update them as part of the same commit.

## BEFORE YOU START:

### Step 0: Scope Challenge
Before reviewing anything, answer these questions:
1. **What existing code already partially or fully solves each sub-problem?** Can we capture outputs from existing flows rather than building parallel ones?
2. **What is the minimum set of changes that achieves the stated goal?** Flag any work that could be deferred without blocking the core objective. Be ruthless about scope creep.
3. **Complexity check:** If the plan touches more than 8 files or introduces more than 2 new classes/services, treat that as a smell and challenge whether the same goal can be achieved with fewer moving parts.

Then ask if I want one of three options:
1. **SCOPE REDUCTION:** The plan is overbuilt. Propose a minimal version that achieves the core goal, then review that.
2. **BIG CHANGE:** Work through interactively, one section at a time (Architecture > Code Quality > Tests > Performance) with at most 8 top issues per section.
3. **SMALL CHANGE:** Compressed review — Step 0 + one combined pass covering all 4 sections. For each section, pick the single most important issue. Present as a single numbered list with lettered options + mandatory test diagram + completion summary. One AskUserQuestion round at the end. For each issue in the batch, state your recommendation and explain WHY, with lettered options.

**Critical: If I do not select SCOPE REDUCTION, respect that decision fully.** Your job becomes making the plan I chose succeed, not continuing to lobby for a smaller plan. Raise scope concerns once in Step 0 — after that, commit to my chosen scope and optimize within it.

## Review Sections (after scope is agreed)

### 1. Architecture review
Evaluate:
* Overall system design and component boundaries.
* Dependency graph and coupling concerns (especially cross-module DI).
* Data flow patterns — handler > service > entity > flush — and potential bottlenecks.
* MikroORM patterns: N+1 risks, cartesian products from joined strategy, missing `fields` projections.
* HMAC signing correctness: path matching, body inclusion, query param exclusion.
* Pulumi/infrastructure implications if touching deployment worker.
* Scaling characteristics and single points of failure.
* Security architecture (auth, RBAC, multi-tenancy via `req.session.organizationId`).
* For each new codepath or integration point, describe one realistic production failure scenario and whether the plan accounts for it.

**STOP.** For each issue found in this section, call AskUserQuestion individually. One issue per call. Present options, state your recommendation, explain WHY. Do NOT batch multiple issues into one AskUserQuestion. Only proceed to the next section after ALL issues in this section are resolved.

### 2. Code quality review
Evaluate:
* Code organization — does it follow the 7-layer import hierarchy? Does new code fit existing patterns?
* DRY violations — be aggressive here. Reference existing file and line.
* Error handling patterns and missing edge cases (call these out explicitly).
* Schema design — natural object notation, correct use of `optional()`, `array()`, `union()`, `enum_()`.
* Handler config — correct `name` (PascalCase, no slashes), proper auth config, response schemas.
* Service patterns — EntityManager usage, flush calls, transaction boundaries.
* Areas that are over-engineered or under-engineered.
* Existing ASCII diagrams in touched files — are they still accurate after this change?

**STOP.** AskUserQuestion per issue. Recommend + WHY. Do NOT batch. Do NOT proceed until resolved.

### 3. Test review
Make a diagram of all new UX flows, new data flows, new codepaths, and new branching conditions. For each new item in the diagram, ensure there is a test.

For each new feature, answer:
* What's the test that would make you confident shipping at 2am on a Friday?
* What's the test a hostile QA engineer would write to break this?

Check test pyramid: many unit, fewer integration, few E2E? Flakiness risks?

**STOP.** AskUserQuestion per issue. Recommend + WHY. Do NOT batch.

### 4. Performance review
Evaluate:
* N+1 queries. For every new MikroORM `find`/`findOne`: is there a `populate` with `strategy: 'select-in'`? Are `fields` limited when only IDs needed?
* Tenant-encrypted reads. For every new `findOne` on an entity with PII/PHI/PCI fields: is the EM already tenant-scoped? If the tenant is unknown, does the code use raw FK lookup or lookup hashes before hydration?
* Batch operations. Any queries inside loops? Should use `{ $in: ids }` + Map lookup.
* Memory usage. For every new data structure: what's the maximum size in production?
* Caching opportunities (Redis via `billingCacheService` pattern).
* Background job sizing — worker queue payload, runtime, retry behavior.
* Connection pool pressure — DB, Redis, HTTP connections to external services.

**STOP.** AskUserQuestion per issue. Recommend + WHY. Do NOT batch.

## CRITICAL RULE — How to ask questions
Every AskUserQuestion MUST: (1) present 2-3 concrete lettered options, (2) state which option you recommend FIRST, (3) explain in 1-2 sentences WHY that option over the others, mapping to engineering preferences. No batching multiple issues into one question. No yes/no questions. Open-ended questions are allowed ONLY when you have genuine ambiguity about developer intent or architecture direction — and you must explain what specifically is ambiguous. **Exception:** SMALL CHANGE mode intentionally batches one issue per section into a single AskUserQuestion at the end — but each issue in that batch still requires its own recommendation + WHY + lettered options.

## For each issue you find
* **One issue = one AskUserQuestion call.** Never combine multiple issues.
* Describe the problem concretely, with file and line references.
* Present 2-3 options, including "do nothing" where reasonable.
* **Lead with your recommendation.** State it as a directive: "Do B. Here's why:" — not "Option B might be worth considering."
* **Map the reasoning to engineering preferences above.** One sentence connecting your recommendation to a specific preference.
* **Escape hatch:** If a section has no issues, say so and move on. If an issue has an obvious fix with no real alternatives, state what you'll do and move on.

## Required outputs

### "NOT in scope" section
List work considered and explicitly deferred, with one-line rationale each.

### "What already exists" section
List existing code/flows that partially solve sub-problems and whether the plan reuses them.

### Failure modes
For each new codepath, list one realistic way it could fail in production (timeout, nil reference, race condition, stale data, etc.) and whether:
1. A test covers that failure
2. Error handling exists for it
3. The user would see a clear error or a silent failure

If any failure mode has no test AND no error handling AND would be silent, flag it as a **critical gap**.

### Completion summary
```
- Step 0: Scope Challenge (user chose: ___)
- Architecture Review: ___ issues found
- Code Quality Review: ___ issues found
- Test Review: diagram produced, ___ gaps identified
- Performance Review: ___ issues found
- NOT in scope: written
- What already exists: written
- Failure modes: ___ critical gaps flagged
```

## Formatting rules
* NUMBER issues (1, 2, 3...) and give LETTERS for options (A, B, C...).
* When using AskUserQuestion, label each option with issue NUMBER and option LETTER (e.g., "3A", "3B").
* Recommended option is always listed first.
* Keep each option to one sentence max. I should be able to pick in under 5 seconds.
* After each review section, pause and ask for feedback before moving on.

## Unresolved decisions
If the user does not respond to an AskUserQuestion or interrupts to move on, note which decisions were left unresolved. At the end of the review, list these as "Unresolved decisions that may bite you later" — never silently default to an option.
