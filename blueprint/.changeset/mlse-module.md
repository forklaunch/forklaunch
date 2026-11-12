---
"@forklaunch/interfaces-mlse": patch
"@forklaunch/implementation-mlse-base": patch
---

First release of the mlse (medical literature search) packages, which back the `mlse-base` module.

`@forklaunch/interfaces-mlse` holds the contracts an implementation provides (`ContentSourceProvider`, `SourceFetcher`, `LlmProvider`, `Reranker`) and the document, retrieval, query and model types they use. The module's HTTP request and response shapes live in the module, not here.

`@forklaunch/implementation-mlse-base` provides:

- Fetchers for free, commercially usable sources: PubMed, PubMed Central Open Access, clinical practice guidelines, MedlinePlus, ClinicalTrials.gov, openFDA, DailyMed, MeSH and NLM Open-i figures. Every request has a deadline, and NCBI's rate limit can be shared through Redis.
- A per-document license gate that stores full text only for CC0, CC BY, CC BY-SA and public-domain works.
- A deterministic safety classifier that decides whether a query reaches the AI: emergencies, self-harm, prescription requests, questions about one patient and dose questions. Its phrase lists are versioned (`DEFAULT_SAFETY_RULES`, draft until clinician review), and a deployment can add to them but never weaken them. It ships with an evaluation set of phrasings.
- Label matching that quotes a drug label's dosing section only for the drug the query names, by generic name.
- Checks that every drafted sentence's citations, numbers, negation and comparison direction match its sources.
- Model providers: Ollama (free, local), Claude (the Anthropic SDK is an optional peer) and a deterministic fake for development.
- Ranking, MeSH expansion, question frameworks for topic pages, and voice summaries.
