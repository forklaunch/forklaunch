# Medical Literature Search Engine — Plan Summary

Planning only. Full detail, risk register, and 13-epic implementation plan live in `MEDICAL_SEARCH_ENGINE_PLAN.md`. This is the short version.

## The idea

A search engine for clinicians, not patients: a query like "open heart surgery" returns a structured, evidence-graded answer — indications, procedure, complications, medications — every claim traced to a citable source. It answers "what does the literature say," never "what should this patient do."

**Two layers, not one.** The retrieval foundation is modeled directly on PubMed — concept-level indexing (a MeSH-style controlled vocabulary, not just free-text matching), field-specific search, filters, and citation relatedness. On top of that sits the product's actual value-add: an AI-synthesized, fully-cited answer, not just a results list. PubMed-grade search is the foundation this is built on; it is not a replacement for the synthesis layer.

## Scope: pure medical literature, permanently

This covers diseases, their treatment, and the full surgical process end to end — indications through recovery. It never touches patient data: no patient records, no patient-specific history, no chart data, ever. This is not a Phase 1 restriction to be lifted later — it's a permanent architectural boundary, enforced structurally (no patient-identifying input accepted, query classification that redirects any prescription/dosing/emergency-shaped request, mechanical citation verification before anything reaches a user) rather than left to an AI model's judgment.

## Why it's buildable

As a new ForkLaunch backend module, following the same pattern as `cac-base` (the existing medical-coding module) — reusing auth/RBAC, multi-tenant Postgres, Redis, S3, async workers, and typed API/SDK generation already proven in this repo. That materially lowers engineering cost versus starting from scratch.

## What's genuinely new — the real scope drivers

Three things don't exist in this codebase today:

1. **Search/indexing infrastructure**, modeled on PubMed's own capabilities — MeSH-style concept tagging, field-specific query syntax, filters, "similar articles" relatedness, saved searches. Proposal: build it on Postgres + pgvector for the MVP rather than standing up a new search service.
2. **Medical terminology licensing.** SNOMED CT, RxNorm, and UMLS all require real license agreements, not just engineering — the same category of problem already resolved for CPT billing codes on the coding product. MeSH itself, notably, is public domain — no license needed there, and it's the natural starting point.
3. **A frontend.** This repo has no UI at all, for anything. The MVP should ship as API + typed SDK only; a UI is a separate initiative.

## Competitive reality check

Tools like OpenEvidence already do something close to this. The real strategic question isn't "can this be built" but whether the differentiator is the technology or the distribution — bundling into hospitals already on ForkLaunch's billing/IAM stack, versus competing head-on as a standalone product. This needs real, hands-on competitive testing (actually using OpenEvidence and UpToDate for real clinical questions) before committing engineering time to it.

## MVP scope

A small, fully-licensed corpus only — FDA labels, ClinicalTrials.gov registry data, a handful of guidelines with explicit permission. No patient data anywhere. API + SDK, no frontend. The AI-synthesis/citation-grounding pipeline is the hardest, highest-risk piece and gates almost everything downstream of it.

## Rough timeline

Based on the full plan's own epic sizing: a realistic MVP is **2-3 quarters of engineering work, not weeks.** The synthesis/citation-grounding pipeline alone is the single largest piece and gates most of what follows it, and that estimate assumes licensing conversations start now, in parallel with engineering, not after it. Directional, not a committed schedule — nobody has staffed this yet.

## Top risks

- **Hallucinated or fabricated citations** reaching a clinician — mitigated by mechanical verification against the actual retrieved evidence, not prompting alone.
- **Copyright exposure** from bulk-copying full-text research papers — default to metadata + short excerpt + link-out until a license says otherwise.
- **Regulatory misclassification** as clinical decision support (FDA) if the boundary in this plan isn't held precisely — needs real legal review, not an engineering guess.
- **This repo's own framework/blueprint release-train gap** has already broken CI twice on `cac-base` — the same discipline is needed here from day one, not learned the hard way again.

## Biggest open questions

- A real, hands-on competitive audit against OpenEvidence and UpToDate — not yet done.
- Who holds the terminology license — ForkLaunch centrally, or each adopting hospital, mirroring the CPT precedent?
- Where the FDA draws the clinical-decision-support line for this specific product.
- One blueprint module, or two (ingestion/corpus vs. search/RAG)?

## Bottom line

Technically buildable on existing ForkLaunch infrastructure, with well-scoped net-new work: a PubMed-grade search/indexing layer, a licensing gate, and a citation-grounded synthesis pipeline. The bigger open questions are legal/licensing and competitive positioning, not engineering feasibility.
