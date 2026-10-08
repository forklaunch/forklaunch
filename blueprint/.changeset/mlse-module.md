---
"@forklaunch/interfaces-mlse": patch
"@forklaunch/implementation-mlse-base": patch
---

Add the mlse (medical literature search) module: `ContentSourceProvider` and `LlmProvider` contracts, a `PublicCorpusProvider` serving the free, commercially usable sources (openFDA, DailyMed, ClinicalTrials.gov, MeSH, PubMed, PubMed Central Open Access), a per-document license gate that stores full text only for CC0, CC BY, CC BY-SA and public-domain works, and a deterministic development AI provider.

The safety classifier reads its phrase lists from `DEFAULT_SAFETY_RULES` (versioned, marked draft until a clinician review), which a deployment can extend with `parseSafetyRuleAdditions` and `useSafetyRuleAdditions` but never weaken. It also catches self-harm, first-person and third-party ingestion with an amount or a poison, and clinicians' shorthand ("pt is 67M").

Embedding requests can say whether the texts are search queries or passages (`purpose`); the Ollama provider then adds the task instruction `nomic-embed-text` was trained with.

The procedure question framework now holds only hint words that fit any procedure; `frameworkItems(framework, topicHints)` adds a topic's own words. `selectEvidence` takes `wholeTerm` so topic pages count a document only when it names the topic in full.

PMC searches now rank by relevance, as PubMed searches did, and PMC articles drop declaration sections (funding, ethics, consent, competing interests, AI use) by heading as well as by type.
