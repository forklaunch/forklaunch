# CAC Module — Objective

**What this is:** a one-page summary of *why* the `cac-base` module exists and *how* it works, for anyone who wants the shape of it without reading the full implementation plan. For entity schemas, the CLI wiring checklist, phasing, PR breakdown, and sourced domain research, see [MEDICAL-CODING-IMPLEMENTATION-PLAN.md](MEDICAL-CODING-IMPLEMENTATION-PLAN.md).

## What we're trying to achieve

Build the coding/billing engine for a hospital claims platform — the piece that takes a patient encounter (diagnoses + procedures), checks it's billable and error-free, and gets it ready to submit to a payer for reimbursement.

We're building it as a reusable ForkLaunch module (`cac-base`), the same way `iam-base` and `billing-base` are — **not** a product ForkLaunch operates or sells directly to hospitals. Confirmed by the founder: ForkLaunch is not in the CPT-licensing business. Companies who already hold their own AMA CPT license use `cac-base` as a building block, build their own product on top of it, and self-host it — modifying and extending it as their own needs require.

## What's actually built right now

- ✅ **Built:** the module itself (entities, DI wiring), free code sets (ICD-10-CM, HCPCS) with a real refresh pipeline, claim building, and all three scrubbing layers — items 1, 2, 4, and 5 below.
- ✅ **Built:** the real-CPT extension point (item 4) — a real, tested adapter an adopter can wire their own licensed data into today — **and the automatic per-organization switch that uses it.** Each organization's own license status decides mock vs. real CPT per request; nobody has to wire anything in by hand.
- ✅ **Built:** a denial worklist API (list/view/resolve the scrubbing engine's findings), an analytics API (clean-claim-rate, denial-rate), and real RBAC — coder/biller/admin routes actually check permissions against a live call to IAM, not a placeholder that granted everyone the same access.
- ❌ **Out of scope, not just unbuilt:** item 3 below (eligibility check, clearinghouse submission, remittance parsing). Confirmed by the founder (2026-08-31): `cac-base` doesn't execute or submit claims on an adopter's behalf — it produces the scrubbing report, and the adopter takes it from there through their own systems. This isn't waiting on a Stedi account; it's not something this module builds.
- ✅ **Built:** Redis-backed caching for the IAM permission/role lookups — a repeat request from the same user within a 5-minute window no longer re-calls IAM.
- ⏸️ **Not built, and not currently useful:** caching the scrubbing-rule lookups. Today those are pure in-memory checks against mock tables — there's no network/DB call there to cache. It becomes worth building once the real-CPT rule source (item 4) replaces the mocks and those lookups become genuine I/O.

## How it works, end to end

1. **A claim gets built.** An encounter (visit) has diagnoses (ICD-10 codes, free/public) and procedures (CPT codes). Out of the box the module uses free mock procedure-code placeholders, since ForkLaunch never holds real CPT content — an adopter with their own license plugs in their own real CPT connector. These combine into a claim.

2. **The claim gets "scrubbed" before it's sent anywhere** — three separate correctness checks:
   - *Do these two procedures conflict* (NCCI PTP)
   - *Is the quantity billed plausible* (NCCI MUE — e.g. not "5 appendectomies")
   - *Does the diagnosis actually justify the procedure* (LCD/NCD — the real medical-necessity check)

   Bad claims get flagged with the same denial codes real payers use (CO-11, CO-16, CO-27, CO-50, CO-97), before they ever leave the building.

3. **We don't submit the claim anywhere — that part is out of scope. What we do instead is hand back a worklist.** Earlier drafts of this plan had `cac-base` checking the patient's coverage (270/271 eligibility), submitting the claim (837) through a clearinghouse, and parsing the payment/denial response (835) when it came back. The founder clarified (2026-08-31): we don't need to execute the claim on the business's behalf, just map the codes — they take the scrubbing report from step 2 and do whatever they like with it through their own systems. This module's job ends at the report — which now includes a real denial worklist (list what got flagged, mark items resolved) and an analytics summary (clean-claim-rate, denial-rate), not just raw data sitting in a table.

4. **The mock-vs-real switch — and who's on each side of it.** There's one interface (`CodeSetProvider`) with two sides: a free, fully-built mock implementation that ships with the module, and a real-CPT extension point (`CptCodeProvider`) that an adopting company wires their own licensed CPT data into. Both sides are built and tested today — the extension point is genuinely usable now, not a future promise — but it never contains real AMA content itself; that's each adopter's own connector, using their own license. A per-organization check decides which side is active automatically: an org with no license (or one that isn't active) transparently gets mock data, an org with `CodeSetLicense.status = 'active'` gets its own real connector — nobody has to flip anything by hand, and a lookup failure always falls back to mock rather than blocking the request.

5. **It's a module, not a one-off app.** It plugs into ForkLaunch's existing pieces — IAM for who's allowed to do what (coders vs. billers), the compliance layer for encrypting patient data and audit logs automatically, tenant isolation so one hospital never sees another's data — the same way the `billing` and `iam` modules already do. This is real, not aspirational: claim/denial/analytics/code-set routes actually check a live permission from IAM before letting a request through, replacing an early placeholder that granted every request the same access regardless of who was asking.

## Business model, in plain terms

ForkLaunch never applies for, holds, or pays for a CPT license — not for ourselves, and not on behalf of anyone using the module. Each company building on `cac-base` is expected to already have (or independently obtain) their own AMA CPT license before wiring up real CPT data, exactly the way they'd bring their own Stripe account to `billing-base` or their own auth policy to `iam-base`. No patient or doctor data ever flows through ForkLaunch — every adopter deploys and runs their own instance, under their own license, on their own infrastructure.

## Validating it without a license

Since ForkLaunch never touches real CPT, validation focuses entirely on the free code sets (ICD-10-CM, HCPCS, and the three-layer scrubbing engine above) — no license needed. A simple UI for exactly this is planned as a separate build in the `forklaunch-platform` repo: enter a diagnosis code, a mock procedure code, and a unit count; see whether the claim passes or gets flagged, and which layer/denial code caught it. See the full plan, §10.

## Naming note

Named `cac` (Computer-Assisted Coding — the recognized health-information-management industry term), confirmed by the founder. One open item worth a quick confirmation: industry-wide, "CAC" specifically implies NLP-suggested codes from clinical documentation, which isn't this plan's current scope (the scrubbing engine validates codes already entered, it doesn't suggest them from text). See the full plan, §12 item 9.
