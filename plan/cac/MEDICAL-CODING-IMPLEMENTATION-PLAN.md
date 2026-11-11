# Medical Coding / Billing Platform — Implementation Plan

**Status:** Draft for review
**Owner:** Engineering
**Based on:** `implementation_plan_free_first.docx` (v2.0, Free-First Strategy) and `ama_cpt_license_timeline.pdf`, reconciled against the current ForkLaunch codebase, deepened with a second codebase pass and external research into medical-coding domain standards (CMS, AMA, X12, MGMA/HFMA — see §16 Sources), revised after a live precedent check against the framework's own recently-added module families (messaging, ecommerce — see §1), and revised again after the founder resolved the module's business model directly (§1, §2).

---

## Executive summary

- **What:** a HIPAA-compliant medical coding/billing module — `Module::BaseCac` (`cac-base`) — built as a first-class ForkLaunch module the same way `billing-base`/`iam-base`/`messaging-base` are, consumable via `forklaunch init module -m cac-base` by any ForkLaunch application. `framework/core`/`express`/`common` need zero changes; the CLI (`cli/`) and three new `blueprint/` packages are the entire lift (§1, §3).
- **ForkLaunch never holds a CPT license — this is a toolkit, not our own hospital product.** Confirmed directly by the founder: `cac-base` is a reusable building block, like `iam-base`/`billing-base`, that other companies use to build and self-host their own hospital/billing product under their own AMA CPT license. ForkLaunch is not the AMA's licensee and never becomes one (§1, §2).
- **CPT support has to be genuine and ready now, not deferred behind a trigger.** Earlier drafts of this plan gated real CPT on "our first paying hospital client." That premise is gone: CAC's actual customers are companies that already hold (or are independently pursuing) their own CPT license by the time they reach for this module, so the `CodeSetProvider` extension point for real CPT has to work today, well-documented, for anyone plugging in their own licensed data — not something we finish "once triggered" (§5).
- **Free-first still describes what ships fully built, usable by anyone with zero license:** ICD-10-CM, HCPCS, NCCI, and LCD/NCD-shaped scrubbing, using mock procedure codes — built, tested, and now also demoable through a lightweight validation UI in the separate `forklaunch-platform` repo (§10).
- **What research corrected:** the source doc conflated NCCI code-conflict rules with diagnosis-procedure medical necessity. These are two separate CMS mechanisms with different data, cadences, and denial codes — the scrubbing engine has three distinct rule layers instead of one (§6).
- **CAC's job stops at the report — we never submit claims on an adopter's behalf.** Confirmed by the founder (2026-08-31): "we don't need to execute the claim on the business's behalf, just map the CPT codes — they can take the report and do whatever they like with it." This removes eligibility checks, clearinghouse claim submission, and remittance parsing from `cac-base`'s scope **entirely** — not deferred pending a Stedi account, but not something this module builds, period. The adopting business takes the scrubbing report (§6) — pass/fail, and exactly which layer and CARC code caught a problem — and handles actual submission through their own systems. See §8, §10 (old Phase 3), §14 (old PR 4).
- **What's still open, not just deferred:** the Phase 0–2 week estimates are inherited from the source doc and unvalidated against actual team capacity (§11); a *meaningful* real LCD/NCD crosswalk can't exist until whichever downstream customer wires in real CPT does so themselves, even though the LCD data itself is free (§6, §12); and the `forklaunch-platform` validation UI's scope and hosting haven't been explored yet (§10, §12).
- **How it ships:** PR 1 (CLI module registration + blueprint skeleton) is merged; PR 2 (all §4 entities, the ICD-10-CM/HCPCS loaders, and the `refresh-code-sets.ts` ETL shape), PR 3 (claim engine, three-layer scrubbing, and the real-CPT extension point, all mock-code-tested), and PR 5 (analytics API + a real RBAC fix — routes are now genuinely coder/biller-facing, with real IAM permission/role surfacing replacing the PR 1 placeholder) are all implemented. With claim submission now out of scope, the original six-PR plan collapses to PRs 1/2/3/5 plus a documentation/hardening pass (PR 6) — see §14 for the current breakdown.
- **Where to start:** §13 Immediate next steps.

---

## 1. Decision: how this gets built

Two ways to build this were considered — and the decision **flipped once** during planning, after new evidence changed the risk calculus.

| Option | What it means | Verdict |
|---|---|---|
| A. App-level service | A new service inside a separate consumer application, built with `forklaunch init service`, following the same architectural pattern as `blueprint/billing-base`/`blueprint/iam-base` but **not** registered as a reusable module in the CLI. | Superseded — see below |
| **B. First-class framework module** | Add `Module::BaseCac` (`cac-base`) to the Rust CLI, plus new `blueprint/interfaces/cac` and `blueprint/implementations/cac/base` packages, so any ForkLaunch user can run `forklaunch init module -m cac-base`. | **Chosen** |

**Naming: `cac`, not `medical-coding`.** Confirmed directly with the founder (Rohin) — "CAC" is the recognized health-information-management industry term (Computer-Assisted Coding), used consistently instead of the more generic `medical-coding` name this plan used in earlier drafts. Worth a quick gut-check before Phase 2 (§12, open item): "Computer-Assisted Coding" specifically denotes NLP/AI-suggested codes from clinical documentation industry-wide — this plan's scrubbing engine (§6) validates codes a human already entered, it doesn't suggest codes from text. If the name is meant as a forward-looking product category rather than a description of what Phase 0–4 actually builds, that's fine — just confirm that's the intent so the name doesn't quietly set an NLP-suggestion expectation nobody's built yet.

**Why it flipped.** Option A was originally chosen because Option B looked like a much larger, riskier lift — the CLI's `Module` enum was, at the time, only exercised by `billing`/`iam`, and touching ~12 exhaustively-matched Rust files for a brand-new module family was unproven territory. That assessment no longer holds. Since then, the framework itself added **two more first-class module families**: `messaging` (`messaging-base`/`messaging-twilio`, PR #264) and `ecommerce` (`ecommerce-stripe`). The messaging PR did this exact thing — "following the billing-base/billing-stripe pattern end to end" — with 346 Rust tests passing and the scaffold smoke-verified.

**Pre-implementation blocker check (done before committing to this rewrite):**
- **No structural blocker.** Every CLI match site the original research flagged — the `Module` enum, `validate_modules`, `get_service_module_name`/`get_service_module_cache`, `get_routers_from_standard_package`, package.json version constants, manifest boolean flags — is a clean, mechanical exhaustive-`match` extension, confirmed live in `cli/src/constants.rs` and `cli/src/core/modules.rs`, and now exercised twice more since the original research.
- **RBAC dependency is merged.** `hasPermissionChecks` (`framework/core/src/http/guards/hasPermissionChecks.ts`) is on `main` and wired into `auth.middleware.ts` — the RBAC design in §3 has what it needs.
- **One cautionary precedent, not a blocker, but a checklist item.** `ecommerce-stripe` was registered in the `Module` enum without finishing the rest of the pipeline — no template farm, no `ProjectDependencies` fields, no version constants, no `is_ecommerce` manifest flag. It's explicitly allowlisted as a *known, intentional gap* in a regression test (`every_module_variant_has_an_embedded_template_dir` in `cli/src/core/rendered_template.rs`), with a comment reading "do not add to this list." Every step in §3's checklist must be completed for `cac-base` — leaving it half-wired like `ecommerce-stripe` is the one concrete way to get this wrong.
- **Two local-machine prerequisites, not architectural blockers.** This dev machine has no Rust toolchain installed (`cargo` not found — needs `rustup`), and git symlinks aren't materializing on this Windows checkout (`core.symlinks` is `false` — confirmed by inspecting `cli/src/templates/project/messaging-base/bootstrapper.ts`, which is a 43-byte text file containing the target path string, not a real symlink to the blueprint content). Both need fixing before a local `cargo build`/`cargo test` reflects real template content. CI runs on Linux and is unaffected by either.

Given this, `cac-base` is built as a genuine first-class module, the same way `billing-base`/`iam-base`/`messaging-base` are. `framework/core`, `framework/express`, and `framework/common` still need **zero changes** — confirmed by direct inspection, same as before this pivot. The entire lift is the CLI (`cli/`) plus three new `blueprint/` packages, detailed in §3.

### Who is the customer, and do we hold a CPT license?

Resolved directly with the founder (Rohin), 2026-08-26. The question put to him: is `cac-base` a building block other startups (e.g. hospitals or the vendors serving them) use to build and own their own product — or are we building and operating a hospital-facing billing product ourselves, in which case *ForkLaunch* would need the CPT license? His answer: **Option A — we don't want to be in the licensing business.**

Concretely, that means:

- `cac-base` is a reusable module — like `iam-base` for authentication or `billing-base` for payments — that other companies use to build their own product, exactly the way the rest of this framework already works (§2).
- ForkLaunch never applies for, holds, or pays for an AMA CPT license, on anyone's behalf, at any point in this plan.
- The company adopting `cac-base` to build a real coding/billing product supplies its **own** CPT license and its own real CPT data. No patient or doctor data ever flows through ForkLaunch — each adopter deploys and operates their own instance of the module on their own infrastructure, under their own license, exactly like they would with `iam-base` or `billing-base` today.
- This supersedes every "our trigger to license CPT once a hospital confirms as a paying client" framing in earlier drafts of this plan — that premise no longer applies. See §2 for the full rewrite of the licensing posture, and §5 for what this means for the CPT extension point itself.

---

## 2. Business model and CPT licensing posture

**We are not in the CPT-licensing business, and don't plan to be (§1).** `cac-base` never bundles, stores, or distributes real AMA CPT content, and ForkLaunch never applies for a CPT license — not on our own behalf, and not on behalf of anyone using the module.

Everything except CPT itself is free/government-published, requires no license from anyone, and ships fully built into the module:

| Component | Source | Cost | Built by ForkLaunch? |
|---|---|---|---|
| ICD-10-CM (diagnosis codes) | CDC/NCHS + CMS | Free | Yes — fully built, ships in the module |
| HCPCS Level II (supplies/drugs/equipment) | CMS | Free | Yes — fully built, ships in the module |
| NCCI PTP + MUE edit tables (code-conflict rules) | CMS | Free | Yes — fully built, ships in the module |
| LCD/NCD (diagnosis-procedure medical necessity) | CMS (per-MAC) | Free | Yes, structurally — a *meaningful* real crosswalk needs real CPT codes to reference, which only exists once a customer wires in their own licensed data (§6) |
| CARC/RARC (denial reason codes) | X12 | Free to view | Yes — fully built, ships in the module |
| Eligibility / claim status (270/271) | Clearinghouse API | Per-transaction fee | Yes — fully built, ships in the module |
| **CPT (procedure codes)** | **AMA** | **Paid license** | **No — never held or distributed by ForkLaunch. The module ships a real, working extension point (`CodeSetProvider`); each adopting company supplies their own licensed data through it (§5).** |

**Who actually needs a CPT license, and when: it's never us.** CAC's real customers — companies building their own hospital/clinic/billing product on top of `cac-base` — are expected to already hold, or be independently pursuing, their own AMA CPT license before they wire up real CPT data. Which license type (end-user vs. distributor — see below) and what it costs depends entirely on *their* product and business model, not ours. This plan doesn't drive that relationship, doesn't apply for it, and doesn't budget for it.

**Why this changes how "CPT support" has to be built.** Earlier drafts of this plan treated real CPT as gated behind a future trigger ("our first paying hospital client") — reasonable when the plan assumed *we* would eventually hold the license. That assumption is gone. Because adopting customers already have their own license by the time they reach for this module, "does real CPT actually work" can't be a capability we finish later — the extension point has to be genuinely usable, documented, and tested now, for anyone plugging in their own licensed connector on day one. See §5 for exactly what that means concretely.

### Reference: AMA licensing timeline and pricing (for downstream builders, not ForkLaunch's own roadmap)

The following is research kept in this plan as a reference for whoever builds a real product on `cac-base` and doesn't yet hold a CPT license themselves — it is not a task on ForkLaunch's own roadmap, since ForkLaunch itself never goes through this process.

| Step | What happens | Time |
|---|---|---|
| 1. Apply | Submit CPT distributor license application at ama-assn.org | Same day |
| 2. AMA reaches out | Licensing specialist schedules a call | Within 10 business days |
| 3. Discovery call | Discuss product, user count, distribution model | 1–2 weeks after step 2 |
| 4. Quote & contract | AMA sends pricing + license agreement | 1–2 weeks |
| 5. Signed & active | Legally allowed to use real CPT codes commercially | — |

**Total: ~4–6 weeks.** Whoever is doing this licensing (the adopting company, not us) should budget the lead time against their own go-live date.

**License type and pricing (researched — refines the source PDF).** The AMA distinguishes two license categories: an **end-user/limited license** (for organizations *using* CPT internally, e.g. a single clinic's own EHR) and a **distributor license** (for a company embedding CPT content into a product it distributes/sells to others). A multi-hospital SaaS billing platform typically needs the **distributor license**, and — importantly — **a separate license is required per product** that contains CPT content, not one blanket company-wide license. Which category applies is a question for whoever is building the actual product — it depends on their own distribution model, not on `cac-base` itself.

Pricing is royalty-based and varies by product/user type rather than a single published rate card. Publicly reported reference points (vendor-reported, not an official AMA rate card — confirm exact numbers with an AMA licensing specialist on the discovery call):
- Roughly **$18–20 per clinician/user per year** for EHR/practice-management platforms.
- **~$0.24 per health-plan member per year** for payer-side use, with a **$100/year minimum royalty** on health-plan-model licenses.
- At least one reported vendor arrangement combining a flat annual royalty (~$1,050/yr) with a separate annual tooling fee (~$13,000/yr for "CPT Link" integration).

**Application inputs required (unchanged from the source PDF):** legal company info, a product description (what the platform does & how CPT is used), technical details (how CPT data is stored/secured), scale info (# hospitals/users, SaaS model), and an **AI/LLM disclosure** if applicable.

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
    cptCodeProvider.service.ts                     # reference/example adapter shape (§5) — takes
                                                     # any CptCodeSource; never contains real AMA
                                                     # content itself
    scrubbing.service.ts                           # three-layer engine, §6 — pure logic,
                                                     # no DB dependency, so it stays here
  domain/
    mockNcciRules.ts, mockLcdCrosswalk.ts          # mock PTP/MUE/LCD fixture data, §6
    cptShapedFixture.ts                            # synthetic CPT-shaped codes merged into the
                                                     # tables above — proves the engine is
                                                     # format-agnostic, §5 readiness bar item 4
  domain/schemas/{zod,typebox}/
  __test__/schemaEquality.test.ts, scrubbing.service.test.ts,
  scrubbing.cptShaped.test.ts, cptCodeProvider.test.ts
  README.md                                        # downstream-builder docs — ForkLaunch never
                                                     # holds real CPT content, here's how to wire
                                                     # your own licensed connector in (§5)

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
  services/
    claim.service.ts       # claim builder + scrubbing orchestration (§6) — as-built, this lives
                            # here rather than implementations/cac/base as originally sketched
                            # above: it needs the real entities, and unlike CodeSetProvider there's
                            # no swappable mock/real variant of "how a claim gets built" (cac has
                            # only one variant, §3's "Module variant count" note below)
    codeValidation.service.ts
    cptCodeSource.service.ts  # EntityManagerCptCodeSource — the ready-to-use CptCodeSource
                               # (§5), backed by the CptCode entity below. Same "needs real
                               # entities" reasoning as claim.service.ts for why it's here.
  bootstrapper.ts, registrations.ts, sdk.ts, server.ts, mikro-orm.config.ts
  scripts/
    enforce-retention.ts   # reuse framework's RetentionService, same as billing/iam
    refresh-code-sets.ts   # see §7, follows the enforce-retention.ts pattern exactly
```

This is copy-the-shape from `blueprint/billing-base` and, even more directly now, from `blueprint/messaging-base`/`blueprint/implementations/messaging/base` — the newest module the team actually shipped, so it's the freshest reference for exactly which files matter.

### CLI wiring checklist (mirrors PR #264's real diff, not a hypothetical) — implemented in PR 1

- **`cli/src/constants.rs`** — `Module::BaseCac` variant (`id: "cac-base"`, `exclusive_files: Some(&["cac-base"])`), plus match arms in `get_service_module_name` (→ `"cac"`), `get_service_module_description`, and `get_service_module_cache`.
- **`cli/src/core/modules.rs`** — `CacConfig` enum + `ModuleConfig.cac` field + a `Module::BaseCac` arm in `validate_modules()`.
- **`cli/src/core/template.rs`** — as-built, this returns `None`, not a router list. The `patient`/`encounter`/`claim`/`eligibility`/`remittance` routers don't exist as real controllers until Phase 1/2 add entities and business logic; declaring them now would have been metadata for routers that don't exist yet. Revisit this arm each phase as real routers land.
- **`cli/src/core/manifest/service.rs`** + `init/module.rs` + `init/application.rs` + `init/service.rs` — as-built, only `is_cac` was added — no `is_cac_configured`. That flag exists for IAM/billing specifically because *other* modules call into them; nothing calls into `cac` the way things call into IAM/billing, so the pattern doesn't apply.
- **`cli/src/core/package_json/project_package_json.rs`** + `package_json_constants.rs` — `ProjectDependencies` fields + version constants for `@forklaunch/interfaces-cac` / `@forklaunch/implementation-cac-base`, using the machine-readable comment format `check_blueprint_deps` expects.
- **`cli/src/templates/project/cac-base/*`** — git symlink farm into `blueprint/cac-base/*`, same mechanism as billing/iam/messaging.
- **`client-sdk`** — `cacSdkClient` gated into `clientSdk.ts`'s template via `{{#is_cac}}`, mirroring messaging's client-sdk gating.
- **`blueprint/.changeset`** — changesets for the new publishable packages (interfaces, implementation-base).
- **`docs/adding-projects/modules.md`** — row added to the module table.
- **`cli/tests/init_cac.sh`** — new CLI shell test, following `init_module.sh`/`init_messaging_twilio.sh`; e2e workflow glob extended so it actually runs.
- **The regression test does the rest for free.** `every_module_variant_has_an_embedded_template_dir` (`cli/src/core/rendered_template.rs`) fails loudly with the exact missing path if the template farm is incomplete.

**Module variant count.** Unlike `billing` (`-base`/`-stripe`) or `messaging` (`-base`/`-twilio`), `cac` gets **one** variant: `cac-base`. The mock-vs-real-CPT swap is an intra-service extension point (§5), not a module-level implementation choice — there's no second package to publish, and there never will be one we own, since the "real" side is always the customer's own connector, not a ForkLaunch-shipped alternative. This mirrors how `ecommerce` currently has only `ecommerce-stripe` with no `-base` counterpart.

### Multi-tenancy: hospital = Organization

The framework already has organization-scoped tenant isolation (`framework/core/src/persistence/tenantFilter.ts`, `rls.ts`) and it's used today in `iam-base` (`Organization` entity, `organizationId` on JWT session). We reuse this directly:

- Each hospital/clinic client == one `Organization` (from the existing IAM module).
- Every `cac-base` entity carries an `organizationId` (tenant) field, exactly like `iam-base`'s pattern of scoping `User` queries by `organization.id` in `blueprint/iam-base/api/controllers/user.controller.ts`.

### RBAC: coders, billers, admins

Reuse IAM's `Role`/`Permission` entities and the existing permission-guard machinery (`framework/core/src/http/guards/hasPermissionChecks.ts`, wired into `auth.middleware.ts`, confirmed merged to `main` — §1). Routes declare `allowedPermissions` / `allowedRoles` exactly like `iam-base`'s controllers do today.

**As-built (PR 5, extended by the §5 feature-gate work), the permission slugs differ from this section's original sketch** — `coder:submit_claim` and `biller:view_remittance` no longer fit now that claim submission/remittance are out of scope entirely (§8, §14). The real slugs in use: `coder:manage_claims` (`claim.controller.ts`), `biller:view_denials`/`biller:manage_denials` (`denial.controller.ts`, split read vs. write), `admin:view_analytics` (`analytics.controller.ts`, looser than the others — aggregate data, not PHI-bearing), and `admin:manage_codesets` (`codeSet.controller.ts`, retrofitted from internal/HMAC to protected/JWT so the per-organization feature gate in §5 knows which organization's `CodeSetLicense` to check).

- PHI-bearing read endpoints (patient demographics, claim detail) get stricter `allowedPermissions` than aggregate/analytics endpoints.
- A real adopter's IAM deployment needs to seed these slugs as actual `Permission` rows (the same way IAM's own migration seeds `platform:read`/`platform:write`, §3 "Migrations") before any of this is enforceable in practice — `cac-base` declares the requirement, it doesn't create the permission records.

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
| `Charge` | procedure code (real CPT or mock placeholder), units, amount, encounter | `none`; `units` matters for MUE checks (§6) |
| `Claim` | charges[], diagnoses[], status, payer | `phi` by relation to Patient |
| `Remittance` (ERA/835) | claim, paid amount, CARC/RARC codes | `none` |
| `Denial` | claim, CARC/RARC reason code, worklist status | `none` |
| `CodeSetLicense` | org, codeSetType (`cpt`), status (`none`/`pending`/`active`), signedAt | tracks the *adopting organization's own* license/connector status — not anything ForkLaunch holds; drives the extension-point feature gate, see §5 |
| `AuditLog` | actor, action, entity, timestamp | uses framework's existing `auditLogger.ts` — every PHI read/write |

**On SSN specifically:** storing a raw SSN maximizes PHI blast radius for minimal benefit — most billing workflows only need it for payer eligibility verification, not as a primary key. Recommendation: use an internal surrogate identifier (UUID or MRN) as `Patient`'s reference everywhere in the domain model, and store SSN only in the fields a specific payer integration actually requires, still `phi`-classified and encrypted. This is a direct application of HIPAA's "minimum necessary" principle, not just an encryption checkbox.

All entities get a `retention` policy via the same mechanism `RetentionService` (`framework/core/src/services/retentionService.ts`) already enforces for billing/iam — e.g. denial worklist records anonymized after N years per HIPAA §164.530(j), enforced by `scripts/enforce-retention.ts` exactly as the other modules do.

---

## 5. Code-set provider abstraction and the real-CPT extension point

This is the mechanism that makes "ForkLaunch never touches real CPT, but any customer with their own license genuinely can" safe and mechanical — the same interface-swap pattern `billing-base` already uses ("the real provider, Stripe, costs money and isn't always configured, so build against an interface and swap the implementation") applied here to procedure codes.

```
CodeSetProvider (interface)
 ├─ MockProcedureCodeProvider   — "PROC-001: Office Visit" — free, built-in, fully functional
 └─ CptCodeProvider             — reference/example adapter shape only — demonstrates how a
                                   customer's own real, licensed CPT connector plugs in;
                                   ForkLaunch never fills it with real AMA content
```

### What "genuine CPT support, built now" concretely means

Because adopting customers already hold their own license by the time they need this (§2), none of the following waits on a trigger — it ships as part of the same phase that builds the mock provider:

1. **`CodeSetProvider` and `MockProcedureCodeProvider` are complete and production-usable today** — this is what every adopter gets out of the box, with zero license required.
2. **The reference `CptCodeProvider` adapter is a complete, structurally-proven *shape*, not a stub — but it intentionally contains zero real AMA content.** It demonstrates exactly how to implement the interface against a real code-set feed; a customer either extends it or writes their own equivalent implementation pointed at their own licensed data. This must be documented loudly (package README, inline docs) so no adopter mistakes the shipped class for a working CPT dataset.
3. **`refresh-code-sets.ts` (§7) is generalized to ingest a CPT-shaped feed**, parameterized so a customer pointing it at their own real licensed data source is a config change on their side, not new engineering on ours. The exact delivery format any given customer's real feed uses (file drop, API, etc.) is unknowable in advance — the pipeline is built pluggable specifically because of that (§12).
4. **The three-layer scrubbing engine (§6) is tested against a synthetic, CPT-*shaped* fixture** — real CPT's actual numeric structure and code-range categories (Category I is 5-digit numeric, ranges like 10000–69990 for surgery, 70000–79999 for radiology, etc.; Category II is 4 digits + `F`; Category III is 4 digits + `T`) — not just against `MockProcedureCodeProvider`'s placeholder strings. This proves the NCCI PTP/MUE and LCD/NCD logic works against something structurally real without ever containing any of AMA's actual copyrighted code+description content, since ForkLaunch never has a license to hold that content in the first place (see §12's open question on who defines this fixture).
5. **Downstream-builder documentation is written**, explaining exactly how to implement a real `CodeSetProvider` against a customer's own licensed CPT feed, stating explicitly and unambiguously that ForkLaunch never holds or distributes real CPT content, and pointing to the reference adapter as a starting shape, not a working dataset.

**Feature gate — as-built, this is a plain resolve-and-branch, not `hasFeatureChecks`/`requiredFeatures`.** Earlier drafts of this section proposed reusing the framework's `hasFeatureChecks`/`hasSubscriptionChecks` guards (`framework/core/src/http/guards/`, the mechanism billing uses for entitlement gating) — but that mechanism *blocks* a route entirely when a required feature is missing, which is wrong here: real CPT is never a hard requirement for `buildClaim`/`scrubClaim`/`codeSet` endpoints to work at all, only for which data they use. Built instead as `CodeSetProviderResolver` (`cac-base`): given an organization id, it queries that org's own `CodeSetLicense` and returns either `MockProcedureCodeProvider` or a `CptCodeProvider` wired to that org's `EntityManagerCptCodeSource` — the `CodeSetProvider` interface itself is untouched, callers (`codeSet.controller.ts`) just resolve the right instance first, then call it normally.

- No connector wired up (no active `CodeSetLicense` for that org/`CPT`): the resolver returns `MockProcedureCodeProvider` — even though `CptCodeProvider`'s adapter shape is fully built and ready to be extended.
- Once the organization's own real connector is active and `CodeSetLicense.status = 'active'`: the resolver returns a `CptCodeProvider` for that organization, same scrubbing/claim logic runs unchanged.
- Knowing *which* organization is asking requires a real caller identity — `codeSet.controller.ts` was retrofitted from internal/HMAC to protected/JWT for this (§14 PR 5's RBAC pass covered the other routes; this one needed the same treatment specifically because of the feature gate).

This also gives a clean **per-organization** cutover: some organizations can stay on mock codes while others already have a real connector wired in, with zero risk of one tenant's status leaking into another (enforced by the same tenant-isolation filter used everywhere else).

**Failure mode (fail closed).** `CodeSetProviderResolver` looks up `CodeSetLicense` directly (a local table, no cross-service call needed) — if that lookup throws, the resolver catches it and falls back to `MockProcedureCodeProvider` rather than propagating the error. A stalled request is worse than a claim coded against mock data that can be re-submitted later — and, per the rule below, historical claims are never retroactively recoded anyway, so a transient mock-coded claim during an outage is not a special case to design around.

**Which CPT edition or vintage a customer's real feed uses is entirely their decision.** The extension point is edition-agnostic by design — built against CPT's structural shape, not any specific year's content — so it works the same regardless of which edition a customer's own license and data feed happen to be on.

**Historical claims are never retroactively recoded.** When an organization's `CodeSetLicense` flips to `active`, only *new* encounters created afterward use the real provider. Claims already built and submitted under `MockProcedureCodeProvider` remain exactly as they were coded — a submitted claim is a financial/legal record, and retroactively changing its procedure codes after the fact would itself be a compliance problem. This should be stated explicitly in the downstream-builder documentation (item 5 above), so any adopter can pass it along to their own end customers.

---

## 6. Claim scrubbing engine — four distinct rule layers

The source doc's phase plan referred to a single, vaguely-named "diagnosis-procedure necessity check" and "NCCI code-pair conflicts (ICD-10 side)." Research corrected this: **NCCI and medical-necessity checking are two unrelated mechanisms with different data sources**, and the original wording conflated them. The scrubbing engine needs four separate rule layers — the first three below plus a fourth, structural-completeness layer (added 2026-09-08, see the §9 update):

| Layer | What it checks | Code pairs involved | Data source | Update cadence | Typical denial if missed |
|---|---|---|---|---|---|
| **NCCI PTP** (Procedure-to-Procedure) | Two procedures billed together that shouldn't be, absent a justifying modifier | CPT/HCPCS ↔ CPT/HCPCS only — **never ICD-10** | CMS NCCI PTP edit tables | Quarterly | CO-97 ("benefit included in another service already adjudicated") |
| **NCCI MUE** (Medically Unlikely Edits) | Implausible unit count for a single code on one date of service (e.g. 5 appendectomies) | Single CPT/HCPCS code + units | CMS NCCI MUE tables | Quarterly | Line- or date-of-service-level unit denial |
| **LCD/NCD medical necessity** | Whether a diagnosis justifies a procedure at all | ICD-10-CM ↔ CPT/HCPCS — this *is* the real diagnosis-procedure crosswalk | CMS Medicare Coverage Database, per Medicare Administrative Contractor (MAC) — coverage is regional | Ongoing, MAC-specific | CO-50 ("not deemed a medical necessity") or CO-11 ("diagnosis is inconsistent with the procedure") |
| **Required fields** | Structural completeness — no diagnosis codes on the claim at all, a charge line missing its procedure code, or a non-positive unit count | Not a code-pair check — this is "is there enough on the claim to run the other three layers meaningfully at all" | N/A — internal validation, no external code-set dependency | N/A | CO-16 ("claim/service lacks information," paired with the specific missing field) |

Each mock procedure code built in Phase 2 needs a corresponding mock LCD-style mapping (which mock diagnoses justify which mock procedures) so the scrubbing logic and its test suite are exercising the real rule-layer shape — not a placeholder single check. Per §5's readiness bar, this same scrubbing engine also needs a second pass of tests run against the synthetic CPT-*shaped* fixture (real numeric code-range structure, no real AMA content) — the mock-placeholder tests prove the logic is correct, the CPT-shaped tests prove it survives contact with real-shaped data before any customer ever wires in their own real feed.

**A subtlety §2's free/paid table doesn't fully capture: real LCD/NCD data is coupled to real CPT, even though it has no license of its own.** LCD/NCD data itself is free CMS data (§2 correctly marks it "Built by ForkLaunch"), but every real LCD policy is written *in terms of real CPT/HCPCS codes* — "procedure X is covered for diagnoses A, B, C" only means something once "procedure X" is a real CPT code, not a mock placeholder. So while the LCD/NCD *data* can be downloaded and stored without any license, a *meaningful* real LCD crosswalk can't exist until whichever customer wires in their own real, licensed CPT connector — and that ingestion is their own responsibility once they do, not a milestone on ForkLaunch's roadmap (§10's Phase 6 note).

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

**Refresh mechanism — no framework-native cron exists (confirmed by codebase inspection):** `framework/implementations/worker/{bullmq,kafka,redis,database}` expose only `enqueueJob`/`enqueueBatchJobs`/`start` — no repeat/cron primitive is surfaced anywhere, even though BullMQ itself supports one internally. The one real precedent in this codebase is `scripts/enforce-retention.ts`, wired to a plain `"retention:enforce"` npm script that an external scheduler (k8s CronJob / cloud scheduler) invokes — there's no in-repo trigger. `cac-base` should follow this exact convention: a `scripts/refresh-code-sets.ts` + `"codeset:refresh"` npm script, invoked externally on a schedule matched to the table above (the tightest cadence — HCPCS/NCCI quarterly — sets the polling interval). CPT's own cadence (annual/semiannual) becomes each adopting customer's own operational concern once they wire their real connector in (§5) — this pipeline's job is only to make that a config change for them, not new ETL work.

**Scrubbing lookups must be cached, not per-line queries.** The §6 scrubbing engine checks NCCI PTP/MUE and LCD/NCD edit tables per claim line, and these tables run into the tens of thousands of code pairs. A naive per-line DB query against them is an N+1 risk on multi-line claims at any real submission volume. Scrubbing should check these tables against an in-memory or Redis-cached lookup keyed by the active code-set version, refreshed each time `refresh-code-sets.ts` runs on its quarterly cadence — not queried fresh per claim line. Demo-scale (Phase 2–4) volume won't expose this, but it's cheap to design correctly now versus retrofitting a caching layer onto a scrubbing engine that already shipped with naive queries.

**Bulk loading needs a dedicated ETL step, not the seeder pattern.** The existing `persistence/seeders/*.seeder.ts` + `seed.data.ts` pattern (e.g. `blueprint/billing-base/persistence/seeders/plan.seeder.ts`) is a thin wrapper that does one `em.create(...).flush()` per hand-written object literal — clearly sized for a handful of config rows, not the ~70,000 ICD-10-CM codes or ~7,000 HCPCS codes. `framework/infrastructure/S3`'s `S3ObjectStore` is closer but its `putBatchObjects` is just `Promise.all` over individual JSON puts, not a bulk-CSV loader either. `refresh-code-sets.ts` should instead: stream the government-published CSV/XML (staged in S3), parse it, and batch-insert via MikroORM in chunks (e.g. 1,000 rows per `em.persist(...).flush()`) — a purpose-built ETL script, following the *shape* of `enforce-retention.ts`'s batching loop (`framework/core/src/services/retentionService.ts` already batches at 1,000 records/flush for exactly this reason) but against a new code-set-specific service, not a reused generic primitive.

---

## 8. EDI transaction sets and clearinghouse choice — out of scope for `cac-base` (reference only)

**Superseded by the founder's scope call (2026-08-31, see Executive summary).** This section's original research — which transaction sets a claim submission pipeline needs and which clearinghouse to use — assumed `cac-base` would submit claims and receive remittances on an adopter's behalf. That's no longer the plan: `cac-base`'s job stops at the scrubbing report (§6). Building a real submission/remittance pipeline, if an adopter wants one, is *their* engineering effort, on top of whatever clearinghouse relationship they bring — not something ForkLaunch builds into this module. This section is kept as reference material for that adopter, not as ForkLaunch's own roadmap (the same treatment §2 already gives the AMA licensing timeline).

**Transaction sets a downstream builder would need** (confirmed standard for a hospital billing platform, if they build submission themselves): **837** (claim submission — P/I/D variants), **835** (ERA/remittance), **270/271** (eligibility inquiry/response), **276/277** (claim status inquiry/response), **277CA** (claim acknowledgment — front-end edit summary on the 837), and **999** (functional acknowledgment, the 5010 replacement for 997). Given hospital utilization review needs, also plan for **278** (prior authorization/referral) even though the source doc didn't call it out.

**Version:** the HIPAA-mandated version remains **X12 005010** — a proposed 008020 update was declined by NCVHS in 2023, and while X12 has since published an 008060 guide as a forward candidate, nothing is mandated yet. Build against 5010 with **CAQH CORE operating-rule compliance** (e.g. its 835 Code Combinations rules, eligibility/claim-status response-time rules) as the near-term target — don't design for a version bump that hasn't been mandated.

**Clearinghouse research, for whoever builds this themselves:** **Stedi** was the researched pick for a primary sandbox/integration — it's the only clearinghouse of the three that's API-first, accepting/returning JSON rather than raw X12 for 837/270/271/276/277/835, with a permanently free sandbox and pure usage-based pricing (no monthly minimum). **Claim.MD** as a lower-cost secondary/fallback (REST+XML, ~$0.10–0.25/claim or ~$100/month unlimited) for redundancy. **Availity** has the broadest payer network but is architected EDI/portal-first with weaker self-serve API access than the other two.

---

## 9. Compliance / HIPAA posture

We are not starting from zero — `COMPLIANCE_COVERAGE.md` shows the framework already addresses 34/43 cross-standard (HIPAA/SOC2/PCI/GDPR) requirements at the framework layer, including the exact things a PHI-handling service needs: field-level data classification + PHI encryption at rest, tenant isolation, access control + audit logging, automatic session logoff, right to erasure, data portability, and data retention/disposal.

Confirmed by direct code inspection and by `COMPLIANCE_GAPS_PLAN.md`'s own framing — every remaining framework gap is "no new framework primitives needed," meaning the compliance layer is intentionally module-agnostic. Concretely, for free, with zero extra code in `cac-base`:

- Any entity field marked `.compliance('phi')` on a `defineComplianceEntity()` is **auto-encrypted** (AES-256-GCM, per-tenant HKDF-derived key) via `EncryptedType`/`FieldEncryptor`.
- `forklaunch init service`'s generic router template already generates a `compliance.controller.ts` exposing `DELETE /erase/:userId` and `GET /export/:userId` (HMAC-protected, internal-only), backed by `ComplianceDataService` — GDPR-style per-patient erase/export for free.
- `scripts/enforce-retention.ts` + `RetentionService` batches delete/anonymize per entity's `retention` policy.
- Tenant isolation (`tenantFilter.ts` + `rls.ts`) activates automatically for any entity with an `organizationId`/`organization` relation.

Gaps called out in `COMPLIANCE_GAPS_PLAN.md` (consent management, pen testing, DR testing) are framework/CLI-level and orthogonal to this module. The **new** thing this module needs that doesn't exist yet is the `CodeSetLicense` entity/feature-gate itself (§5) — everything else is reuse.

Since ForkLaunch never operates a hospital-facing product itself (§1, §2), the "before any real hospital data touches the system, run an external security review" step is each adopting company's own responsibility for their own deployment — not a ForkLaunch operational task.

### Test/QA strategy (confirmed against existing test conventions — fills a prior gap)

`billing-base`'s tests (`__test__/test-utils.ts`, `plan.test.ts`) use `BlueprintTestHarness` from `@forklaunch/testing`, backed by **real `testcontainers`** (Postgres + Redis) — not mocks, not in-memory sqlite. Test data is seeded via real MikroORM entities, and assertions call the generated route SDK in-process (`route.sdk.createPlan({...})`) rather than raw HTTP. This directly answers the previously-open question of how to validate the mock→real-connector cutover: **spin up the real containerized test DB, seed a representative subset of both mock and structurally-real code pairs (including at least one of each CARC scenario in §6's table), and assert against the actual `sdk.*` calls end-to-end** — not a mocked unit test — as the reference test suite any adopter can run before flipping `CodeSetLicense.status` to `active` for their own real organization.

**Required test matrix (one row per scrubbing scenario from §6):**

| Scenario | Layer exercised | Test file | Asserts |
|---|---|---|---|
| Two procedures billed together without a justifying modifier | NCCI PTP | ✅ `scrubbing.service.test.ts` + `claimFlow.e2e.test.ts` | Claim rejected pre-submission with CO-97-equivalent internal denial code |
| Implausible unit count for a single code/date-of-service | NCCI MUE | ✅ `scrubbing.service.test.ts` + `claimFlow.e2e.test.ts` | Claim rejected with a unit-level denial, valid unit counts pass |
| Diagnosis doesn't justify the procedure (mock LCD-style crosswalk) | LCD/NCD medical necessity | ✅ `scrubbing.service.test.ts` + `claimFlow.e2e.test.ts` | Claim rejected with CO-11/CO-50-equivalent, covered diagnosis-procedure pairs pass |
| Missing required claim field | Required-fields scrubbing | ✅ `scrubbing.service.test.ts` + `claimFlow.e2e.test.ts` | Claim rejected with CO-16-equivalent, the specific missing field named in the finding's message |
| ~~Eligibility check fails at intake (coverage terminated)~~ | ~~Eligibility (270/271)~~ | ~~`eligibility.test.ts`~~ | **Out of scope (§8, §14) — `cac-base` never performs eligibility checks itself.** Row kept for the historical record. |
| `CodeSetLicense` flips to `active` mid-organization-lifecycle | Mock→real cutover (§5) | ✅ `claimFlow.e2e.test.ts` ("code-set cutover") | **Regression test:** claims submitted under `MockProcedureCodeProvider` before the flip are byte-for-byte unchanged after the flip; only claims created after the flip use the real provider |
| License-check lookup fails/throws | Feature-gate fail-closed (§5) | ✅ `codeSetProviderResolver.test.ts` (unit, fake `EntityManager` throwing) + `claimFlow.e2e.test.ts` ("code-set feature gate", happy path against a real DB) | Organization falls back to `MockProcedureCodeProvider` rather than blocking claim submission — row description corrected: the lookup is a local `CodeSetLicense` table read via MikroORM, not a call to IAM as originally described |

Each row is a full `testcontainers` end-to-end test per this section's harness, not a mocked unit test — the scrubbing engine's correctness is exactly the kind of logic where a passing mock and a failing production query diverge.

**Implemented (2026-09-08): `cac-base/__test__/e2e/claimFlow.e2e.test.ts`.** Real Postgres + Redis via `BlueprintTestHarness`, real migrations, the real app (matching `server.ts` exactly — `forklaunchExpress` + `setupTenantFilter`/`setupRls` + real `surfacePermissions`/`surfaceRoles`), real HTTP calls with a real self-signed JWT and a tiny in-process IAM stub. Covers all four scrubbing layers (NCCI PTP, NCCI MUE, LCD/NCD, required-fields — including the PTP negative case and both required-fields cases: zero diagnoses and an invalid unit count), the full build→scrub→worklist→resolve→analytics path, auth (missing token, missing permission), tenant isolation, and the code-set feature gate/cutover (§5). Every row in this section's test matrix now has real coverage. Landing this suite immediately surfaced three real, previously-undetected bugs on its first run, none caught by the existing pure-logic unit tests (which exercise `ScrubbingService` directly against in-memory data, never a live DB round-trip or the real HTTP/auth chain):

- **`Diagnosis.icd10Code`** silently mapped to DB column `icd10code` (no underscore) at runtime, while the hand-written migration created `icd10_code` — every scrub read zero diagnosis codes back, regardless of what was actually diagnosed. Fixed on the entity side (`.fieldName('icd10_code')`).
- **`Patient.dateOfBirth`** — PHI-compliance encryption always produces ciphertext (a string), but the migration created the column as `timestamptz` — every patient insert failed outright. Fixed on the migration side (`text`, not `timestamptz`).
- **No route filtered by organization at all.** `DenialWorklistService`/`ClaimService`/`AnalyticsService` never took an `organizationId` parameter, and no controller ever set the DI-scoped tenant context — so the framework's own automatic MikroORM tenant filter (which fails *open*, by its own design, when no tenant context is set) never filtered anything. A valid token for one organization could read and resolve another organization's claims and denials. This is the most serious of the three — a real cross-tenant PHI exposure, not a schema mismatch. Fixed by threading `organizationId` explicitly through all three services and every controller (the same pattern `CodeSetProviderResolver`/`codeSet.controller.ts` already used correctly) — the framework's automatic tenant filter was never load-bearing here and still isn't; explicit per-query filtering is what actually protects this data.

**Implemented (2026-09-08): required-fields scrubbing itself, not just its tests.** Until this point `ScrubbingService` only had three layers (NCCI PTP/MUE, LCD/NCD) — `required_fields` existed solely as a reserved `DenialReasonCategory` enum value and a CO-16 reference-table row, with no logic anywhere producing it. Added as the fourth layer described in §6: a claim with zero diagnosis codes, or a charge line with an empty procedure code or a non-positive unit count, now gets a `CO-16`/`required_fields` finding naming the specific problem (e.g. `"Claim has no diagnosis codes"`, `"Charge line 1 has an invalid unit count (0); must be a positive integer"`). This closes a real gap the mock crosswalk alone couldn't catch: a claim with an uncrosswalked procedure code and zero diagnoses previously scrubbed clean, since NCCI PTP/MUE only examine codes that are present and LCD/NCD only fires for a procedure that has a crosswalk entry at all. 4 new unit tests in `implementations/cac/base/__test__/scrubbing.service.test.ts` (22/22 passing) and 2 new e2e cases in `claimFlow.e2e.test.ts` (12/12 passing).

**Implemented (2026-09-08): the code-set cutover itself is now real, not just describable.** Before this, `CodeSetProviderResolver` was only ever called from `codeSet.controller.ts`'s standalone `describe`/`lookup` endpoints — `ClaimService.buildClaim` never consulted it or `CodeSetLicense` at all, so the "byte-for-byte unchanged, only new claims use the real provider" cutover guarantee this section's test matrix promised had no code path to test. `Claim` gains a `codeSetType` field (`'mock' | 'cpt'`, defaulting to `'mock'`); `buildClaim` now resolves the organization's `CodeSetProviderResolver` result once, at build time, and stores it on the claim — `scrubClaim` never re-resolves it, so a later `CodeSetLicense` flip can never retroactively change an already-built claim's own history, matching §5's explicit rule. The `POST /claim/build` response now includes `codeSetType`. 4 new e2e tests in `claimFlow.e2e.test.ts` (16/16 passing): the feature gate's real-DB happy path (mock → cpt once a license is active, complementing the existing fake-`EntityManager` unit tests' fail-closed coverage), and the cutover itself — a claim built before a license activation stays `'mock'` after it while a new claim built after picks up `'cpt'`, plus a tenant-isolation variant (one organization's license activation never affects another's claims).

---

## 10. Validation UI in `forklaunch-platform` (free code sets only)

**Purpose:** give a non-engineer (the founder, a doctor contact, a prospective early adopter) a real, clickable way to see the module's free-code-set behavior work — ICD-10-CM + HCPCS-based claim building and the three-layer scrubbing engine (§6) — without reading code or standing up a local dev environment.

**Where it lives, and why:** the separate `forklaunch-platform` repository (ForkLaunch's own internal SaaS platform product), not `forklaunch-js`. `cac-base` itself ships headless and API-only, like every other ForkLaunch module — it never gets its own UI. A validation UI belongs in the platform product that already exists to host UI surfaces, not bolted onto the module.

**Scope — explicitly free codes only, no CPT.** The UI validates ICD-10-CM diagnosis codes plus mock-procedure-code claim building against `MockProcedureCodeProvider`, and exercises all three scrubbing layers (NCCI PTP, NCCI MUE, LCD/NCD-style necessity). It does **not** touch real CPT — there is nothing for ForkLaunch to validate there without a customer's own license and real data (§2), and validating our own hypothetical real-CPT behavior is explicitly out of scope for an internal tool.

**Basic shape (matches the plain-language validation-plan example already shared with the team):**
- **User provides:** a diagnosis code (or picks one from a short seeded list), a mock procedure code, and a unit count.
- **System returns:** whether the claim would pass or get flagged — and if flagged, which layer caught it (NCCI PTP / NCCI MUE / LCD-style necessity) and the matching mock denial code (CO-11/CO-50/CO-97/etc., §6).

**Status: not yet started.** `forklaunch-platform`'s own structure (its `.forklaunch/manifest.toml`, `src/modules`, `data-proxy`) hasn't been explored in the context of this task — that exploration is the first step before any UI work begins there (§13), and it's separate scope from this repo's own PR sequence (§14), tracked and built in that repository instead.

---

## 11. Success metrics (reference benchmarks, not ForkLaunch's own product metrics)

Since ForkLaunch doesn't operate a hospital-facing billing product itself (§1, §2), these describe what the scrubbing engine and claim pipeline should be *capable of* producing for whoever builds a real product on `cac-base` — reference benchmarks for downstream builders, benchmarked against industry data, not metrics ForkLaunch tracks internally.

| Metric | Trial baseline | Plan target | Industry benchmark context |
|---|---|---|---|
| Clean claim rate | 60% | 95%+ | Matches MGMA's "good performance" benchmark (~95%); industry median is often 85–90%. **Credible, appropriately aggressive.** |
| Denial rate | 30% | Under 5% | HFMA considers <5% "optimal," but industry average is 5–10% and initial denial rates were ~11.8% in 2024, trending toward 12–15%. **This is a best-in-class target, not a typical baseline — it will require real scrubbing-engine automation (§6), not just clean data entry.** |
| ~~Average days to payment~~ | ~~24 days~~ | ~~Under 40 days~~ | **Dropped (2026-08-31) — not just deferred.** This needs real remittance timing (when a claim actually gets paid), which requires the submission/remittance pipeline that's now out of scope entirely (§8, §14). Row kept for the historical record. |

**Clean claim rate and denial rate are the two metrics `AnalyticsService` (§14 PR 5) actually reports** (`GET /analytics/claims/summary`) — computed from `Claim`/`Denial` data `cac-base` owns outright, not aspirational.

---

## 12. Open questions

1. **IAM cross-service integration** — does an adopter's staff (coders/billers) get provisioned in the *existing* `iam-base` module as `User`s with new `Role`s, or does this need its own lightweight staff directory? (Recommend: existing IAM — the cross-service SDK mechanism in §3 makes this straightforward, and avoids duplicating auth.)
2. **CO-11/CO-50 documentation honesty** — the downstream-builder documentation (§5, item 5) must clearly disclose that LCD/NCD-style medical-necessity checks run against a mock diagnosis-procedure crosswalk until a real licensed CPT+LCD feed is wired in — this is documentation scope now, not sales messaging, since ForkLaunch doesn't run its own demos to hospitals.
3. **Mock LCD/NCD data source and synthetic CPT-shaped fixture design — built, needs SME sign-off.** Both (a) the mock diagnosis-procedure crosswalk (`mockLcdCrosswalk.ts`) and (b) the synthetic CPT-*shaped* fixture (`cptShapedFixture.ts`) now exist and are exercised by the scrubbing-engine tests, but were designed by engineering in this pass rather than a coding/compliance SME. Treat both as structural test fixtures only, not authoritative, until an SME reviews them — (b) in particular needs sign-off that it doesn't inadvertently reproduce anything resembling AMA's actual copyrighted code+description content.
4. **MAC jurisdiction scope for real LCD ingestion** — this is now each adopting customer's own concern once they wire in their real CPT+LCD feed (§6, §10), not something ForkLaunch resolves. Worth a clear line in the downstream documentation pointing out that LCD coverage is regional, so adopters know to scope it themselves.
5. **Phase timeline validation** — the Phase 0–4 week estimates (§11) come from the source doc, not from this team's actual velocity. Needs a sizing session with whoever will staff this.
6. ~~Multi-org billers~~ — **Resolved (2026-09-02): no cross-org access pattern needed.** Confirmed that billing setups genuinely differ per hospital — each hospital's coding/billing configuration (code-set licenses, rules, staff) is its own, not shared across a third-party billing company's client roster. §3's existing model (each hospital/clinic as one `Organization`, RBAC and tenant isolation scoped to a single org per user, matching `iam-base` exactly) is therefore correct as designed — a coder/biller sees one organization's data, full stop. No cross-org visibility feature to build.
7. **Real CPT data delivery mechanism** — no longer ours to discover directly, since ForkLaunch never applies for a license or talks to AMA (§2). The extension point (§5) should stay agnostic to file-drop vs. API vs. any other delivery shape, since different adopters' real feeds may take different forms and we can't know which in advance.
8. ~~Does `cac-base` need its own env vars / Redis dependency?~~ **Half-resolved (2026-09-01).** IAM permission/role caching is done: `surfacing.ts`'s `createSurfacePermissions`/`createSurfaceRoles` now check/populate an `AuthCacheService` (5min TTL, Redis-backed via `RedisTtlCache`) before calling IAM, so a repeat request from the same user within the window skips the round-trip — `REDIS_URL` and a `TtlCache` registration were added to `registrations.ts`, matching `billing-base`'s existing pattern exactly. Scrubbing-rule caching (the other half of the original item) is deliberately **not** built: `ScrubbingService.scrub()` is synchronous, pure, in-memory logic against the mock rule tables (`mockLcdCrosswalk.ts`/`mockNcciRules.ts`) — there's no external I/O to cache yet, and wrapping an in-memory lookup in a Redis round-trip would make it slower, not faster. Revisit once the real CPT-connector-backed rule source (§5) replaces the mocks and rule lookups become genuine I/O.
9. **Does the "CAC" name imply NLP-based code suggestion?** (§1) Industry-wide, Computer-Assisted Coding means software that suggests codes from clinical documentation via NLP/AI — this plan's scope (§6's scrubbing engine) validates codes already entered, it doesn't suggest them. Confirm with the founder whether `cac-base` is meant to grow into that capability later, or whether it's just the industry-standard label applied to a validation/billing engine.
10. **`forklaunch-platform` validation UI scope** (§10) — hosting, auth model, and whether it needs its own module in that repo's manifest or is a lightweight standalone page are all unresolved; first thing to scope once that repository is actually explored.
11. ~~Per-organization real-CPT feature gate not wired up yet~~ — **done, but not via `hasFeatureChecks`/`requiredFeatures` as this item originally proposed.** That mechanism *blocks* a route entirely when a required feature is missing (correct for billing's "you need this plan to access this endpoint"), but real CPT is never a hard requirement here — an org without a license should transparently keep getting mock data, not a 403. Built as a plain resolve-and-branch instead: `CodeSetProviderResolver` (`cac-base`) queries the requesting organization's `CodeSetLicense` and hands back either `MockProcedureCodeProvider` or a `CptCodeProvider` wired to that org's own `EntityManagerCptCodeSource`, with the `CodeSetProvider` interface itself untouched. Replaces the old always-mock `CodeSetProvider` Singleton. `codeSet.controller.ts` was retrofitted from internal/HMAC to protected/JWT as part of this — resolving per-organization requires knowing *which* organization is asking, which an HMAC service-to-service call can't tell you. Fails closed per this section's own framing: a `CodeSetLicense` lookup error falls back to mock, never blocks the request.
12. ~~Does the module still want a denial worklist API, now that PR 4 is removed?~~ — **Resolved: yes, built.** `DenialWorklistService` + `GET /denial`, `GET /denial/:id`, `POST /denial/:id/resolve` (`cac-base`), plus a `WorklistStatus` enum (`open`/`resolved`) formalizing the field `ClaimService` was already writing as a raw string. Pure query/status layer over the `Denial` rows PR 3's scrubbing engine creates — no Stedi dependency, none introduced. See §14.
13. **`Remittance` entity (§4) is now speculative, not load-bearing.** It was built in PR 2 as part of the bulk entity delivery, sized for real 835 parsing (§8) — which is now out of scope. It's harmless to leave as-is (an adopter could still use it to log outcomes from their own submission pipeline), but nothing in `cac-base` writes to it, and no PR currently plans to. Revisit whether it should be trimmed, repurposed, or left as an unused convenience for downstream builders.
14. ~~`@forklaunch/implementation-cac-base`'s `./services` export was broken at runtime since PR 1~~ — **found and fixed while verifying §5's feature gate.** `package.json`'s `exports["./services"]` pointed `import`/`require`/`default` at `./lib/services/index.js`, but the build script's single tsup entry point (`services/index.ts`) actually outputs a flat `./lib/index.js` — tsup only preserves the source directory structure when given multiple entries (compare `implementations/messaging/base`'s 5-entry build, which does nest correctly). Every `tsc --noEmit` check throughout this whole effort passed regardless, because TypeScript resolves workspace packages through their `.d.ts`/source info, not Node's real ESM/CJS `exports` resolution — so this went undetected until something (this session's new `codeSetProviderResolver.test.ts`) actually tried to `import` the built package at runtime. Fixed by pointing `import`/`require`/`default` at the real `./lib/index.js`/`./lib/index.mjs` output; `types` was already correct, since `tsc --emitDeclarationOnly`'s output does preserve source structure regardless of tsup's bundling. **Swept the other 12 `implementations/*` packages (2026-09-01): none share the bug.** Every one of them (`billing/{base,stripe}`, `ecommerce/{base,paypal,stripe}`, `iam/base`, `messaging/{base,twilio}`, `worker/{bullmq,database,kafka,redis}`) is built with *multiple* tsup entry points, each mapping 1:1 to an exports subpath — `implementation-cac-base` was the only package with a single tsup entry point (`services/index.ts`) declaring a nested exports path, which is what triggered the flattening. Verified empirically, not just by inspection: built `billing/base` directly with tsup and confirmed its output actually lands at `lib/services/index.js`, `lib/domain/schemas/index.js`, etc. — matching its exports map, no fix needed.

---

## 13. Immediate next steps

1. ~~Register `Module::BaseCac` in the CLI and scaffold the three blueprint packages~~ — **done.** PR 1 merged.
2. ~~Rewrite this plan to reflect the resolved business model (§1, §2)~~ — **done, this revision.**
3. ~~Finalize the `cac-base` entity schema (§4)~~ — **done.** All §4 entities implemented (compliance-classified via `defineComplianceEntity`, tenant-scoped via `organizationId`), including the SSN/surrogate-ID decision (`Patient.mrn` as the internal reference, `ssn` nullable/`phi`).
4. ~~Extend the ICD-10 loader to HCPCS; stand up the `refresh-code-sets.ts` ETL shape (§7)~~ — **done.** `CodeSetLoaderService` (generic batch upsert) + `Icd10Code`/`HcpcsCode` reference tables + `loadIcd10Codes`/`loadHcpcsCodes` + the `refresh-code-sets.ts` entrypoint script are implemented and unit-tested. Pointed at local CSV fixtures today (`ICD10_SOURCE_PATH`/`HCPCS_SOURCE_PATH`) — wiring a real CMS/CDC feed (or S3) is a follow-up config change, not new ETL code, per §7.
5. Explore `forklaunch-platform`'s structure and scope the validation UI (§10, §12 item 10) — not yet started.
6. Run a sizing session against §11's phase estimates with whoever will actually staff this, before quoting any timeline externally (§12, item 5).
7. ~~Create a Stedi sandbox account and request API credentials (§8)~~ — **removed, not deprioritized.** The founder confirmed (2026-08-31) that `cac-base` never submits claims or handles remittances on an adopter's behalf (Executive summary, §8) — there's no longer a phase this would unblock. Only relevant again if that scope decision itself is revisited.
8. ~~Get a coding/compliance SME to define the mock LCD/NCD crosswalk and the synthetic CPT-shaped fixture (§12, item 3)~~ — **superseded.** Engineering built both (`mockLcdCrosswalk.ts`, `cptShapedFixture.ts`) rather than waiting; still needs SME sign-off before being treated as authoritative (§12 item 3).
9. ~~Build the scrubbing rules engine against `MockProcedureCodeProvider`, implementing all three rule layers from §6, and build the real-CPT extension point's reference adapter and downstream documentation to genuine readiness (§5)~~ — **done.** `ClaimService` + `ScrubbingService` (all three layers, mock codes) and `CptCodeProvider` + `EntityManagerCptCodeSource` + `CptCode` entity + downstream-builder README are implemented. Remaining: wire the per-organization runtime feature gate (§12 item 11).
10. ~~Decide whether a standalone denial-worklist API (§12 item 12) is worth building~~ — **done.** Built; see §12 item 12, §14.

---

## 14. PR breakdown

Originally one PR per phase, six PRs total. **PR 4 (eligibility & remittance) is now removed, not just deferred** — the founder confirmed (2026-08-31) that `cac-base` never submits claims or handles remittances on an adopter's behalf; it stops at the scrubbing report (Executive summary, §8). Kept in this table struck through, for the historical record, rather than silently deleted. Numbering (PR 5, PR 6) is left as-is rather than renumbered, to avoid breaking references elsewhere in this doc.

| PR | Phase | Scope |
|---|---|---|
| PR 1 | Phase 0 | **CLI module registration + blueprint package skeleton (§3) — implemented and merged**, as two commits (CLI wiring, then the three blueprint packages). HIPAA hosting/BAA and synthetic test dataset are business/ops tasks still outstanding, not engineering |
| PR 2 | Phase 1 | **Code validation — implemented.** All §4 entities (+ `Icd10Code`/`HcpcsCode` reference tables), a hand-written schema migration, the `CodeSetLoaderService` ETL shape with ICD-10-CM/HCPCS loaders, `refresh-code-sets.ts`, and free-code-set validation endpoints (`GET /codeValidation/icd10/:code`, `/hcpcs/:code`) — the surface §10's `forklaunch-platform` UI calls |
| PR 3 | Phase 2 | **Claim engine + three-layer scrubbing implemented** (mock codes only) — `ClaimService` (`cac-base`) builds a claim from an encounter's charges/diagnoses and runs it through `ScrubbingService` (`implementations/cac/base`, pure logic, unit-tested), which checks NCCI PTP conflicts, NCCI MUE unit caps, and LCD/NCD-style medical necessity against mock fixture data, persisting `Denial` rows and updating `Claim.status`. New `POST /claim/build` and `POST /claim/:id/scrub` endpoints — this **is** the report the founder's scope call describes; there's nothing further to build here toward submission. **Real-CPT extension point also implemented:** `CptCodeProvider` (reference adapter, `implementations/cac/base`) + `EntityManagerCptCodeSource`/`CptCode` entity (the ready-to-use concrete source, `cac-base`) + `loadCptCodes`/`CPT_SOURCE_PATH` wiring into `refresh-code-sets.ts` + a synthetic CPT-shaped fixture merged into the same scrubbing-rule tables the mock data uses, proving the engine is format-agnostic — plus a downstream-builder README (`implementations/cac/base/README.md`) stating explicitly that ForkLaunch never holds real CPT content. The fixture was designed by this session rather than a coding/compliance SME (§12 item 3) — flagged as needing that review before being treated as more than a structural test fixture. The per-organization runtime feature gate mentioned as outstanding here was later built — see §12 item 11 and §5. Also deferred: the full `testcontainers` end-to-end test matrix from §9 — the pure-logic unit tests on `ScrubbingService` cover the same branching logic without a DB round-trip |
| ~~PR 4~~ | ~~Phase 3~~ | ~~Eligibility & remittance — 270/271, 835, 277CA/999~~ **Removed (2026-08-31) — out of scope, not deferred.** `cac-base` never submits claims or parses remittances on an adopter's behalf; that's their own downstream engineering. **The denial worklist piece was salvaged and built separately** (not tied to any phase number): `DenialWorklistService` + `GET /denial`, `GET /denial/:id`, `POST /denial/:id/resolve` — list/view/resolve the `Denial` rows PR 3's scrubbing engine already creates, with a new `WorklistStatus` enum. No Stedi dependency |
| PR 5 | Phase 4 | **Implemented.** Analytics API — `AnalyticsService` + `GET /analytics/claims/summary`, reporting clean-claim-rate and denial-rate (with a per-category breakdown) over scrubbed claims, optionally date-ranged. Average-days-to-payment dropped from §11's three metrics — it needs real remittance timing, out of scope entirely (§8, §14). **RBAC verification pass found and fixed a real gap:** `claim`/`denial` routes were `access: 'internal'` + HMAC-only (service-to-service, no user identity) despite §3's original intent that they be coder/biller-facing with real permissions — retrofitted to `access: 'protected'` + JWT + `allowedPermissions` (`coder:manage_claims`, `biller:view_denials`/`biller:manage_denials` — renamed from §3's original `coder:submit_claim`/`biller:view_remittance` slugs, which no longer fit now that submission/remittance are out of scope). The new `admin:view_analytics` permission gates the analytics endpoint, looser than the claim/denial ones per §3's "aggregate/analytics endpoints get looser permissions than PHI-bearing ones" framing. **Also fixed: `surfacePermissions`/`surfaceRoles` in `server.ts` were still the hardcoded placeholder from PR 1** (always returning `PLATFORM_READ`/`ADMIN` regardless of who was asking) — without fixing this, the new `allowedPermissions` checks could never pass for anyone. Replaced with a real cross-service call to IAM (`surfacing.ts`, adapted from `iam-base/surfacing.ts`'s `createSurfacePermissions`/`createSurfaceRoles` — §3 "Cross-service calls to IAM"). **Redis-backed caching added (2026-09-01, §12 item 8):** `surfacing.ts` now checks/populates an `AuthCacheService` before calling IAM, same shape `iam-base`'s own version uses — a repeat request from the same user skips the round-trip for 5 minutes. External security review still sits outside any PR, and is each adopter's own responsibility for their own deployment (§9) |
| PR 6 | Phase 5 | **Implemented (2026-09-01).** Downstream-builder documentation & extension-point hardening. Found and fixed a real staleness bug in `implementations/cac/base/README.md`: it still told adopters the per-organization runtime switch was "not wired up yet" and pointed them at `hasFeatureChecks`/`surfaceFeatures` — both wrong since the feature-gate work (§12 item 11) landed. Rewrote that section to describe `CodeSetProviderResolver` accurately (how to actually activate real CPT for an org via a `CodeSetLicense` row, and how to verify it via the `codeSet` routes). Also added test coverage for `EntityManagerCptCodeSource` (`cac-base/__test__/cptCodeSource.test.ts`) — the concrete adapter the README holds up as the ready-to-use starting point had zero tests verifying it actually queried `CptCode` correctly before this pass |

**PR 1 is already merged; PR 2 and PR 3 are implemented.** With PR 4 removed, PR 3 is now the largest engineering PR in the plan — the claim builder, all three scrubbing layers (NCCI PTP, NCCI MUE, LCD/NCD), and the real-CPT extension point build (§5's readiness bar) are implemented, and there's no submission/remittance work left to add on top of it. Both PR 1 and PR 3 should land as a sequence of reviewable commits/checkpoints within the PR rather than a single undifferentiated diff.

This count is a working estimate, same caveat as §11 and §12 item 5 — it should flex with whatever the sizing session decides, not be treated as fixed.

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
- AMA — [CPT Licensing FAQs](https://www.ama-assn.org/practice-management/cpt/cpt-licensing-frequently-asked-questions-faqs); AMA compliance portal — [Standard CPT Distribution Pricing Schedule 2026](https://compliance.ama-assn.org/hc/en-us/articles/15166274293399-Notice-Standard-CPT-Distribution-Pricing-Schedule-2026) — retained as reference material for downstream builders (§2), not ForkLaunch's own licensing process
- MGMA-benchmarked summaries: Human Medical Billing — [2025 medical billing KPIs](https://humanmedicalbilling.com/blog/essential-medical-billing-kpis-for-2025-metrics-that-matter-for-revenue-cycle-success/); HFMA — [Redesigning denials management](https://www.hfma.org/revenue-cycle/redesigning-denials-management-in-the-obbba-era/); BillingBench — [RCM benchmarks](https://billingbench.com/benchmarks)
- Stedi — [API-first clearinghouse](https://www.stedi.com/blog/stedi-healthcare-the-only-api-first-clearinghouse-for-health-tech-companies) / [docs](https://www.stedi.com/docs/healthcare); Claim.MD — [software vendor integration](https://www.claim.md/services-software-vendors); Availity — [API guide](https://developer.availity.com/blog/2025/3/25/availity-api-guide)
- ForkLaunch precedent: PR #264 ("feat(messaging): messaging-base + messaging-twilio preconfigured modules") — the concrete template for §1 and §3, confirmed via `git show` on this repo rather than external research.
