# Medical Coding / Billing Platform — Implementation Plan

**Status:** Draft for review
**Owner:** Engineering
**Based on:** `implementation_plan_free_first.docx` (v2.0, Free-First Strategy) and `ama_cpt_license_timeline.pdf`, reconciled against the current ForkLaunch codebase, deepened with a second codebase pass and external research into medical-coding domain standards (CMS, AMA, X12, MGMA/HFMA — see §15 Sources), and revised after a live precedent check against the framework's own recently-added module families (messaging, ecommerce — see §1).

---

## Executive summary

- **What:** a HIPAA-compliant medical coding/billing module — `Module::BaseCac` (`cac-base`) — built as a first-class ForkLaunch module the same way `billing-base`/`iam-base`/`messaging-base` are, consumable via `forklaunch init module -m cac-base` by any ForkLaunch application. `framework/core`/`express`/`common` need zero changes; the CLI (`cli/`) and three new `blueprint/` packages are the entire lift (§1, §3).
- **Free-first, but CPT is engineered to be ready, not deferred as an afterthought:** launch and demo on ICD-10-CM, HCPCS, NCCI, and LCD/NCD data (all free/government-published) using mock procedure codes. Licensing CPT (the paid contract with AMA) still only happens once a hospital confirms as a paying client (§2) — but `CptCodeProvider`, the real-CPT code path, is built and structurally proven well *before* that trigger, in parallel with the mock-code work, not written for the first time once a client shows up (§5, §10). Per the founder: CPT is the most valuable standard, so whenever we're ready to enable it, we should already be in good shape — the code should be the easy part, not the bottleneck.
- **The mechanism that makes this safe:** a `CodeSetProvider` interface, gated per-organization by the framework's existing feature-flag guard — flipping a hospital from mock to real CPT is a config/data change against already-built code, not new engineering, and historical claims are never retroactively recoded (§5).
- **What research corrected:** the source doc conflated NCCI code-conflict rules with diagnosis-procedure medical necessity. These are two separate CMS mechanisms with different data, cadences, and denial codes — the scrubbing engine now has three distinct rule layers instead of one (§6).
- **What's still open, not just deferred:** the Phase 0–6 week estimates are inherited from the source doc and unvalidated against actual team capacity (§10); real LCD/NCD data can't be meaningfully integrated until CPT is licensed even though LCD data itself is free, because LCD policies are written in terms of real CPT codes (§6, §12); and the module-level CLI wiring itself is new work that didn't exist when the phase estimates were first drafted (§14).
- **How it ships:** one PR per phase, six PRs total — see §14 for the breakdown. PR 1 now also carries the CLI module registration and blueprint package skeleton, so it's larger than originally scoped; PR 3 (Phase 2) remains the largest overall.
- **Where to start:** §13 Immediate next steps.

---

## 1. Decision: how this gets built

Two ways to build this were considered — and the decision **flipped once** during planning, after new evidence changed the risk calculus.

| Option | What it means | Verdict |
|---|---|---|
| A. App-level service | A new service inside a separate consumer application, built with `forklaunch init service`, following the same architectural pattern as `blueprint/billing-base`/`blueprint/iam-base` but **not** registered as a reusable module in the CLI. | Superseded — see below |
| **B. First-class framework module** | Add `Module::BaseCac` (`cac-base`) to the Rust CLI, plus new `blueprint/interfaces/cac` and `blueprint/implementations/cac/base` packages, so any ForkLaunch user can run `forklaunch init module -m cac-base`. | **Chosen** |

**Naming: `cac`, not `medical-coding`.** Confirmed directly with the founder (Rohin) — "CAC" is the recognized health-information-management industry term (Computer-Assisted Coding), used consistently instead of the more generic `medical-coding` name this plan used in earlier drafts. Worth a quick gut-check before Phase 2 (§12, new item): "Computer-Assisted Coding" specifically denotes NLP/AI-suggested codes from clinical documentation industry-wide — this plan's scrubbing engine (§6) validates codes a human already entered, it doesn't suggest codes from text. If the name is meant as a forward-looking product category rather than a description of what Phase 0–4 actually builds, that's fine — just confirm that's the intent so the name doesn't quietly set an NLP-suggestion expectation nobody's built yet.

**Why it flipped.** Option A was originally chosen because Option B looked like a much larger, riskier lift — the CLI's `Module` enum was, at the time, only exercised by `billing`/`iam`, and touching ~12 exhaustively-matched Rust files for a brand-new module family was unproven territory. That assessment no longer holds. Since then, the framework itself added **two more first-class module families**: `messaging` (`messaging-base`/`messaging-twilio`, PR #264) and `ecommerce` (`ecommerce-stripe`). The messaging PR did this exact thing — "following the billing-base/billing-stripe pattern end to end" — with 346 Rust tests passing and the scaffold smoke-verified.

**Pre-implementation blocker check (done before committing to this rewrite):**
- **No structural blocker.** Every CLI match site the original research flagged — the `Module` enum, `validate_modules`, `get_service_module_name`/`get_service_module_cache`, `get_routers_from_standard_package`, package.json version constants, manifest boolean flags — is a clean, mechanical exhaustive-`match` extension, confirmed live in `cli/src/constants.rs` and `cli/src/core/modules.rs`, and now exercised twice more since the original research.
- **RBAC dependency is merged.** `hasPermissionChecks` (`framework/core/src/http/guards/hasPermissionChecks.ts`) is on `main` and wired into `auth.middleware.ts` — the RBAC design in §3 has what it needs.
- **One cautionary precedent, not a blocker, but a checklist item.** `ecommerce-stripe` was registered in the `Module` enum without finishing the rest of the pipeline — no template farm, no `ProjectDependencies` fields, no version constants, no `is_ecommerce` manifest flag. It's explicitly allowlisted as a *known, intentional gap* in a regression test (`every_module_variant_has_an_embedded_template_dir` in `cli/src/core/rendered_template.rs`), with a comment reading "do not add to this list." Every step in §3's checklist must be completed for `cac-base` — leaving it half-wired like `ecommerce-stripe` is the one concrete way to get this wrong.
- **Two local-machine prerequisites, not architectural blockers.** This dev machine has no Rust toolchain installed (`cargo` not found — needs `rustup`), and git symlinks aren't materializing on this Windows checkout (`core.symlinks` is `false` — confirmed by inspecting `cli/src/templates/project/messaging-base/bootstrapper.ts`, which is a 43-byte text file containing the target path string, not a real symlink to the blueprint content). Both need fixing before a local `cargo build`/`cargo test` reflects real template content. CI runs on Linux and is unaffected by either.

Given this, `cac-base` is built as a genuine first-class module, the same way `billing-base`/`iam-base`/`messaging-base` are. `framework/core`, `framework/express`, and `framework/common` still need **zero changes** — confirmed by direct inspection, same as before this pivot. The entire lift is the CLI (`cli/`) plus three new `blueprint/` packages, detailed in §3.

---

## 2. Strategic context (why free-first)

From the reference doc: we have no paying hospital clients yet. CPT (procedure) codes are owned by the AMA and require a **paid commercial license** — everything else needed to run a real billing pipeline is free and government-published:

| Component | Source | Cost | Build now? |
|---|---|---|---|
| ICD-10-CM (diagnosis codes) | CDC/NCHS + CMS | Free | Yes |
| HCPCS Level II (supplies/drugs/equipment) | CMS | Free | Yes |
| NCCI PTP + MUE edit tables (code-conflict rules) | CMS | Free | Yes |
| LCD/NCD (diagnosis-procedure medical necessity) | CMS (per-MAC) | Free | Yes |
| CARC/RARC (denial reason codes) | X12 | Free to view | Yes |
| Eligibility / claim status (270/271) | Clearinghouse API | Per-transaction fee | Yes |
| **CPT (procedure codes)** | **AMA** | **Paid license** | **Defer until a hospital confirms as paying client** |

**Trigger to license CPT:** the moment a real hospital/clinic is confirmed as a paying client and will run real CPT-coded claims. Not before — this is about the paid contract with AMA specifically, since we can't legally hold or use real CPT data without it, at any edition or vintage. Until then, the platform is built, tested, and demoed using ICD-10 + HCPCS + **mock procedure codes** that carry the same shape/behavior as CPT without using AMA's actual code list.

**This does not mean CPT engineering itself waits for the trigger.** The founder's direction: CPT is the most valuable standard, so `CptCodeProvider` — the code path, the ingestion pipeline, the scrubbing-engine integration — should be built and structurally proven *now*, in parallel with the mock-code work, so that the only thing gated on the trigger is the license and the real data feed itself. See §5's "CPT readiness bar" and §10's revised Phase 2/Phase 6 split.

### AMA licensing timeline (once triggered)

| Step | What happens | Time |
|---|---|---|
| 1. Apply | Submit CPT distributor license application at ama-assn.org | Same day |
| 2. AMA reaches out | Licensing specialist schedules a call | Within 10 business days |
| 3. Discovery call | Discuss product, user count, distribution model | 1–2 weeks after step 2 |
| 4. Quote & contract | AMA sends pricing + license agreement | 1–2 weeks |
| 5. Signed & active | Legally allowed to use real CPT codes commercially | — |

**Total: ~4–6 weeks.** Because of the lead time, Phase 6 (licensing) must start **4–6 weeks before** the target go-live date for the first paying client — it should never be the thing blocking go-live.

### License type and pricing (researched — refines the source PDF)

The AMA distinguishes two license categories: an **end-user/limited license** (for organizations *using* CPT internally, e.g. a single hospital's own EHR) and a **distributor license** (for a company embedding CPT content into a product it distributes/sells to others). A multi-hospital SaaS billing platform needs the **distributor license**, and — importantly — **a separate license is required per product** that contains CPT content, not one blanket company-wide license.

Pricing is royalty-based and varies by product/user type rather than a single published rate card. Publicly reported reference points (vendor-reported, not an official AMA rate card — confirm exact numbers with an AMA licensing specialist on the discovery call):
- Roughly **$18–20 per clinician/user per year** for EHR/practice-management platforms.
- **~$0.24 per health-plan member per year** for payer-side use, with a **$100/year minimum royalty** on health-plan-model licenses.
- At least one reported vendor arrangement combining a flat annual royalty (~$1,050/yr) with a separate annual tooling fee (~$13,000/yr for "CPT Link" integration).

**Application inputs still required (unchanged from the source PDF):** legal company info, a product description (what the platform does & how CPT is used), technical details (how CPT data is stored/secured), scale info (# hospitals/users, SaaS model), and an **AI/LLM disclosure** if applicable — expect the per-clinician/per-provider-seat model to be the default the specialist proposes for a SaaS platform, and budget accordingly per hospital rather than as one fixed fee.

---

## 3. Target architecture — a first-class module, following messaging/ecommerce's proven pattern

Three packages, mirroring `billing`/`iam`/`messaging` exactly:

```
blueprint/interfaces/cac/              # pure contract package — no implementation
  interfaces/
    codeSet.service.interface.ts                  # CodeSetProvider contract (§5)
    claim.service.interface.ts
    patient.service.interface.ts
    eligibility.service.interface.ts
    remittance.service.interface.ts
  types/

blueprint/implementations/cac/base/    # concrete services implementing the interfaces
  services/
    mockProcedureCodeProvider.service.ts
    cptCodeProvider.service.ts                     # built to full readiness per §5 — not a stub
    claim.service.ts
    scrubbing.service.ts                           # three-layer engine, §6
  domain/schemas/{zod,typebox}/
  __test__/schemaEquality.test.ts

blueprint/cac-base/                     # the living reference app — entities, controllers, DI wiring
  api/
    controllers/        # patient, encounter, claim, eligibility, remittance, codeSet controllers
    routes/
  domain/
    enum/                # ClaimStatus, DenialReasonCategory, CodeSetType, LicenseStatus...
    mappers/              # request/response <-> entity mapping
    schemas/
    types/
  persistence/
    entities/             # see §4 — all built with defineComplianceEntity()
    migrations/           # hand-written DDL, see "Migrations" below
    seeders/              # small reference/config data only — NOT full code-set files, see §7
    seed.data.ts
  bootstrapper.ts, registrations.ts, sdk.ts, server.ts, mikro-orm.config.ts
  scripts/
    enforce-retention.ts   # reuse framework's RetentionService, same as billing/iam
    refresh-code-sets.ts   # see §7, follows the enforce-retention.ts pattern exactly
```

This is copy-the-shape from `blueprint/billing-base` and, even more directly now, from `blueprint/messaging-base`/`blueprint/implementations/messaging/base` — the newest module the team actually shipped, so it's the freshest reference for exactly which files matter.

### CLI wiring checklist (mirrors PR #264's real diff, not a hypothetical)

- **`cli/src/constants.rs`** — new `Module::BaseCac` variant (`id: "cac-base"`, `exclusive_files: Some(&["cac-base"])`), plus match arms in `get_service_module_name` (→ `"cac"`), `get_service_module_description`, and `get_service_module_cache` (likely `None` — no Redis dependency unless the §7 scrubbing-lookup cache needs it, in which case mirror billing/messaging's `Some(Infrastructure::Redis...)`).
- **`cli/src/core/modules.rs`** — new `CacConfig` enum + `ModuleConfig.cac` field + a `Module::BaseCac` arm in `validate_modules()`.
- **`cli/src/core/template.rs`** — **as-built (PR 1), this returns `None`, not a router list.** The `patient`/`encounter`/`claim`/`eligibility`/`remittance` routers listed here originally don't exist as real controllers until phase 1/2 add entities and business logic — declaring them now would have been metadata for routers that don't exist yet. `codeSet` (the one thing PR 1 actually built) isn't in this list either, since this registry is for manifest/conflict-detection metadata, not what gets scaffolded; it'll be added once there's a reason to track it for `forklaunch add router` conflict avoidance. Revisit this arm each phase as real routers land.
- **`cli/src/core/manifest/service.rs`** + `init/module.rs` + `init/application.rs` + `init/service.rs` — **as-built (PR 1), only `is_cac` was added — no `is_cac_configured`.** That flag exists for IAM/billing specifically because *other* modules call into them (an app needs to know "is IAM configured somewhere" to decide whether to inject `app_iam`). Nothing calls into `cac` the way things call into IAM/billing, so the pattern doesn't apply — confirmed by checking every `is_iam_configured`/`is_billing_configured` call site before assuming this needed mirroring.
- **`cli/src/core/package_json/project_package_json.rs`** + `package_json_constants.rs` — new `ProjectDependencies` fields + version constants for `@forklaunch/interfaces-cac` / `@forklaunch/implementation-cac-base`, using the machine-readable comment format `check_blueprint_deps` expects (per the messaging PR's notes).
- **`cli/src/templates/project/cac-base/*`** — git symlink farm into `blueprint/cac-base/*`, same mechanism as billing/iam/messaging (verify with `git config core.symlinks true` set and a real Windows Developer Mode / Linux checkout — see §1's local-prerequisite note).
- **Docker/env** — only if `cac-base` needs its own env vars beyond DB/Redis (e.g. a Stedi API key for local dev) — mirror messaging's `TWILIO_*` injection pattern in `docker-compose.yaml`, typed `Env` fields, `env_defaults.rs`, and `.env.template` category if so.
- **`client-sdk`** — gate a `cacSdkClient` into `clientSdk.ts`'s template if we want app-init to wire it automatically, mirroring messaging's client-sdk gating (also touches the app-init context that sets the flag).
- **`blueprint/.changeset`** — changesets for the new publishable packages (interfaces, implementation-base), same as messaging's `messaging-module.md`.
- **`docs/adding-projects/modules.md`** — add a row to the module table.
- **New CLI shell test** — `cli/tests/init_cac.sh`, following `init_module.sh`/`init_messaging_twilio.sh`; extend the e2e workflow glob so it actually runs (the messaging PR had to do this explicitly — don't skip it).
- **The regression test does the rest for free.** `every_module_variant_has_an_embedded_template_dir` (`cli/src/core/rendered_template.rs`) will fail loudly with the exact missing path if the template farm is incomplete — this is the safety net that would have caught the `ecommerce-stripe` gap. No changes needed to the test itself; just don't end up on its `KNOWN_MISSING_TEMPLATE_DIRS` allowlist.

**Module variant count — resolved, not left open.** Unlike `billing` (`-base`/`-stripe`) or `messaging` (`-base`/`-twilio`), `cac` gets **one** variant: `cac-base`. The mock-vs-real-CPT swap is an intra-service feature flag (§5), not a module-level implementation choice — there's no second package to publish. This mirrors how `ecommerce` currently has only `ecommerce-stripe` with no `-base` counterpart; a single-variant module family is an established, normal shape here, not a gap.

### Multi-tenancy: hospital = Organization

The framework already has organization-scoped tenant isolation (`framework/core/src/persistence/tenantFilter.ts`, `rls.ts`) and it's used today in `iam-base` (`Organization` entity, `organizationId` on JWT session). We reuse this directly:

- Each hospital/clinic client == one `Organization` (from the existing IAM module).
- Every `cac-base` entity carries an `organizationId` (tenant) field, exactly like `iam-base`'s pattern of scoping `User` queries by `organization.id` in `blueprint/iam-base/api/controllers/user.controller.ts`.

### RBAC: coders, billers, admins

Reuse IAM's `Role`/`Permission` entities and the existing permission-guard machinery (`framework/core/src/http/guards/hasPermissionChecks.ts`, wired into `auth.middleware.ts`, confirmed merged to `main` — §1). Routes declare `allowedPermissions` / `allowedRoles` exactly like `iam-base`'s controllers do today, e.g.:

- `coder:submit_claim`, `biller:view_remittance`, `admin:manage_codesets`, `auditor:read_only`.
- PHI-bearing read endpoints (patient demographics, claim detail) get stricter `allowedPermissions` than aggregate/analytics endpoints.

### Cross-service calls to IAM (concrete mechanism, confirmed by codebase inspection)

Two distinct SDK layers exist in this codebase and it's easy to reach for the wrong one:

- `blueprint/client-sdk` is explicitly for **external** consumers (dashboards, third-party integrations) — its own comment states internal services should **not** use it.
- For service-to-service calls, the actual pattern (see `blueprint/iam-base/surfacing.ts`) is: cache a typed client via `universalSdk<IamSdkClient>({ host: iamUrl, registryOptions: { path: 'api/v1/openapi' } })` (from `framework/universal-sdk`, which fetches the target service's OpenAPI spec and builds a typed client on the fly), then call e.g. `iamSdk.user.surfacePermissions({ params: { id }, headers: generateHmacAuthHeaders(...) })`.

`cac-base` will call IAM the same way — cache a `universalSdk<IamSdkClient>` instance in `registrations.ts`, and reuse it wherever a coder/biller's roles or permissions need to be surfaced, mirroring `createSurfacePermissions`/`createSurfaceRoles` in `surfacing.ts` almost verbatim rather than inventing a new integration style.

### Migrations (confirmed pattern)

`iam-base`'s migrations (`Migration00000000000000.ts`, `Migration00000000000001.ts`) are **hand-written DDL**, sequentially zero-padded, with migration 0 = schema and migration 1 = static seed data (permissions/roles), each with a matching reversible `down()`. `cac-base` follows the same convention — small, versioned reference tables (e.g. an initial `CodeSetType` lookup) belong in a migration exactly like IAM's permission/role seed; the *bulk* ICD-10/HCPCS code tables do not (see §7 — that's an ETL job, not a migration).

---

## 4. Data model (Phase 1 schema, compliance-classified)

Every entity uses `defineComplianceEntity()` (`framework/core/src/persistence/defineComplianceEntity.ts`), which **forces** every scalar field to declare `.compliance('pii' | 'phi' | 'pci' | 'none')` at the type level — the entity won't compile otherwise. `phi`/`pci` fields are automatically eligible for the framework's `FieldEncryptor` (AES-256-GCM, per-tenant HKDF-derived keys — `framework/core/src/persistence/fieldEncryptor.ts`).

| Entity | Key fields | Compliance notes |
|---|---|---|
| `Patient` | surrogate ID (internal UUID/MRN), name, DOB, address, contact; SSN **only if a payer requires it for eligibility** | `phi` on name/DOB/contact/SSN — encrypted at rest. See "SSN" note below. |
| `Insurance` | payer, member ID, group number | `phi` on member ID |
| `Encounter` (visit) | patient, provider, date, org | `phi` via relation; scalar fields mostly `none` |
| `Diagnosis` | ICD-10-CM code, encounter link | `none` — code itself is public data |
| `Charge` | procedure code (real CPT or mock placeholder), units, amount, encounter | `none`, gated by license (see §5); `units` matters for MUE checks (§6) |
| `Claim` | charges[], diagnoses[], status, payer | `phi` by relation to Patient |
| `Remittance` (ERA/835) | claim, paid amount, CARC/RARC codes | `none` |
| `Denial` | claim, CARC/RARC reason code, worklist status | `none` |
| `CodeSetLicense` | org, codeSetType (`cpt`), status (`none`/`pending`/`active`), signedAt | drives feature gate, see §5 |
| `AuditLog` | actor, action, entity, timestamp | uses framework's existing `auditLogger.ts` — every PHI read/write |

**On SSN specifically:** storing a raw SSN maximizes PHI blast radius for minimal benefit — most billing workflows only need it for payer eligibility verification, not as a primary key. Recommendation: use an internal surrogate identifier (UUID or MRN) as `Patient`'s reference everywhere in the domain model, and store SSN only in the fields a specific payer integration actually requires, still `phi`-classified and encrypted. This is a direct application of HIPAA's "minimum necessary" principle, not just an encryption checkbox.

All entities get a `retention` policy via the same mechanism `RetentionService` (`framework/core/src/services/retentionService.ts`) already enforces for billing/iam — e.g. denial worklist records anonymized after N years per HIPAA §164.530(j), enforced by `scripts/enforce-retention.ts` exactly as the other modules do.

---

## 5. Code-set provider abstraction, CPT readiness, and license gating

This is the mechanism that makes "free-first, swap later" actually safe and mechanical rather than a manual migration — and, per the founder's direction, the mechanism that makes "enable CPT" a flag flip against finished code rather than a Phase 6 engineering scramble.

**Pattern to copy:** `billing-base` already solves an almost identical problem — "the real provider (Stripe) costs money and isn't always configured, so build against an interface and swap the implementation." We do the same for procedure codes:

```
CodeSetProvider (interface)
 ├─ MockProcedureCodeProvider   — "PROC-001: Office Visit", built Phase 2, no license needed
 └─ CptCodeProvider             — real AMA CPT data — built to full readiness in Phase 2 alongside
                                   the mock provider; only its real data feed and license wait on the trigger
```

### CPT readiness bar — what "in good shape" concretely means

Built and proven *before* the AMA trigger, not after:

1. **`CptCodeProvider` fully implements the `CodeSetProvider` interface** — same rigor as `MockProcedureCodeProvider`, not a stub or a TODO. No new interface work happens at Phase 6.
2. **`refresh-code-sets.ts` (§7) is generalized to ingest a CPT-shaped feed**, parameterized so pointing it at the real licensed data source at Phase 6 is a config change, not new ETL code. (The exact delivery format AMA uses — file drop, API, etc. — is still unknown; see §12's open question on this. The pipeline is built pluggable specifically *because* that answer isn't known yet.)
3. **The three-layer scrubbing engine (§6) is tested against a synthetic, CPT-*shaped* fixture** — real CPT's actual numeric structure and code-range categories (Category I is 5-digit numeric, ranges like 10000–69990 for surgery, 70000–79999 for radiology, etc.; Category II is 4 digits + `F`; Category III is 4 digits + `T`) — not just against `MockProcedureCodeProvider`'s placeholder strings. This proves the NCCI PTP/MUE and LCD/NCD logic works against something structurally real, without reproducing any of AMA's actual copyrighted code+description content pre-license (see §12's open question on who defines this fixture).
4. **A smoke-test runbook is written and ready before the trigger**, so that once real data lands the validation step is hours, not a new test-design effort.

**License gate, reusing an existing framework guard — not a new primitive:** the framework already has `hasFeatureChecks` / `hasSubscriptionChecks` guards (`framework/core/src/http/guards/`, wired into `auth.middleware.ts`) that gate routes on `requiredFeatures` resolved per-request via a `surfaceFeatures(session, req)` callback — this is the exact mechanism billing uses for entitlement-gated features. We model **"real CPT codes active"** as a feature flag surfaced from the `CodeSetLicense` entity:

- Before license is signed: `surfaceFeatures` never returns `cpt-licensed`, so any route/behavior requiring real CPT falls back to `MockProcedureCodeProvider` — even though `CptCodeProvider` is fully built and sitting idle behind the flag.
- After license is signed and `CodeSetLicense.status = 'active'`: `surfaceFeatures` returns `cpt-licensed` for that organization, `CptCodeProvider` is used, same scrubbing/claim logic runs unchanged.

This also gives a clean **per-hospital** cutover: some organizations can stay on mock codes while others are already licensed, with zero risk of one tenant's licensing status leaking into another (enforced by the same tenant-isolation filter used everywhere else).

**License-check failure mode (fail closed).** `surfaceFeatures` resolves `cpt-licensed` via a cross-service call — if that lookup fails or times out mid-claim-submission (e.g. a network blip between `cac-base` and IAM), treat the organization as unlicensed and fall back to `MockProcedureCodeProvider` rather than blocking claim submission. A stalled claims pipeline is worse than a claim coded against mock data that can be re-submitted later — and, per the rule below, historical claims are never retroactively recoded anyway, so a transient mock-coded claim during an outage is not a special case to design around.

**What "untested" honestly means here, and where the line is.** The founder's "it's ok if it's untested" should mean: it's fine that `CptCodeProvider` hasn't been exercised against real production traffic before the trigger fires — we can't legally get real CPT data to test with pre-license, so of course that gap exists, and it's not a reason to hold the whole plan hostage. It should *not* mean: skip the smoke-test runbook (item 4 above) or submit a real claim to a real payer before running it. Once real data lands, running the pre-built runbook is cheap; skipping it isn't.

**Whichever CPT edition is available at trigger time is fine to use.** Don't hold out for the current-year release if AMA's specialist offers an earlier one — the readiness work above is edition-agnostic by design (it's built against CPT's structural shape, not any specific year's content), so swapping editions later is the same low-cost flag flip as licensing for the first time.

**Historical claims are never retroactively recoded.** When an organization's `CodeSetLicense` flips to `active`, only *new* encounters created afterward use `CptCodeProvider`. Claims already built and submitted under `MockProcedureCodeProvider` remain exactly as they were coded — a submitted claim is a financial/legal record, and retroactively changing its procedure codes after the fact would itself be a compliance problem. This should be stated explicitly to any pilot client during Phase 5 demos (see §12).

---

## 6. Claim scrubbing engine — three distinct rule layers

The source doc's phase plan referred to a single, vaguely-named "diagnosis-procedure necessity check" and "NCCI code-pair conflicts (ICD-10 side)." Research corrected this: **NCCI and medical-necessity checking are two unrelated mechanisms with different data sources**, and the original wording conflated them. The scrubbing engine needs three separate rule layers:

| Layer | What it checks | Code pairs involved | Data source | Update cadence | Typical denial if missed |
|---|---|---|---|---|---|
| **NCCI PTP** (Procedure-to-Procedure) | Two procedures billed together that shouldn't be, absent a justifying modifier | CPT/HCPCS ↔ CPT/HCPCS only — **never ICD-10** | CMS NCCI PTP edit tables | Quarterly | CO-97 ("benefit included in another service already adjudicated") |
| **NCCI MUE** (Medically Unlikely Edits) | Implausible unit count for a single code on one date of service (e.g. 5 appendectomies) | Single CPT/HCPCS code + units | CMS NCCI MUE tables | Quarterly | Line- or date-of-service-level unit denial |
| **LCD/NCD medical necessity** | Whether a diagnosis justifies a procedure at all | ICD-10-CM ↔ CPT/HCPCS — this *is* the real diagnosis-procedure crosswalk | CMS Medicare Coverage Database, per Medicare Administrative Contractor (MAC) — coverage is regional | Ongoing, MAC-specific | CO-50 ("not deemed a medical necessity") or CO-11 ("diagnosis is inconsistent with the procedure") |

Each mock procedure code built in Phase 2 needs a corresponding mock LCD-style mapping (which mock diagnoses justify which mock procedures) so the scrubbing logic and its test suite are exercising the real three-layer shape — not a placeholder single check — before real CPT data is swapped in. Per §5's readiness bar, this same scrubbing engine also needs a second pass of tests run against the synthetic CPT-*shaped* fixture (real numeric code-range structure, no real AMA content) — the mock-placeholder tests prove the logic is correct, the CPT-shaped tests prove it survives contact with real-shaped data before the license ever arrives.

**A subtlety §2's free/paid table doesn't fully capture: real LCD/NCD data is coupled to CPT licensing, even though it has no license of its own.** LCD/NCD data itself is free CMS data (§2 correctly marks it "Build now: Yes"), but every real LCD policy is written *in terms of real CPT/HCPCS codes* — "procedure X is covered for diagnoses A, B, C" only means something once "procedure X" is a real CPT code, not a mock placeholder. So while the LCD/NCD *data* can be downloaded and stored pre-license, a *meaningful* real LCD crosswalk can't be built until CPT is licensed either. Phase 6 (§10) already accounts for re-running QA on real CPT+ICD-10 pairs — this should explicitly include re-ingesting real LCD/NCD crosswalks at the same time, not treat LCD/NCD as "already done" just because it was technically buildable earlier.

### Reference: exact CARC definitions in use (confirmed against x12.org)

| Code | Definition | Layer it maps to |
|---|---|---|
| CO-11 | "The diagnosis is inconsistent with the procedure." | LCD/NCD medical necessity |
| CO-16 | "Claim/service lacks information or has submission/billing error(s)" (always paired with a RARC specifying what's missing) | Required-fields scrubbing |
| CO-27 | "Expenses incurred after coverage terminated." | Eligibility (270/271) |
| CO-50 | "These are non-covered services because this is not deemed a 'medical necessity' by the payer." | LCD/NCD medical necessity |
| CO-97 | "The benefit for this service is included in the payment/allowance for another service/procedure that has already been adjudicated." | NCCI PTP bundling |

---

## 7. Code-set lifecycle: update cadences and the refresh pipeline

Every code set this platform depends on has its own publisher and update cadence — the plan needs an explicit refresh mechanism, not just a one-time Phase-1 loader.

| Code set | Publisher | Cadence |
|---|---|---|
| ICD-10-CM | CDC/NCHS + CMS | Annual, effective **October 1** |
| HCPCS Level II | CMS | **Quarterly** (Jan/Apr/Jul/Oct) |
| CPT Category I | AMA | Annual, effective **January 1** (published previous fall) |
| CPT Category III | AMA | **Semiannual** (Jan/Jul) |
| NCCI PTP + MUE tables | CMS | **Quarterly** |
| CARC/RARC | X12 | **3×/year** (March, July, November) |

**Refresh mechanism — no framework-native cron exists (confirmed by codebase inspection):** `framework/implementations/worker/{bullmq,kafka,redis,database}` expose only `enqueueJob`/`enqueueBatchJobs`/`start` — no repeat/cron primitive is surfaced anywhere, even though BullMQ itself supports one internally. The one real precedent in this codebase is `scripts/enforce-retention.ts`, wired to a plain `"retention:enforce"` npm script that an external scheduler (k8s CronJob / cloud scheduler) invokes — there's no in-repo trigger. `cac-base` should follow this exact convention: a `scripts/refresh-code-sets.ts` + `"codeset:refresh"` npm script, invoked externally on a schedule matched to the table above (the tightest cadence — HCPCS/NCCI quarterly — sets the polling interval).

**Scrubbing lookups must be cached, not per-line queries.** The §6 scrubbing engine checks NCCI PTP/MUE and LCD/NCD edit tables per claim line, and these tables run into the tens of thousands of code pairs. A naive per-line DB query against them is an N+1 risk on multi-line claims at any real submission volume. Scrubbing should check these tables against an in-memory or Redis-cached lookup keyed by the active code-set version, refreshed each time `refresh-code-sets.ts` runs on its quarterly cadence — not queried fresh per claim line. Demo-scale (Phase 2–5) volume won't expose this, but it's cheap to design correctly now versus retrofitting a caching layer onto a scrubbing engine that already shipped with naive queries.

**Bulk loading needs a dedicated ETL step, not the seeder pattern.** The existing `persistence/seeders/*.seeder.ts` + `seed.data.ts` pattern (e.g. `blueprint/billing-base/persistence/seeders/plan.seeder.ts`) is a thin wrapper that does one `em.create(...).flush()` per hand-written object literal — clearly sized for a handful of config rows, not the ~70,000 ICD-10-CM codes or ~7,000 HCPCS codes. `framework/infrastructure/S3`'s `S3ObjectStore` is closer but its `putBatchObjects` is just `Promise.all` over individual JSON puts, not a bulk-CSV loader either. `refresh-code-sets.ts` should instead: stream the government-published CSV/XML (staged in S3), parse it, and batch-insert via MikroORM in chunks (e.g. 1,000 rows per `em.persist(...).flush()`) — a purpose-built ETL script, following the *shape* of `enforce-retention.ts`'s batching loop (`framework/core/src/services/retentionService.ts` already batches at 1,000 records/flush for exactly this reason) but against a new code-set-specific service, not a reused generic primitive.

---

## 8. EDI transaction sets and clearinghouse choice

**Transaction sets needed** (confirmed standard for a hospital billing platform): **837** (claim submission — P/I/D variants), **835** (ERA/remittance), **270/271** (eligibility inquiry/response), **276/277** (claim status inquiry/response), **277CA** (claim acknowledgment — front-end edit summary on the 837), and **999** (functional acknowledgment, the 5010 replacement for 997). Given hospital utilization review needs, also plan for **278** (prior authorization/referral) even though the source doc didn't call it out.

**Version:** the HIPAA-mandated version remains **X12 005010** — a proposed 008020 update was declined by NCVHS in 2023, and while X12 has since published an 008060 guide as a forward candidate, nothing is mandated yet. Build against 5010 with **CAQH CORE operating-rule compliance** (e.g. its 835 Code Combinations rules, eligibility/claim-status response-time rules) as the near-term target — don't design for a version bump that hasn't been mandated.

**Clearinghouse decision (resolves the previously-open question):** **Stedi** for the primary sandbox/integration — it's the only clearinghouse of the three that's API-first, accepting/returning JSON rather than raw X12 for 837/270/271/276/277/835, with a permanently free sandbox and pure usage-based pricing (no monthly minimum). **Claim.MD** as a lower-cost secondary/fallback (REST+XML, ~$0.10–0.25/claim or ~$100/month unlimited) for redundancy. **Availity** deferred to a later phase — it has the broadest payer network but is architected EDI/portal-first with self-serve API access weaker than the other two; only worth the onboarding overhead once claim volume justifies its network breadth.

---

## 9. Compliance / HIPAA posture

We are not starting from zero — `COMPLIANCE_COVERAGE.md` shows the framework already addresses 34/43 cross-standard (HIPAA/SOC2/PCI/GDPR) requirements at the framework layer, including the exact things a PHI-handling service needs: field-level data classification + PHI encryption at rest, tenant isolation, access control + audit logging, automatic session logoff, right to erasure, data portability, and data retention/disposal.

Confirmed by direct code inspection and by `COMPLIANCE_GAPS_PLAN.md`'s own framing — every remaining framework gap is "no new framework primitives needed," meaning the compliance layer is intentionally module-agnostic. Concretely, for free, with zero extra code in `cac-base`:

- Any entity field marked `.compliance('phi')` on a `defineComplianceEntity()` is **auto-encrypted** (AES-256-GCM, per-tenant HKDF-derived key) via `EncryptedType`/`FieldEncryptor`.
- `forklaunch init service`'s generic router template already generates a `compliance.controller.ts` exposing `DELETE /erase/:userId` and `GET /export/:userId` (HMAC-protected, internal-only), backed by `ComplianceDataService` — GDPR-style per-patient erase/export for free.
- `scripts/enforce-retention.ts` + `RetentionService` batches delete/anonymize per entity's `retention` policy.
- Tenant isolation (`tenantFilter.ts` + `rls.ts`) activates automatically for any entity with an `organizationId`/`organization` relation.

Gaps called out in `COMPLIANCE_GAPS_PLAN.md` (consent management, pen testing, DR testing) are framework/CLI-level and orthogonal to this module. The **new** thing this module needs that doesn't exist yet is the `CodeSetLicense` entity/feature-gate itself (§5) — everything else is reuse.

Before any real hospital data touches the system: run an external security review, same as any other PHI-bearing service on this stack.

### Test/QA strategy (confirmed against existing test conventions — fills a prior gap)

`billing-base`'s tests (`__test__/test-utils.ts`, `plan.test.ts`) use `BlueprintTestHarness` from `@forklaunch/testing`, backed by **real `testcontainers`** (Postgres + Redis) — not mocks, not in-memory sqlite. Test data is seeded via real MikroORM entities, and assertions call the generated route SDK in-process (`route.sdk.createPlan({...})`) rather than raw HTTP. This directly answers the previously-open question of how to validate the mock→real CPT cutover: **spin up the real containerized test DB, seed a representative subset of both mock and real code pairs (including at least one of each CARC scenario in §6's table), and assert against the actual `sdk.*` calls end-to-end** — not a mocked unit test — before flipping `CodeSetLicense.status` to `active` for any real organization.

**Required test matrix (one row per scrubbing scenario from §6):**

| Scenario | Layer exercised | Test file | Asserts |
|---|---|---|---|
| Two procedures billed together without a justifying modifier | NCCI PTP | `scrubbing.ncciPtp.test.ts` | Claim rejected pre-submission with CO-97-equivalent internal denial code |
| Implausible unit count for a single code/date-of-service | NCCI MUE | `scrubbing.ncciMue.test.ts` | Claim rejected with a unit-level denial, valid unit counts pass |
| Diagnosis doesn't justify the procedure (mock LCD-style crosswalk) | LCD/NCD medical necessity | `scrubbing.lcdNcd.test.ts` | Claim rejected with CO-11/CO-50-equivalent, covered diagnosis-procedure pairs pass |
| Missing required claim field | Required-fields scrubbing | `scrubbing.requiredFields.test.ts` | CO-16-equivalent with the specific missing field surfaced |
| Eligibility check fails at intake (coverage terminated) | Eligibility (270/271) | `eligibility.test.ts` | CO-27-equivalent, blocks claim submission before it reaches scrubbing |
| `CodeSetLicense` flips to `active` mid-organization-lifecycle | Mock→real CPT cutover (§5) | `codeSetCutover.test.ts` | **Regression test:** claims submitted under `MockProcedureCodeProvider` before the flip are byte-for-byte unchanged after the flip; only claims created after the flip use `CptCodeProvider` |
| License-check lookup to IAM fails/times out | License-gate fail-closed (§5) | `codeSetLicenseGate.test.ts` | Organization falls back to `MockProcedureCodeProvider` rather than blocking claim submission |

Each row is a full `testcontainers` end-to-end test per this section's harness, not a mocked unit test — the scrubbing engine's correctness is exactly the kind of logic where a passing mock and a failing production query diverge.

---

## 10. Phased delivery plan

Directly adopting the source doc's phases, corrected for the domain-accuracy fixes in §6–8 and for the Option B pivot in §1. **Caveat: the week ranges below are inherited from the source doc, not re-estimated against this team's actual capacity or velocity, and Phase 0 now carries CLI-wiring work that didn't exist in the original estimate** — treat them as a starting hypothesis to validate in a sizing session before committing to them externally, not as a researched estimate.

| Phase | Focus | CPT needed? | Concrete engineering tasks |
|---|---|---|---|
| **0** (Wk 1–4) | Foundations **+ module registration** | No | **Register `Module::BaseCac` in the CLI and scaffold the three blueprint packages per §3's checklist** — this must land before `forklaunch init module -m cac-base` exists to do anything else in this phase; Stedi sandbox credentials (§8); confirm HIPAA-ready hosting + BAA; synthetic test dataset |
| **1** (Wk 3–6) | Code validation | No | ICD-10-CM loader; extend to HCPCS Level II; define all entities in §4 with `defineComplianceEntity`; stand up `scripts/refresh-code-sets.ts` (§7) even before it's needed on a schedule, so the ETL shape exists from day one |
| **2** (Wk 6–10) | Claim engine & scrubbing, mock codes **+ CPT readiness** | No — placeholders, but `CptCodeProvider` is built here too | `MockProcedureCodeProvider`; claim builder (encounter+charges+diagnoses→claim); scrubbing rules across all **three layers** from §6 — mock NCCI PTP pairs, mock MUE unit caps, mock LCD-style diagnosis-procedure crosswalk; clearinghouse sandbox submission end-to-end via Stedi. **In parallel:** build `CptCodeProvider` to full readiness per §5's bar — interface implementation, generalized ETL, and scrubbing-engine tests against the synthetic CPT-shaped fixture — so Phase 6 has no new engineering left to do |
| **3** (Wk 10–14) | Eligibility & remittance | No | EDI 270/271 eligibility check at intake (blocks CO-27); 835 remittance parsing, auto-post + CARC/RARC capture; 277CA/999 acknowledgment handling; denial worklist UI |
| **4** (Wk 14–16) | Analytics & compliance hardening | No | Clean-claim-rate / denial-rate / days-in-A/R dashboard (benchmarked against §11); confirm RBAC + audit logging on every PHI path; external security review |
| **5** (Mo 4–6) | Sales demos | No — mock codes | Demo to small clinics/billing companies; use CO-27/CO-16/CO-97 denial examples as live proof points; be transparent that CO-11/CO-50 (LCD/NCD) checks run against mock diagnosis-procedure mappings pre-license, and that historical claims are never retroactively recoded post-license (§5) |
| **6** (Trigger-based) | CPT licensing & go-live — **activation only, not new engineering** | **Yes** | On trigger: contact AMA immediately (§2); file license 4–6 weeks before go-live target, budgeting per-clinician royalty pricing (§2), accepting whichever edition AMA offers (§5); once signed — point the already-built `CptCodeProvider`'s ETL at the real licensed feed, **ingest real LCD/NCD crosswalks for the relevant MAC jurisdiction(s)** (§6 — this cannot happen meaningfully before real CPT exists), flip `CodeSetLicense.status='active'` for that org, run the pre-built smoke-test runbook (§5) and the container-based cutover test suite (§9) against real CPT+ICD-10 pairs before first real claim. If Phase 2's readiness work was done properly, this phase is measured in days, not weeks |

**Do not start Phase 6 on a calendar** — it's trigger-based off a real signed/verbally-confirmed paying client, run in parallel with that client's onboarding.

---

## 11. Success metrics (benchmarked against industry data — refines the source doc's targets)

| Metric | Trial baseline | Plan target | Industry benchmark context |
|---|---|---|---|
| Clean claim rate | 60% | 95%+ | Matches MGMA's "good performance" benchmark (~95%); industry median is often 85–90%. **Credible, appropriately aggressive.** |
| Denial rate | 30% | Under 5% | HFMA considers <5% "optimal," but industry average is 5–10% and initial denial rates were ~11.8% in 2024, trending toward 12–15%. **This is a best-in-class target, not a typical baseline — it will require real scrubbing-engine automation (§6), not just clean data entry.** |
| Average days to payment | 24 days | Under 40 days | MGMA's 2024 survey shows top performers at 36 days vs. a 47-day median; HFMA's healthy range is 30–40. **Realistic/good as stated — consider tightening to sub-35 days to match a genuine "high performer" framing rather than just "healthy."** |

---

## 12. Open questions

1. **IAM cross-service integration** — does the hospital's staff (coders/billers) get provisioned in the *existing* `iam-base` module as `User`s with new `Role`s, or does this need its own lightweight staff directory? (Recommend: existing IAM — the cross-service SDK mechanism in §3 makes this straightforward, and avoids duplicating auth.)
2. **CO-11/CO-50 demo honesty** — Phase 5 demos must clearly disclose to prospects that LCD/NCD-style medical-necessity checks run against a mock diagnosis-procedure crosswalk pre-license, not real CMS coverage data; confirm sales is aligned on this messaging.
3. **Mock LCD/NCD data source and synthetic CPT-shaped fixture design** — who owns building (a) a plausible mock diagnosis-procedure crosswalk for Phase 2's mock-code path, and (b) the synthetic CPT-*shaped* fixture used to test `CptCodeProvider` and the scrubbing engine per §5's readiness bar? Both need a coding/compliance SME, not engineering alone — and (b) specifically needs sign-off that the fixture is structurally realistic (real numeric ranges/categories) without reproducing any of AMA's actual copyrighted code+description content pre-license.
4. **MAC jurisdiction scope for real LCD ingestion** — once CPT is licensed (§6, §10 Phase 6), which Medicare Administrative Contractor jurisdiction(s) does the first real client fall under? LCD coverage is regional, so Phase 6's "ingest real LCD/NCD crosswalks" task needs this answered before it can be scoped, not after.
5. **Phase timeline validation** — the Phase 0–6 week estimates (§10) come from the source doc, not from this team's actual velocity, and Phase 0 now also carries CLI module-registration work that wasn't in the original estimate. Needs a sizing session with whoever will staff this before the timeline is quoted to a prospective client or used to plan the AMA licensing lead time against a real go-live date.
6. **Multi-org billers** — §3 models each hospital/clinic as one `Organization`, with RBAC and tenant isolation scoped to a single org per user (matching `iam-base`'s existing model exactly). But §2's own go-to-market ("demos to small clinics/billing companies") implies some customers may be third-party billing companies whose coders/billers need visibility across multiple hospital clients — a cross-org access pattern the current single-tenant-per-user model doesn't support. Unclear whether the near-term (mock-code, small-clinic-demo) target is hospitals' own staff or billing companies acting on their behalf; answering this wrong now risks baking in the wrong RBAC model. Needs resolving before Phase 2's RBAC work, not after.
7. **AMA's real CPT data delivery mechanism** — file drop, API, or something else? §5's `refresh-code-sets.ts` generalization is built pluggable specifically because this is unknown; worth asking directly on the AMA discovery call (§2) so Phase 2's readiness work targets the right shape rather than guessing.
8. **Does `cac-base` need its own env vars / Redis dependency?** §3's CLI checklist flags this as conditional — resolve during Phase 0 once the Stedi integration and §7's caching design (Redis-backed scrubbing lookups) are scoped, since that determines whether `get_service_module_cache` should return `Some(Infrastructure::Redis...)` like billing/messaging do, or `None`.
9. **Does the "CAC" name imply NLP-based code suggestion?** (§1) Industry-wide, Computer-Assisted Coding means software that suggests codes from clinical documentation via NLP/AI — this plan's scope (§6's scrubbing engine) validates codes already entered, it doesn't suggest them. Confirm with the founder whether `cac-base` is meant to grow into that capability later (in which case the name is a deliberate, forward-looking choice) or whether it's just the industry-standard label being applied to a validation/billing engine — either is fine, but worth being explicit so Phase 2+ scope doesn't quietly drift toward NLP work nobody's planned for.

---

## 13. Immediate next steps

1. **Register `Module::BaseCac` in the CLI and scaffold the three blueprint packages**, following §3's checklist and PR #264's real diff as the concrete template. This is the literal prerequisite for everything else — nothing else in Phase 0 can use `forklaunch init module` until this lands.
2. Run a sizing session against §10's phase estimates (now including the CLI work above) with whoever will actually staff this, before quoting the timeline externally (§12, item 5).
3. Create a Stedi sandbox account (free) and request API credentials (§8).
4. Finalize the `cac-base` entity schema (§4) — including the SSN/surrogate-ID decision — and confirm HIPAA-ready hosting/BAA.
5. Extend the existing ICD-10 loader to HCPCS using the same pattern; stand up the `refresh-code-sets.ts` ETL shape (§7) even before its first scheduled run.
6. Get a coding/compliance SME to define the mock LCD/NCD crosswalk *and* the synthetic CPT-shaped fixture (§12, item 3) — both block Phase 2 and need lead time, so line this up now rather than discovering the gap mid-phase.
7. Build the scrubbing rules engine against `MockProcedureCodeProvider`, implementing all **three** rule layers from §6 — **and, per the founder's direction, build `CptCodeProvider` to full readiness (§5) in parallel, not as a Phase 6 afterthought.**
8. Begin outreach to small clinics/billing companies for early demos — no license required yet.
9. Only once a paying client is confirmed: contact AMA and start the actual CPT *licensing* process (§2) — by this point the engineering side should already be done, so this step is a business/legal process, not an engineering one.

---

## 14. PR breakdown

One PR per phase from §10, mapped 1:1 — six PRs total (Phase 5 is GTM-only, no engineering PR):

| PR | Phase | Scope |
|---|---|---|
| PR 1 | Phase 0 | **CLI module registration + blueprint package skeleton (§3) — implemented, on `feat/medical-coding-module-decision` as two commits (CLI wiring, then the three blueprint packages).** Stedi sandbox, HIPAA hosting/BAA, and synthetic test dataset are business/ops tasks still outstanding, not engineering |
| PR 2 | Phase 1 | Code validation — all §4 entities, ICD-10-CM + HCPCS loaders, `refresh-code-sets.ts` ETL shape |
| PR 3 | Phase 2 | Claim engine, three-layer scrubbing, **and** `CptCodeProvider` built to full readiness (§5) |
| PR 4 | Phase 3 | Eligibility & remittance — 270/271, 835, 277CA/999, denial worklist |
| PR 5 | Phase 4 | Analytics dashboard + RBAC/audit verification pass (the external security review sits outside any PR) |
| PR 6 | Phase 6 | Activation only — point the already-built `CptCodeProvider` at the real feed, ingest real LCD/NCD, flip the flag |

**PR 1 is now larger than originally scoped** — it carries the entire CLI wiring checklist from §3 (Rust `Module` enum + ~11 other files, the git-symlinked template farm, docs, and a new e2e shell test) on top of what was already there. **PR 3 remains the largest engineering PR overall** — it carries the claim builder, all three scrubbing layers (NCCI PTP, NCCI MUE, LCD/NCD), the clearinghouse submission path, *and* the entire CPT-readiness build (§5's four-item bar). Both should land as a sequence of reviewable commits/checkpoints within the PR rather than a single undifferentiated diff — PR 1 in particular has a natural split (CLI-side Rust changes, then the three blueprint packages) worth keeping visible in the commit history even though it's one PR.

This count is a working estimate, same caveat as §10 and §12 item 5 — it should flex with whatever the sizing session decides, not be treated as fixed.

---

## 15. Sources

Domain research behind §2, §6, §7, §8, and §11:

- CMS — [NCCI Procedure-to-Procedure (PTP) Edits](https://www.cms.gov/medicare/coding-billing/national-correct-coding-initiative-ncci-edits/medicare-ncci-procedure-procedure-ptp-edits)
- CMS — [NCCI Medically Unlikely Edits (MUEs)](https://www.cms.gov/medicare/coding-billing/national-correct-coding-initiative-ncci-edits/medicare-ncci-medically-unlikely-edits-mues)
- CMS — [Medicare Coverage Database (LCD/NCD search)](https://www.cms.gov/medicare-coverage-database/search.aspx)
- CMS — [HCPCS Quarterly Update](https://www.cms.gov/medicare/coding-billing/healthcare-common-procedure-system/quarterly-update)
- CDC — [ICD-10-CM files](https://www.cdc.gov/nchs/icd/icd-10-cm/files.html); AAPC — [FY2026 ICD-10-CM guidelines](https://www.aapc.com/blog/92967-coding-update-fy-2026-icd-10-cm-official-guidelines-released/)
- AMA — [CPT 2026 code set release](https://www.ama-assn.org/press-center/ama-press-releases/ama-releases-cpt-2026-code-set)
- AAPC — [CARC/RARC/MREP update cadence](https://www.aapc.com/codes/exclusives/transmittals/claim-adjustment-reason-code-carc-remittance-advice-remark-code-rarc-and-medicare-remit-easy-print-mrep-update-2); X12 — [code lists](https://x12.org/codes); X12 — [Claim Adjustment Reason Codes](https://x12.org/codes/claim-adjustment-reason-codes)
- WEDI — [HIPAA Transactions and Operating Rules](https://www.wedi.org/hipaa-transactions-and-operating-rules/); Accountable HQ — [EDI transactions explained](https://www.accountablehq.com/post/hipaa-edi-transactions-explained-types-x12-codes-and-compliance)
- AMA — [CPT Licensing FAQs](https://www.ama-assn.org/practice-management/cpt/cpt-licensing-frequently-asked-questions-faqs); AMA compliance portal — [Standard CPT Distribution Pricing Schedule 2026](https://compliance.ama-assn.org/hc/en-us/articles/15166274293399-Notice-Standard-CPT-Distribution-Pricing-Schedule-2026)
- MGMA-benchmarked summaries: Human Medical Billing — [2025 medical billing KPIs](https://humanmedicalbilling.com/blog/essential-medical-billing-kpis-for-2025-metrics-that-matter-for-revenue-cycle-success/); HFMA — [Redesigning denials management](https://www.hfma.org/revenue-cycle/redesigning-denials-management-in-the-obbba-era/); BillingBench — [RCM benchmarks](https://billingbench.com/benchmarks)
- Stedi — [API-first clearinghouse](https://www.stedi.com/blog/stedi-healthcare-the-only-api-first-clearinghouse-for-health-tech-companies) / [docs](https://www.stedi.com/docs/healthcare); Claim.MD — [software vendor integration](https://www.claim.md/services-software-vendors); Availity — [API guide](https://developer.availity.com/blog/2025/3/25/availity-api-guide)
- ForkLaunch precedent: PR #264 ("feat(messaging): messaging-base + messaging-twilio preconfigured modules") — the concrete template for §1 and §3, confirmed via `git show` on this repo rather than external research.
