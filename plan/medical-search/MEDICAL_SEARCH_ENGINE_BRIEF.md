# Medical Literature Search Engine (MLSE)

**Team Brief**

| | |
|---|---|
| **Version / Date** | 0.3 Draft · 23 September 2026 |
| **Prepared by** | ForkLaunch Engineering |
| **Related documents** | MLSE Implementation Plan (5 pages), MLSE Plan Summary (2 pages) |

## 1. What we are building

MLSE is an AI-assisted medical literature search module that ForkLaunch clients install and offer to their doctors. A doctor searches a disease, drug or procedure and gets one complete answer page instead of a list of links. MLSE fetches information from trusted medical sources on the internet, and every statement on the page links to a verified published source.

**Who it serves:** all doctors. Surgery pages, with a step-by-step operative walkthrough, are the standout feature, and a hands-free voice mode supports doctors during operations.

## 2. Why it matters

Doctors follow up only about half of their clinical questions, mainly for lack of time; surgical residents prepare mostly with online videos of inconsistent quality; and doctors' main AI worries are liability (83%) and transparency (76%). About 81% of US doctors already use AI, but no tool combines evidence search, surgery walkthroughs and case-study evidence.

## 3. How it works for a doctor

A doctor searches **"laparoscopic cholecystectomy"** and the page shows:

1. **Overview and key numbers:** typical operating time, expected blood loss, blood to prepare, anesthesia type and length of stay, each with its source.
2. **Question framework:** what it is, why it is done, who it suits, when, how, risks, aftercare and outcomes.
3. **Operative walkthrough in nine phases:** preparation, anesthesia, positioning, access, core steps, blood management, closure, recovery from anesthesia and post-operative course, with danger points highlighted.
4. **Case studies by diagnosis:** related published cases (for example acute cholecystitis or Mirizzi syndrome), whose key details also feed into the relevant sections, labelled as case-report evidence.
5. **Conflicting evidence and gaps:** disagreements shown side by side; missing evidence shown as "insufficient evidence."
6. **Sources:** every citation, clickable.

A surgeon can also ask by voice ("What's the expected blood loss?", "Next phase") and hear a short answer while the full answer appears on screen.

## 4. What makes it different

- **Complete:** a clinician-approved question set, so nothing important is left out.
- **Checked:** every citation and number is verified automatically.
- **Honest:** "insufficient evidence" instead of guesses. The leading competitor never admitted uncertainty in a published test.
- **Unique:** surgery walkthroughs and case-study evidence, offered by no competitor.
- **Built in:** runs inside ForkLaunch clients' own systems.

## 5. Safety rules (permanent)

- No patient data, ever. Only published, de-identified literature is used.
- No treatment or dose is calculated for a specific patient, whether typed or spoken; the published label range is shown instead.
- Emergency-type questions skip the AI and return a fixed safety message.
- Voice mode listens only after a wake word or button, processes speech on the device and stores no audio.

## 6. How it is built

MLSE is a ForkLaunch module (`mlse-base`) that reuses existing ForkLaunch components: authentication and roles, multi-tenant database, encryption, background workers, caching, storage, monitoring and SDK generation.

**New components**
- **Hybrid search:** keyword and meaning-based search in PostgreSQL with the pgvector extension. The CLI will use a pgvector-enabled database image for MLSE projects only.
- **Live retrieval:** at search time, MLSE queries PubMed, PubMed Central, openFDA and ClinicalTrials.gov for the latest publications, alongside a pre-indexed corpus. General websites are not used because they cannot be verified or licensed.
- **AI provider layer:** provider-neutral, so the AI model can be changed after re-testing.
- **Ingestion pipeline:** scheduled jobs download, license-check, split, embed and index documents.

**Answer process (seven stages):** classify the question → retrieve evidence and related case studies → map it to the question set → draft from evidence only → verify citations → verify numbers → record for audit. Any sentence or number that fails a check is removed.

## 7. Content sources (free-first)

Version 1 uses only free sources that allow commercial use: **openFDA and DailyMed** (drug labels), **ClinicalTrials.gov** (trials), **MeSH** (medical terms), **PubMed** (paper search, abstracts as short excerpts with links), **PubMed Central Open Access** (full text and case reports, commercial-use licenses only) and **US government guidance** (CDC, USPSTF, AHRQ, NIH).

StatPearls and WHO guidelines are excluded because their licenses prohibit commercial use. Clients can add paid content later, such as surgical textbooks, journals and video, with their own licenses. Until then, detailed operating steps will be partial.

## 8. Delivery plan

| Release | Scope |
|---|---|
| 1. Foundation | CLI registration, package setup, pgvector database |
| 2. Core corpus | Source registry, license checks, FDA, trials and MeSH |
| 3. Search | PubMed and PubMed Central, live retrieval, hybrid search, evaluation tools |
| 4. Topic pages | Question sets, surgery phases, sourced numbers |
| 5. Answer generation | Drafting, citation and number checks, safety rules (critical path) |
| 6. Licensed content | Support for clients' paid content |
| 7. Case studies and governance | Case-study extraction, retractions, editor tools |
| 8. Voice readiness | Spoken answers, per-phase access, hardening |

Indicative effort for Releases 1–5 is two to three quarters, to be confirmed once staffed.

**Success targets:** citation validity ≥ 99%, unsupported claims < 5%, correct "insufficient evidence" ≥ 90%, safety compliance 100%, first answer within 10 seconds.

## 9. Main risks

| Risk | Mitigation |
|---|---|
| Wrong information reaches a doctor | Seven-stage checks; clinical testing before every change |
| Treated as a regulated medical device | Literature-only design; legal advice before launch |
| Free sources thin on surgical technique | Pick launch procedures by coverage; clients add paid content later |
| No doctor input yet | Appoint a clinical advisor before Release 4 |

## 10. What we need from the team

| # | Action | Owner | When |
|---|---|---|---|
| 1 | Confirm the module name | Founder | Before Release 1 |
| 2 | Show the clickable demo to 3–5 doctors and collect feedback | Product | 4 weeks |
| 3 | Test leading competitors (OpenEvidence, ClinicalKey AI, Doximity GPT, UpToDate) | Product | 4 weeks |
| 4 | Measure free-source coverage and choose launch topics | Engineering | 4 weeks |
| 5 | Choose the AI provider | Engineering | Before Release 3 |
| 6 | Find a clinical advisor | Founder | Before Release 4 |
| 7 | Get regulatory advice | Legal | Before launch |
| 8 | Start Release 1 | Engineering | After action 1 |

