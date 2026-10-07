# CAC Module — Objective

**What this is:** a one-page summary of *why* the `cac-base` module exists and *how* it works, for anyone who wants the shape of it without reading the full implementation plan. For entity schemas, the CLI wiring checklist, phasing, PR breakdown, and sourced domain research, see [MEDICAL-CODING-IMPLEMENTATION-PLAN.md](MEDICAL-CODING-IMPLEMENTATION-PLAN.md).

## What we're trying to achieve

Build the coding/billing engine for a hospital claims platform — the piece that takes a patient encounter (diagnoses + procedures), checks it's billable and error-free, and gets it ready to submit to a payer for reimbursement.

We're building it as a reusable ForkLaunch module (`cac-base`), not a one-off app, and building it **free-first** — no paid AMA CPT license needed to build, test, or demo it — while making sure the expensive part (real CPT codes) is engineered to full readiness ahead of time, so switching it on later is a flag flip, not a rewrite.

## How it works, end to end

1. **A claim gets built.** An encounter (visit) has diagnoses (ICD-10 codes, free/public) and procedures (CPT codes — normally paid, but we use free mock placeholders until a hospital actually signs). These combine into a claim.

2. **The claim gets "scrubbed" before it's sent anywhere** — three separate correctness checks:
   - *Do these two procedures conflict* (NCCI PTP)
   - *Is the quantity billed plausible* (NCCI MUE — e.g. not "5 appendectomies")
   - *Does the diagnosis actually justify the procedure* (LCD/NCD — the real medical-necessity check)

   Bad claims get flagged with the same denial codes real payers use (CO-11, CO-16, CO-27, CO-50, CO-97), before they ever leave the building.

3. **Clean claims go out and remittances come back.** We check the patient's coverage first (270/271 eligibility), submit the claim (837) through a clearinghouse (Stedi), and parse the payment/denial response (835) when it comes back — feeding a worklist so staff can see and fix what got denied.

4. **The mock-vs-real switch.** Everything above runs on free mock procedure codes today. There's one interface (`CodeSetProvider`) with two implementations — mock and real CPT — and a single flag per hospital that decides which one's active. Nothing else in the claim/scrubbing logic changes when that flag flips; only the code data underneath it does. So the moment a hospital pays and AMA is licensed, real CPT turns on without a rewrite.

5. **It's a module, not a one-off app.** It plugs into ForkLaunch's existing pieces — IAM for who's allowed to do what (coders vs. billers), the compliance layer for encrypting patient data and audit logs automatically, tenant isolation so one hospital never sees another's data — the same way the `billing` and `iam` modules already do.

## Naming note

Named `cac` (Computer-Assisted Coding — the recognized health-information-management industry term), confirmed by the founder. One open item worth a quick confirmation: industry-wide, "CAC" specifically implies NLP-suggested codes from clinical documentation, which isn't this plan's current scope (the scrubbing engine validates codes already entered, it doesn't suggest them from text). See the full plan, §12 item 9.
