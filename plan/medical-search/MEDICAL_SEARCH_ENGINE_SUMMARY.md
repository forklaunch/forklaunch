# Medical Literature Search Engine — Executive Summary

Planning only. Full detail in `MEDICAL_SEARCH_ENGINE_PLAN.md`.

**The idea.** A search engine for clinicians, not patients: a query like "open heart surgery" returns a structured, evidence-graded answer — indications, procedure, complications, medications — every claim traced to a citable source. It answers "what does the literature say," never "what should this patient do."

**Why it's buildable.** As a new ForkLaunch backend module, following the same pattern as `cac-base` (the existing medical-coding module) — reusing auth/RBAC, multi-tenant Postgres with per-org encryption, Redis, S3, async workers, and typed API/SDK generation already in this repo. That lowers engineering cost versus starting from scratch.

**What's genuinely new — the real scope drivers.** Three things don't exist in this codebase today: (1) search/vector infrastructure — modeled directly on PubMed's own capabilities (concept-level indexing via MeSH-style tagging, field-specific search, filters, citation relatedness), propose Postgres + pgvector for the MVP rather than a new search service; (2) medical terminology licensing (SNOMED CT / RxNorm / UMLS) — real license agreements, not just engineering, mirroring the CPT-licensing question already resolved for `cac-base` (MeSH itself, notably, is public domain — no license needed there); (3) a frontend — this repo has no UI at all, for anything, so the MVP should ship as API + typed SDK only.

**The non-negotiable safety boundary.** The system retrieves and summarizes evidence — drug labels, guideline ranges, study findings — but never computes or asserts a specific patient's treatment or dose. It is pure medical literature — diseases, their treatment, the full surgical process end to end — never patient data, and that's a permanent product boundary, not a launch-phase restriction. Enforced structurally: no patient data accepted, ever, query classification that redirects prescription/dosing/emergency queries away from open synthesis, and citations mechanically verified against the actual retrieved evidence before any response reaches a user.

**Competitive reality check.** Tools like OpenEvidence already do something close to this. The real strategic question isn't "can this be built" but whether the differentiator is the technology or the distribution — bundling into hospitals already on ForkLaunch's billing/IAM stack, versus competing head-on as a standalone product. Needs real, hands-on competitive testing before committing engineering time.

**MVP scope.** A small, fully-licensed corpus only (FDA labels, ClinicalTrials.gov, a handful of permitted guidelines). No patient data anywhere. API + SDK, no frontend. The RAG/citation-grounding pipeline is the hardest, highest-risk piece and gates almost everything downstream of it.

**Rough timeline.** Based on the full plan's own epic sizing, a realistic MVP is **2-3 quarters of engineering work, not weeks** — the RAG/citation-grounding pipeline alone is the single largest piece and gates most of what follows it, and that assumes the licensing conversations (§4.1) start now, in parallel with engineering, not after it. Directional, not a committed schedule — nobody has staffed this yet.

**Top risks.** Hallucinated or fabricated citations (mitigated by mechanical verification, not prompting alone) · copyright exposure from bulk-copying full-text papers (default to metadata + excerpt + link-out until licensed) · regulatory misclassification as clinical decision support (needs real legal review) · this repo's own framework/blueprint release-train gap has already broken CI twice on `cac-base` — the same discipline is needed here from day one.

**Biggest open questions.** A real competitive audit against OpenEvidence/UpToDate · who holds the terminology license (ForkLaunch centrally, or each hospital, like the CPT precedent) · where the FDA draws the clinical-decision-support line for this product · one blueprint module or two (ingestion vs. search/RAG).

**Bottom line.** Technically buildable on existing ForkLaunch infrastructure with moderate, well-scoped net-new work. The bigger open questions are legal/licensing and competitive positioning, not engineering feasibility.
