# MLSE — Medical Literature Search Engine

MLSE is a ForkLaunch module that gives doctors one sourced answer page for a
disease, drug or procedure. It searches published medical sources (live and
from its own corpus), drafts answers with an AI model **from those sources
only**, and checks every sentence against the passage it cites before a
doctor sees it. For surgery it builds an end-to-end walkthrough, phase by
phase, with published case reports grouped by diagnosis.

> **Status: not for clinical use yet.** The question frameworks, drafting
> rules and fixed safety messages are engineering drafts. They must be
> reviewed by a clinician (and the safety boundaries by legal) before answers
> are shown to doctors. Every page and answer says so until then.

## Contents

1. [How it works](#how-it-works)
2. [Setup](#setup)
3. [Running it](#running-it)
4. [API](#api)
5. [Authentication and permissions](#authentication-and-permissions)
6. [Safety model](#safety-model)
7. [Privacy and data](#privacy-and-data)
8. [Licensed content](#licensed-content)
9. [Voice](#voice)
10. [AI provider and cost](#ai-provider-and-cost)
11. [Known limitations](#known-limitations)

## How it works

```
query ─► classify (fixed rules) ─┬─► emergency / prescription / one patient ─► fixed message (no AI)
                                 │                                            + label dosing quoted as written
                                 └─► literature question
                                        │
             corpus (PostgreSQL + pgvector) ◄── ingestion worker ◄── openFDA, DailyMed,
             + live queries (4 s budget)                                ClinicalTrials.gov,
                                        │                               PubMed, PMC OA, MeSH
                                        ▼
                    hybrid search: keyword + vector + MeSH synonyms, fused and re-ranked
                                        ▼
                    AI drafts each section from the passages only, citing [P1]…
                                        ▼
                    verify every sentence: citations exist, words and numbers match
                    ─ failed sentences removed, section redrafted once (self-check)
                                        ▼
                    answer + audit record (sources, removed sentences and reasons)
```

- **Sources** are public medical APIs with licenses that allow use in a
  commercial product. Each document is license-checked before anything is
  stored: full text, excerpt only, or metadata only.
- **Topic pages** (`/topic`) answer a fixed list of questions per topic type,
  plus nine operative phases for procedures, with evidence per item or an
  explicit "insufficient evidence".
- **Case reports** are matched to topics by MeSH descriptor or title and
  grouped by diagnosis.

## Setup

Scaffold it like any other module:

```bash
forklaunch init module mlse -m mlse-base
```

MLSE needs **PostgreSQL with pgvector**. The CLI switches the shared
`postgres` service to `pgvector/pgvector:pg18-trixie` (same PostgreSQL major
version and Debian base as `postgres:latest`) and refuses other databases. It
also needs Redis for the ingestion queue and the live-source cache.

Environment (`.env.local`):

| Variable | Required | Purpose |
|---|---|---|
| `MLSE_INGESTION_QUEUE` | yes | Redis queue the worker drains |
| `NCBI_TOOL`, `NCBI_EMAIL` | yes | NCBI asks every app calling E-utilities to identify itself |
| `NCBI_API_KEY` | no | Your own NCBI key; raises PubMed/PMC from 3 to 10 requests/s |
| `OPENFDA_API_KEY` | no | Higher openFDA rate limit |
| `LLM_PROVIDER` | yes | `claude`, or `fake` for development (no key, deterministic) |
| `LLM_API_KEY` | with `claude` | Your own Anthropic API key; the service will not start without it |
| `LLM_MODEL` | no | Default `claude-opus-5` |
| `LLM_EFFORT` | no | `low` \| `medium` \| `high` (default) \| `xhigh` \| `max` |
| `EMBEDDING_DIMENSIONS` | no | Must match the embedding model (default 8, development only) |
| `LIVE_RETRIEVAL_TIMEOUT_MS` | no | Budget for live source queries per search (default 4000) |
| `CORPUS_TOPICS`, `CORPUS_SOURCES`, `CORPUS_LIMIT` | no | What `corpus:refresh` queues: terms, sources (default all) and documents per term (default 20) |

Each deployment uses its own API keys. Never share one key between clients.

## Running it

```bash
pnpm dev             # migrations, then the HTTP service
pnpm dev:worker      # ingestion worker (registers sources, drains the queue)
pnpm corpus:refresh  # queue ingestion of CORPUS_TOPICS from every source
pnpm mesh:load path/to/desc2026.xml      # load NLM MeSH descriptors (synonyms, tree numbers)
pnpm eval:run eval/gold-set.json         # retrieval evaluation against a clinician gold set
pnpm retention:enforce                   # run daily: history retention
```

Build a topic page after the corpus has content:
`POST /topic/laparoscopic-cholecystectomy/assemble`, then
`GET /topic/laparoscopic-cholecystectomy`.

## API

Every route is also in the generated SDK (`sdk.ts`).

| Route | What it does |
|---|---|
| `GET /search?q=` | Hybrid search; returns citable passages and per-source live status |
| `POST /answer` | Answer, streamed as server-sent events: `start`, one `section` per verified section, `done` |
| `POST /answer/complete` | The same answer as one JSON response |
| `GET /topic`, `GET /topic/:slug`, `GET /topic/:slug/phase/:n` | Topic pages and single procedure phases |
| `POST /topic/:slug/assemble` | Rebuild a topic page from the corpus |
| `GET /source`, `POST /source/:key/refresh` | Source registry; queue an ingestion |
| `GET /document/:id?organizationId=` | One stored document (hidden if licensed without an active license, or flagged) |
| `POST /voice/query`, `GET/PUT /voice/setting` | Voice questions; voice settings per care area |
| `GET/POST /saved-search`, `DELETE /saved-search/:id`, `GET /saved-search/history` | Saved searches and history |
| `GET /admin/source-access`, `POST /admin/source` | Source access per organization; register a licensed source |
| `GET/POST /admin/content-license`, `POST /admin/content-license/:id/revoke` | Content licenses |
| `GET/POST /admin/content-flag`, `POST /admin/content-flag/:id/resolve` | Reviewer flags |
| `POST /admin/topic/:slug/approve` | Mark a topic clinician-approved |
| `GET /compliance/export/:userId`, `DELETE /compliance/erase/:userId` | GDPR export and erasure |

`organizationId` and `userId` in requests are trusted as sent (see below).
Without `organizationId`, licensed sources are never searched.

## Authentication and permissions

MLSE is called **by your application's backend, not by browsers**. Every
route except compliance uses HMAC internal auth with `HMAC_SECRET_KEY`. Your
backend signs in the doctor (for example with the IAM module), checks their
permission, and calls MLSE with the doctor's `organizationId` and `userId`.
Keep `HMAC_SECRET_KEY` server-side only: anyone holding it can act as any
organization.

Permissions to create in IAM and check before calling MLSE:

| Permission | Give to | Routes |
|---|---|---|
| `clinician:search` | Doctors | `/search`, `/answer*`, `/topic` (GET), `/document`, `/source` (GET) |
| `clinician:voice` | Doctors in areas where voice is enabled | `/voice/query` |
| `clinician:save_search` | Doctors | `/saved-search*` |
| `reviewer:manage_content` | Clinical reviewers | `/admin/content-flag*`, `/admin/topic/*/approve`, `POST /topic/*/assemble` |
| `admin:manage_sources` | Content administrators | `/admin/source*`, `/admin/content-license*`, `POST /source/*/refresh` |
| `admin:manage_voice` | Hospital administrators | `PUT /voice/setting` |

Compliance routes use JWT with platform system roles, like every ForkLaunch
module.

## Safety model

- **Classification before anything else, by fixed rules.** An emergency
  happening now gets a fixed message pointing to the emergency protocol.
  Prescription requests are refused. Questions about one patient ("patient
  weighs 80 kg, how much propofol") get a boundary message plus the drug
  label's dosing section quoted as written. A dose asked with no context gets
  the quoted label section. None of these reach the AI. Literature questions
  about emergencies ("management of acetaminophen overdose") are answered
  normally.
- **Answers only from retrieved passages.** The drafting rules forbid
  outside knowledge, calculation and rounding. Passages are treated as
  untrusted data, not instructions.
- **Every sentence is verified.** It must cite a supplied passage, most of
  its words must appear there, and every number must appear there with the
  same unit. Failures are removed and logged. Nothing unverified is shown.
- **Fail closed everywhere.** An unknown provider stops the service at
  startup, a missing voice setting means voice is off, an unlicensed source
  is invisible, and a flagged document is hidden.
- **Drafts are labelled.** Pages and answers carry a notice until their
  topic and framework are clinician-approved. Approval is refused while the
  framework is still a draft.

## Privacy and data

- MLSE stores **published literature**, not patient records. Do not send
  patient identifiers in queries. MLSE reduces the harm if you do, but does
  not make it safe.
- Queries are classified before anything leaves the service. Only
  literature questions are sent to live sources (PubMed, openFDA, …), queued
  for ingestion or kept as text. A patient-specific, prescription or
  emergency query is searched in the stored corpus only, and its text is not
  stored in the answer audit or history. It also can't be saved as a saved
  search. Queries are never written to logs, and live-source cache keys are
  hashes of the query, not its text.
- Saved searches and history are encrypted at rest with a per-organization
  key, and each organization sees only its own. History loses its query text
  after 90 days (`pnpm retention:enforce`, run daily).
- `GET /compliance/export/:userId` and `DELETE /compliance/erase/:userId`
  cover saved searches and history.
- With `LLM_PROVIDER=claude`, retrieved passages and the literature question
  are sent to Anthropic. Review Anthropic's data-handling terms for your
  deployment.

## Licensed content

To add a subscription source (for example a society's guideline feed):

1. Write a `SourceFetcher` for the publisher's API and wrap it:
   `new LicensedContentAdapter(fetcher, { scope: 'full_text' })` (or
   `'excerpt_only'`, as the contract allows). Add it to `SourceFetchers` in
   `registrations.ts`.
2. Register the source: `POST /admin/source`.
3. Record each organization's license: `POST /admin/content-license`.

Documents keep the contract's terms only when they arrive through the
adapter. A public feed that carries the same license string gets metadata
only. The source is ingested only while some organization holds an active,
in-date license, and each organization only sees it while its own license is
active. Revoking a license hides the source from that organization
immediately.

## Voice

Voice is **off everywhere by default**. A hospital turns it on per care area
(`PUT /voice/setting`, for example `operating_room` on, `emergency` off).
Requests from any other area are refused with 403, while typed search always
works. Transcripts have names after titles, record/bed/room numbers, dates,
phone numbers and emails removed before anything else happens. The spoken
reply uses only verified sentences and points to the sources on screen.
Speech-to-text and text-to-speech are left to your client application.

## AI provider and cost

With `LLM_PROVIDER=claude`, MLSE uses the Anthropic SDK with `claude-opus-5`
by default, adaptive thinking, streaming, a cached system prompt, and
Anthropic's server-side fallback if a request is declined. Usage is billed to
your Anthropic account. As a rough guide (check Anthropic's current pricing):
a search answer is a few cents, and assembling a full topic page is around a
dollar. Emergency, prescription and patient-specific queries cost nothing,
since they never reach the AI. `LLM_MODEL=claude-sonnet-5` is cheaper;
measure answer quality with `eval:run` before switching.

Claude has no embeddings API, so embeddings currently come from the
development provider, which is deterministic but not semantic. Choosing a
real embedding model is an open decision. Its dimension fixes the vector
column size.

## Known limitations

- Question frameworks, drafting rules and fixed messages need clinician (and
  legal) review before clinical use.
- Development embeddings limit search relevance until an embedding model is
  chosen.
- Answer quality and latency with Claude have not yet been measured against
  the 10-second first-answer target.
- The generated `server.ts` still surfaces placeholder roles for every
  caller. Replace them with real IAM role surfacing before using the JWT
  compliance routes, which require platform system roles.
- `@forklaunch/core` 2.1.0 keeps a separate copy of its compliance registries
  in the `services` bundle, so `RetentionService` and `ComplianceDataService`
  do not see entity policies. MLSE therefore runs its history retention and
  GDPR export/erase itself, until core is fixed.
