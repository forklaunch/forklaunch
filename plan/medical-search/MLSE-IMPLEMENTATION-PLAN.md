# Medical Literature Search Engine (MLSE)

**Implementation Plan**

| | |
|---|---|
| **Version / Date** | 0.3 Draft for review · 23 September 2026 |
| **Prepared by** | ForkLaunch Engineering |
| **Status** | Pending decisions in Section 11 |
| **Classification** | Internal |

## 1. Executive Summary

MLSE is an AI-assisted medical literature search module for ForkLaunch clients to offer their doctors. A clinician searches a disease, drug or procedure and receives one complete answer page, with every statement linked to a verified published source.

- **Audience:** all doctors. Procedure pages with a step-by-step operative walkthrough are the distinguishing feature, supported by a hands-free voice mode for use during surgery.
- **Differentiation:** complete answers from a clinician-approved question framework, automated checking of every citation and number, and explicit "insufficient evidence" messages instead of guesses.
- **Safety boundary:** no patient data and no patient-specific treatment or dose calculation, permanently.
- **Approach:** a module on ForkLaunch's existing infrastructure. New components are search, an AI provider layer and a literature ingestion pipeline.
- **Content:** answers are built from trusted medical sources fetched from the internet, both pre-indexed and queried live at search time. Version 1 uses only free sources that permit commercial use; licensed content can be added later.
- **Delivery:** eight releases; Release 5 (answer generation and verification) is the critical path.

## 2. Problem and Objectives

Clinicians pursue only about half of their point-of-care questions, mainly for lack of time; surgical residents prepare mostly with online videos of inconsistent quality; and doctors' main AI concerns are liability (83%) and transparency (76%). About 81% of US physicians already use AI, yet no product combines evidence search, structured operative walkthroughs and case-study evidence.

| # | Objective | Measure |
|---|---|---|
| O1 | Complete, sourced answers for launch topics | Every framework question answered or marked "insufficient evidence" |
| O2 | No unsupported information without warning | Citation validity ≥ 99%; unsupported claims < 5% |
| O3 | Fast enough for point-of-care use | First useful answer ≤ 10 s (95th percentile) |
| O4 | Hold the safety boundary | 100% compliance on the safety test set |
| O5 | Launch without paid licenses | Version 1 built on free, commercially usable sources |

## 3. Scope

**In scope (Version 1):** answer pages for a small launch set (conditions, medications and 3–5 procedures in one specialty); question frameworks (what, why, who, when, how, risks, aftercare, outcomes); a nine-phase operative walkthrough with sourced values such as blood loss and label dose ranges; published case studies for each procedure, grouped by diagnosis (see below); conflicting-evidence display; API and SDK with a spoken-answer field for voice; editorial governance tools.

**Case studies as an information source.** For every search, MLSE also retrieves published case reports and case series related to the topic, reads them and uses what they contain:
- **Relevance:** a case is used only if it matches the searched topic (procedure, condition or drug) and, where known, the diagnosis, using MeSH concepts plus the relevance score from retrieval. Unrelated or weakly matching cases are discarded.
- **Extraction:** from each relevant case, MLSE extracts the presentation, diagnosis and investigations, procedure details (approach, findings, technique variations), complications, management and outcome.
- **Use in the answer:** the extracted information is added to the matching section, for example an unusual complication and how it was managed under *Complications*, or a technique variation under the relevant operative step, marked "from published case reports." A *Case studies* section also groups the cases by diagnosis, with a link to each source.
- **Weighting:** case-study information ranks below guidelines and trials, never overrides them, and is never used for rates. Where case reports are the only evidence, the page says so.
- **Boundary:** only published, de-identified cases are used, drawn from PubMed's case-report index and openly licensed PubMed Central articles. MLSE does not compare a user's own patient against them.

**Out of scope, permanently:** patient data and health-record integration; patient-specific treatment or dosing; training a medical foundation model.

**Out of scope for Version 1:** user-interface screens, including the voice application (Phase 2); licensed content; knowledge-graph features.

## 4. Key Decisions

**Build approach:** a reusable ForkLaunch module (`mlse-base`), rather than a standalone product or a single-application service, to reuse existing authentication, multi-tenancy, compliance, workers and SDK generation.

**Operating model (decided 23 September 2026):** MLSE is a module for ForkLaunch clients. ForkLaunch ships the engine and free-source connectors; each client deploys it on their own infrastructure, supplies their own API keys for the medical sources, and brings any paid-content licenses they choose to add. ForkLaunch holds no content licenses.

**Content strategy (decided 23 September 2026):** launch with free sources only; add paid content later through a licensed-content extension point.

## 5. Content Sources

| Source | Terms | Commercial use | Provides |
|---|---|---|---|
| openFDA and DailyMed drug labels | Public domain | Permitted | Labelled dosing, contraindications, warnings |
| ClinicalTrials.gov | Public registry | Permitted | Trials and posted results |
| MeSH | Public domain | Permitted | Medical concepts and synonyms |
| PubMed | Metadata free; some abstracts copyrighted | Metadata; abstracts as short excerpts with links | Search across biomedical literature |
| PubMed Central Open Access | Per-article Creative Commons | CC0, CC BY and CC BY-SA only | Full text, case reports, technique papers |
| US government guidance (CDC, USPSTF, AHRQ, NIH) | Public domain (verified per document) | Permitted | Official guidelines |

StatPearls and WHO guidelines are excluded (non-commercial licenses). Operative detail will be partial in Version 1, so launch procedures are chosen partly by free-source coverage; textbooks, journals and video can be licensed later.

## 6. Solution Architecture

**Reused ForkLaunch components:** HTTP framework and SDK generation, PostgreSQL with multi-tenancy, encryption and retention, IAM authentication and roles, Redis, S3-compatible storage, BullMQ workers and OpenTelemetry monitoring.

**New components:**
- **Hybrid search:** PostgreSQL full-text search plus the pgvector extension. Generated projects currently use `postgres:latest`, which lacks pgvector, so the CLI will select a pgvector image for MLSE projects only.
- **AI provider layer:** a provider-neutral interface for text generation and embeddings, so the provider can change after re-evaluation.
- **Live retrieval:** at search time, MLSE also queries PubMed, PubMed Central, openFDA and ClinicalTrials.gov directly, so answers include the newest publications. Live results pass the same license and verification checks. General web pages are not used, because they cannot be verified or licensed.
- **Ingestion pipeline:** scheduled jobs fetch each document, check its license, split it into passages, embed and index it. Retracted papers are excluded immediately and superseded versions retained for audit.

**Data:** the literature corpus is shared and holds no patient or organization data. Organization data (saved searches, history, licenses) is isolated per organization; search history is encrypted and kept briefly as a precaution. Licensed content, when added, is visible only to the licensing organization.

## 7. Answer Generation Pipeline

| Stage | Function | If a check fails |
|---|---|---|
| 1. Classify | Detect patient-specific, dosing, prescription and emergency queries; rewrite follow-ups as standalone questions | Boundary message with label ranges, or a fixed non-AI emergency message |
| 2. Retrieve | Concept expansion; keyword and vector search over the indexed corpus plus live source queries; re-ranking; related case studies retrieved and their key details extracted | Section marked "insufficient evidence"; unrelated cases discarded |
| 3. Assemble | Map evidence to the question framework or procedure phases | Gaps remain visible |
| 4. Draft | AI writes only from assigned evidence, then self-checks | Unsupported sentences removed |
| 5. Verify citations | Each citation must exist, be retrieved and support its sentence | Sentence removed |
| 6. Verify numbers | Each number must match a sourced value, unit and population | Number removed, section flagged |
| 7. Record | Store answer, evidence and model version for audit | — |

Conflicting sources are shown side by side. Case-study information is labelled, ranked below higher evidence and never used for rates.

## 8. Safety, Compliance and Regulation

- No patient identifiers are accepted; no individual dose or treatment is calculated, typed or spoken.
- Voice mode is off by default and enabled per hospital and per area (for example on in operating rooms, off in emergency wards, where patients and families are present). It listens only after a wake word or button, converts speech on the device, stores no audio and removes identifiers from transcripts.
- Organization data uses ForkLaunch's existing encryption, erasure, audit and retention controls. MLSE presents reviewable literature rather than patient-specific recommendations. This is intended to fall outside regulated clinical decision support, but **regulatory advice is required before launch**.

## 9. Quality and Success Metrics

Automated end-to-end tests run against real database and cache instances and cover citation and number removal, insufficient-evidence and conflict display, safety paths, license gating, retractions, tenant isolation and transcript identifier removal. Clinicians author 100–200 evaluation questions per launch topic, re-run before any change to retrieval, prompts or AI provider.

| Metric | Target |
|---|---|
| Citation validity | ≥ 99% |
| Retrieval completeness against the evaluation set | ≥ 85–90% |
| Unsupported-claim rate | < 5% |
| Correct "insufficient evidence" on unanswerable questions | ≥ 90% |
| Safety boundary compliance | 100% |
| First useful answer (95th percentile) | ≤ 10 seconds |

## 10. Delivery Plan and Risks

| Release | Scope |
|---|---|
| 1. Foundation | CLI registration, package skeletons, pgvector database image |
| 2. Core corpus | Source registry, license checks, FDA, ClinicalTrials.gov and MeSH ingestion |
| 3. Search | PubMed and PubMed Central ingestion, embeddings, hybrid search, evaluation tooling |
| 4. Topic pages | Question frameworks, procedure phases and sourced values |
| 5. Answer generation | Drafting, citation and number verification, safety paths (critical path) |
| 6. Licensed content | Extension point for adopters' licensed content |
| 7. Case reports and governance | Rare-complication evidence, retractions, editorial controls |
| 8. Voice readiness | Spoken summaries, per-phase endpoints, hardening |

Indicative effort for Releases 1–5 is two to three quarters, to be validated once the team is staffed.

| Risk | Impact | Mitigation |
|---|---|---|
| Inaccurate content reaches a clinician | High | Seven-stage verification; clinical evaluation before every change |
| Classified as a regulated medical device | High | Literature-only design; regulatory advice before launch |
| Free corpus thin for surgical technique | Medium | Choose launch procedures by coverage; licensed-content extension |
| No clinical input on content | High | Appoint a clinical advisor before Release 4 |
| License terms misapplied | High | Per-document license gating; legal review |
| Competitors copy features | Medium | Focus on procedure depth, verification and hospital distribution |

## 11. Decisions and Next Steps

| # | Item | Owner | Target |
|---|---|---|---|
| 1 | Confirm module name | Founder | Before Release 1 |
| 2 | Review the clickable prototype with 3–5 doctors | Product | 4 weeks |
| 3 | Compare OpenEvidence, ClinicalKey AI, Doximity GPT and UpToDate hands-on | Product | 4 weeks |
| 4 | Measure free-source coverage and select launch topics | Engineering, clinical advisor | 4 weeks |
| 5 | Select AI and embedding provider | Engineering | Before Release 3 |
| 6 | Appoint a clinical advisor | Founder | Before Release 4 |
| 7 | Obtain regulatory advice | Legal | Before launch |
| 8 | Begin Release 1 | Engineering | After item 1 |

## References

- [OpenEvidence accuracy on subspecialty scenarios (medRxiv, 2025)](https://www.medrxiv.org/content/10.64898/2025.11.29.25341091v1.full)
- [OpenEvidence physician adoption (NBC News)](https://www.nbcnews.com/tech/tech-news/openevidence-ai-doctor-medical-physician-login-app-what-npi-uptodate-rcna341064)
- [Physicians' attitudes toward AI (JMIR, 2025)](https://pmc.ncbi.nlm.nih.gov/articles/PMC12421205/)
