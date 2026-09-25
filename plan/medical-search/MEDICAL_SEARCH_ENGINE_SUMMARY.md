# Medical Literature Search Engine (MLSE)

**Plan Summary**

| | |
|---|---|
| **Version / Date** | 0.3 Draft · 23 September 2026 |
| **Prepared by** | ForkLaunch Engineering |
| **Full plan** | MLSE Implementation Plan (5 pages) |

## What MLSE is

An AI-assisted medical literature search module that ForkLaunch clients offer to their doctors. A clinician searches a disease, drug or procedure and receives one complete answer page, with every statement linked to a verified published source.

## Why it matters

Clinicians pursue only about half of their point-of-care questions, mainly for lack of time. Surgical residents prepare mostly with online videos of inconsistent quality, and doctors' main concerns about AI are liability (83%) and transparency (76%). About 81% of US physicians already use AI, yet no product combines evidence search, structured operative walkthroughs and case-study evidence.

## What makes it different

- **Complete answers:** every topic answers a clinician-approved question set (what, why, who, when, how, risks, aftercare, outcomes).
- **Surgery walkthroughs:** a nine-phase operative guide with sourced values such as blood loss and label dose ranges, plus a hands-free voice mode for use in surgery.
- **Case studies used as evidence:** related published case reports are found, their key details extracted and added to the answer, clearly labelled and ranked below guidelines and trials.
- **Never wrong without warning:** every citation and number is checked automatically; missing evidence is shown as "insufficient evidence."
- **Safety boundary:** no patient data and no patient-specific treatment or dosing, permanently.

## How it will be built

A reusable ForkLaunch module (`mlse-base`) that each client deploys on their own infrastructure, built on existing ForkLaunch components: authentication, multi-tenancy, encryption, background workers and SDK generation. New components are hybrid search (PostgreSQL with pgvector), a provider-neutral AI layer, a literature ingestion pipeline and live retrieval that queries trusted medical sources (PubMed, PubMed Central, openFDA, ClinicalTrials.gov) at search time. General web pages are not used, because they cannot be verified or licensed. Each answer passes through seven stages: classify the query, retrieve evidence and related case studies, assemble the answer framework, draft from evidence only, verify citations, verify numbers, and record for audit.

## Content (free-first, decided)

Version 1 uses only free sources that permit commercial use: FDA drug labels, ClinicalTrials.gov, MeSH, PubMed, openly licensed PubMed Central articles (including case reports) and US government guidance. StatPearls and WHO guidelines are excluded for non-commercial licenses. Surgical textbooks, journals and video can be added later through a licensed-content extension point.

## Targets

| Measure | Target |
|---|---|
| Citation validity | ≥ 99% |
| Unsupported-claim rate | < 5% |
| Correct "insufficient evidence" on unanswerable questions | ≥ 90% |
| Safety boundary compliance | 100% |
| First useful answer | ≤ 10 seconds |

## Delivery

Eight releases: (1) foundation, (2) core corpus, (3) search, (4) topic pages, (5) answer generation and verification, the critical path, (6) licensed-content extension, (7) case studies and governance, (8) voice readiness. Indicative effort for Releases 1–5 is two to three quarters, to be confirmed once staffed.

## Main risks

| Risk | Mitigation |
|---|---|
| Inaccurate content reaches a clinician | Seven-stage verification; clinical evaluation before every change |
| Regulated as a medical device | Literature-only design; regulatory advice before launch |
| Free sources thin on surgical technique | Choose launch procedures by coverage; add licensed content later |
| No clinical input yet | Appoint a clinical advisor before Release 4 |

## Decisions and next steps

1. **Founder:** confirm the module name. (Operating model decided: a module for ForkLaunch clients, who supply their own API keys and any paid-content licenses.)
2. **Product:** review the clickable prototype with 3–5 doctors and compare leading competitors hands-on within 4 weeks.
3. **Engineering:** measure free-source coverage, select launch topics and the AI provider, then begin Release 1.
4. **Founder and legal:** appoint a clinical advisor and obtain regulatory advice before launch.
