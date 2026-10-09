# Internal Planning Documentation

This directory contains internal planning documents, implementation specs, and technical design decisions for the ForkLaunch project.

## 🔥 Latest Updates (2024-10-17)

### 🎉 ✅ OpenAPI Export WORKING! (Without Docker!)

**SUCCESS** 👉 **[OPENAPI-EXPORT-SUCCESS.md](OPENAPI-EXPORT-SUCCESS.md)** - We did it!
- ✅ **6 services exported successfully** on platform-management
- ✅ **No Docker needed!** ConfigInjector returns `{}` for all dependencies
- ✅ **No database needed!** Dummy env vars set directly in Command
- ✅ **Works in CI!** Fast, reliable, no external deps
- **Uses**: `tsx --tsconfig tsconfig.json` (critical for decorators!)

### 🎉 Major Simplification! (Framework is Smarter!)

**CONTEXT** 👉 **[EXECUTIVE-SUMMARY.md](EXECUTIVE-SUMMARY.md)** - 5 minute read
- Framework handles version detection automatically!
- Returns all versions in one call: `{ "v1": {...}, "v2": {...} }`
- No complex logic needed in CLI
- **2-3 days instead of 3-4**
- **MUCH SIMPLER** than originally thought!

**Technical Details** 👉 **[FINAL-ARCHITECTURE-CORRECTIONS.md](FINAL-ARCHITECTURE-CORRECTIONS.md)**
- Use package manager commands directly (pnpm tsx, bun)
- Set DOTENV_FILE_PATH=.env.local for env vars
- Set MIKRO_ORM_SKIP_DB_CONNECTION=true (no Docker needed!)
- Complete implementation examples

### 🎯 Architecture Clarifications
- **[ARCHITECTURE-CLARIFICATIONS-SUMMARY.md](ARCHITECTURE-CLARIFICATIONS-SUMMARY.md)** - 📋 Initial analysis
  - Single Dockerfile (monorepo pattern)
  - Multi-version OpenAPI (versioned APIs)
  - (Now superseded by FINAL-ARCHITECTURE-CORRECTIONS.md)

### CLI/Platform Alignment Analysis
- **[ANALYSIS-SUMMARY.md](ANALYSIS-SUMMARY.md)** - ⭐ **Executive summary** of CLI alignment
  - Current state: 77% aligned with APPLICATION_ONBOARDING_INPUTS.md
  - 3 days work to reach 100% alignment
  - Detailed scorecard and recommendations

- **[IMMEDIATE-ACTION-ITEMS.md](IMMEDIATE-ACTION-ITEMS.md)** - 🚀 **Implementation guide**
  - Updated with correct architecture
  - Ready-to-use code snippets
  - Day-by-day plan (3 days)
  - Testing instructions

- **[UPDATED-IMPLEMENTATION-NOTES.md](UPDATED-IMPLEMENTATION-NOTES.md)** - 📝 **Technical details**
  - Framework changes needed
  - Platform API updates
  - Complete implementation examples

- **[CLI-ALIGNMENT-WITH-ONBOARDING-SPEC.md](CLI-ALIGNMENT-WITH-ONBOARDING-SPEC.md)** - 📚 **Deep dive**
  - Comprehensive gap analysis
  - Detailed comparison with spec
  - Proposed solutions with code examples
  - Platform requirements

## Contents

### Medical Coding / Billing Platform (CAC module — see [plan/cac/](cac/))
- **[cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md](cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md)** - Free-first implementation plan for a HIPAA-compliant medical coding/billing module
  - **Decision flipped mid-plan (§1):** originally an app-level service outside the CLI's module system; now built as a first-class `Module::BaseCac` (`cac-base`) — named `cac` per the founder, the industry term for Computer-Assisted Coding — the same way `billing-base`/`iam-base`/`messaging-base` are, after confirming the pattern is proven (PR #264 added `messaging` the same way, 346 tests passing) and blocker-free (RBAC merged, no structural gaps — one cautionary precedent noted: don't leave it half-wired like `ecommerce-stripe` was). **PR 1 is merged.**
  - `framework/core`/`express`/`common` need zero changes either way; the CLI (`cli/`) plus three new `blueprint/` packages are the entire lift (§3 has the concrete file checklist)
  - **Business model resolved by the founder (§1, §2):** ForkLaunch never applies for, holds, or pays for a CPT license — `cac-base` is a reusable toolkit other companies use to build and self-host their own product under their own CPT license, the same way `iam-base`/`billing-base` work. Because adopting customers already hold their own license by the time they need it, the real-CPT `CodeSetProvider` extension point is built to genuine readiness now, not deferred behind any ForkLaunch-owned trigger (§5)
  - Launches on free code sets (ICD-10, HCPCS) fully built and usable with zero license; the reference `CptCodeProvider` adapter and downstream-builder documentation ship alongside it, intentionally containing no real AMA content since ForkLaunch never holds that license
  - **Real-CPT feature gating — implemented, not via the framework's `hasFeatureChecks`/`surfaceFeatures` guard as originally sketched.** That mechanism blocks a route entirely when a feature is missing, which is wrong here since real CPT is never a hard requirement for these endpoints to work. Built instead as `CodeSetProviderResolver`: a plain per-request lookup against the organization's own `CodeSetLicense` that hands back mock or real CPT, failing closed to mock on any error
  - Phased delivery plan (Phase 0-5) — AMA licensing timeline/pricing research is retained as reference material for downstream builders, not ForkLaunch's own roadmap (§2)
  - Claim scrubbing engine corrected to three distinct rule layers (NCCI PTP, NCCI MUE, LCD/NCD medical necessity) after domain research found the source doc's original single-check design conflated unrelated CMS mechanisms
  - Code-set refresh pipeline and success-metric targets benchmarked against MGMA/HFMA industry data (§15 Sources); the EDI transaction set / clearinghouse (Stedi) research is retained as reference material only — the founder confirmed (2026-08-31) `cac-base` never submits claims or handles remittances itself (§8)
  - **New (§10):** a lightweight validation UI is planned in the separate `forklaunch-platform` repo, scoped to free code sets only (no CPT/no license needed) — not yet started
  - Executive summary up top; explicit open questions flag the LCD/CPT coupling, the unvalidated phase-timeline estimates, and the not-yet-scoped `forklaunch-platform` UI rather than leaving them implicit
  - §14 originally broke delivery into one PR per phase (six PRs total); **PR 4 (eligibility & remittance) is now removed entirely**, not deferred — the founder confirmed (2026-08-31) `cac-base` never submits claims or handles remittances on an adopter's behalf, it stops at the scrubbing report. PR 3 is the largest remaining engineering PR, carrying the claim engine, all three scrubbing layers, and the full real-CPT extension-point build
  - **PR 2 is implemented:** all §4 entities (+ `Icd10Code`/`HcpcsCode` reference tables), a hand-written migration, the `CodeSetLoaderService` ETL shape with ICD-10-CM/HCPCS loaders and unit tests, `refresh-code-sets.ts`, and free-code-set validation endpoints
  - **PR 3 is implemented (mock codes):** `ClaimService` builds a claim from an encounter's charges/diagnoses; `ScrubbingService` runs all three scrubbing layers (NCCI PTP, NCCI MUE, LCD/NCD) and persists `Denial` rows — this **is** the report the founder's scope call describes, with nothing further to build toward submission. The real-CPT extension point is also built — `CptCodeProvider` + `EntityManagerCptCodeSource` + an org-scoped `CptCode` reference table + `CPT_SOURCE_PATH` wiring into `refresh-code-sets.ts` — plus a downstream-builder README and a synthetic CPT-shaped fixture proving the scrubbing engine is format-agnostic
  - **PR 5 is implemented:** an analytics API (`GET /analytics/claims/summary` — clean-claim-rate, denial-rate; average-days-to-payment dropped, it needs remittance timing that's now out of scope) and a real RBAC fix. `claim`/`denial` routes had shipped as internal/HMAC-only (no user identity) despite the plan's own intent that they be coder/biller-facing — retrofitted to protected/JWT with real permissions, which in turn required replacing `server.ts`'s hardcoded `surfacePermissions`/`surfaceRoles` placeholder (always the same access regardless of who asked) with a real cross-service call to IAM
  - **The per-organization runtime feature gate is implemented:** `CodeSetProviderResolver` resolves mock vs. real CPT per request from the organization's own `CodeSetLicense`, replacing the old always-mock `CodeSetProvider`. Required retrofitting `codeSet.controller.ts` to protected/JWT too, for the same reason as PR 5's other routes — resolving per-organization needs a real caller identity. A verification pass on this work (prompted by an automated review flagging zero test coverage on the resolver) surfaced a real, previously-undetected bug: `@forklaunch/implementation-cac-base`'s `./services` package export has been broken at runtime since PR 1 (fixed)
  - **The denial worklist API is implemented:** list/view/resolve the `Denial` rows the scrubbing engine creates — the one piece of the removed PR 4 that never depended on Stedi, salvaged and built separately from any phase number
  - **IAM permission/role caching is implemented (§12 item 8, half-resolved):** `surfacing.ts`'s `createSurfacePermissions`/`createSurfaceRoles` now check a Redis-backed `AuthCacheService` (5min TTL) before calling IAM, so repeat requests from the same user skip the round-trip — `REDIS_URL`/`TtlCache` wired into `registrations.ts` following `billing-base`'s existing pattern. Scrubbing-rule caching (the other half of that item) is deliberately not built: the rule checks are pure in-memory lookups against mock tables today, with no I/O to cache — revisit once the real-CPT rule source replaces the mocks
  - **PR 6 is implemented:** fixed a real staleness bug in the downstream-builder README — it still described the per-organization CPT switch as unwired and pointed at the wrong framework mechanism (`hasFeatureChecks`), both true before the feature gate landed but wrong since. Rewrote it to match `CodeSetProviderResolver`'s actual behavior, plus added the test coverage `EntityManagerCptCodeSource` (the concrete adapter the README holds up as the starting point) was missing
  - All of this work lives in one still-open PR (#295), by explicit request, rather than split across separate PRs per phase — easier for a single reviewer to check in one pass
- **[cac/cac_objective.md](cac/cac_objective.md)** - One-page plain-language summary of what the `cac-base` module does and how it works, for anyone who wants the shape of it without reading the full plan

### Release & Deploy Implementation
- **[release-deploy-implementation.md](release-deploy-implementation.md)** - Implementation plan for release and deploy CLI commands
  - 4 new CLI commands: `integrate`, `openapi export`, `release create`, `deploy create`
  - Free tier defaults strategy
  - 3-4 week timeline
  - Platform already handles: env vars, secrets, observability, deployment tracking

- **[IMPLEMENTATION-STATUS.md](IMPLEMENTATION-STATUS.md)** - ✅ **Current progress tracker**
  - Phase 0: `integrate` command ✅ **COMPLETE**
  - Phase 1: `openapi export` command ✅ **COMPLETE**
  - Phase 2: `release create` command ✅ **COMPLETE**
  - Phase 3: `deploy create` command ✅ **COMPLETE**
  - Testing instructions and next steps

- **[FINAL-SUMMARY.md](FINAL-SUMMARY.md)** - ✅ **Release & Deploy Commands COMPLETE**
  - All 4 commands implemented
  - Code refactored for quality
  - Comprehensive documentation
  - Ready for platform integration

- **[example-manifest-with-platform.toml](example-manifest-with-platform.toml)** - Example manifest with platform integration

## Quick Summary

### What's Being Built
Release and deployment workflow with **free tier first** philosophy:

```bash
# 1. Link to platform
forklaunch integrate --create

# 2. Create release
forklaunch release create --version 1.0.0

# 3. Set env vars in Platform UI

# 4. Deploy (free tier: $0/month)
forklaunch deploy create --release 1.0.0 --environment production
```

### Key Principles
- ✅ Free tier defaults (db.t3.micro, 256m CPU, 512Mi RAM)
- ✅ Platform provides intelligent defaults
- ✅ Secrets managed via Platform UI (never in CLI/git)
- ✅ OTEL auto-configured by platform
- ✅ Framework already supports OpenAPI export

### Timeline
- Week 1: Integrate + OpenAPI export commands
- Week 2: Release create command
- Week 3: Deploy create command
- Week 4-5: Platform enhancements (parallel)

## Purpose

These documents are for:
- Internal team planning and coordination
- Technical implementation details
- Architecture decisions and tradeoffs
- Development timelines and task breakdown

## Not For

- User-facing documentation (see `/docs` instead)
- API references (see `/docs` instead)
- Getting started guides (see `/docs` instead)

## Contribution

When adding new planning documents:
1. Use clear, descriptive filenames
2. Include dates or versions in the filename if time-sensitive
3. Update this README with a link and description
4. Keep plans actionable and implementation-focused

