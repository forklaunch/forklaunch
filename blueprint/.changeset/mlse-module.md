---
"@forklaunch/interfaces-mlse": patch
"@forklaunch/implementation-mlse-base": patch
---

Add the mlse (medical literature search) module: `ContentSourceProvider` and `LlmProvider` contracts, a `PublicCorpusProvider` serving the free, commercially usable sources (openFDA, DailyMed, ClinicalTrials.gov, MeSH, PubMed, PubMed Central Open Access), a per-document license gate that stores full text only for CC0, CC BY, CC BY-SA and public-domain works, and a deterministic development AI provider.

The safety classifier reads its phrase lists from `DEFAULT_SAFETY_RULES` (versioned, marked draft until a clinician review), which a deployment can extend with `parseSafetyRuleAdditions` and `useSafetyRuleAdditions` but never weaken. It also catches self-harm, first-person and third-party ingestion with an amount or a poison, and clinicians' shorthand ("pt is 67M").
