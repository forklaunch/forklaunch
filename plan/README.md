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
  - **Decision flipped mid-plan (§1):** originally an app-level service outside the CLI's module system; now built as a first-class `Module::BaseCac` (`cac-base`) — named `cac` per the founder, the industry term for Computer-Assisted Coding — the same way `billing-base`/`iam-base`/`messaging-base` are, after confirming the pattern is proven (PR #264 added `messaging` the same way, 346 tests passing) and blocker-free (RBAC merged, no structural gaps — one cautionary precedent noted: don't leave it half-wired like `ecommerce-stripe` was)
  - `framework/core`/`express`/`common` need zero changes either way; the CLI (`cli/`) plus three new `blueprint/` packages are the entire lift (§3 has the concrete file checklist)
  - Launches on free code sets (ICD-10, HCPCS); the paid AMA CPT license itself stays deferred until a hospital confirms as a paying client — but per founder direction, `CptCodeProvider`'s engineering is built to full readiness in Phase 2, in parallel with the mock-code work, so enabling CPT later is a flag flip against finished code, not new engineering
  - CPT license gating modeled as a feature flag reusing the existing `hasFeatureChecks`/`surfaceFeatures` guard, so no framework changes are needed
  - Phased delivery plan (Phase 0-6) and AMA licensing timeline (~4-6 weeks, trigger-based)
  - Claim scrubbing engine corrected to three distinct rule layers (NCCI PTP, NCCI MUE, LCD/NCD medical necessity) after domain research found the source doc's original single-check design conflated unrelated CMS mechanisms
  - Code-set refresh pipeline, EDI transaction set list, clearinghouse choice (Stedi), and success-metric targets benchmarked against MGMA/HFMA industry data — see §15 Sources
  - Executive summary up top; explicit open questions flag the LCD/CPT licensing coupling and unvalidated phase-timeline estimates rather than leaving them implicit
  - §14 breaks delivery into one PR per phase (six PRs total), flagging Phase 2's PR as by far the largest since it carries the claim engine, all three scrubbing layers, and the full CPT-readiness build
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

