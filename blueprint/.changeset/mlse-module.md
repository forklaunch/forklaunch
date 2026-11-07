---
"@forklaunch/interfaces-mlse": patch
"@forklaunch/implementation-mlse-base": patch
---

Add the mlse (medical literature search) module: `ContentSourceProvider` and `LlmProvider` contracts, a `PublicCorpusProvider` serving the free, commercially usable sources (openFDA, DailyMed, ClinicalTrials.gov, MeSH, PubMed, PubMed Central Open Access), a per-document license gate that stores full text only for CC0, CC BY, CC BY-SA and public-domain works, and a deterministic development AI provider.

The safety classifier reads its phrase lists from `DEFAULT_SAFETY_RULES` (versioned, marked draft until a clinician review), which a deployment can extend with `parseSafetyRuleAdditions` and `useSafetyRuleAdditions` but never weaken. It also catches self-harm, first-person and third-party ingestion with an amount or a poison, and clinicians' shorthand ("pt is 67M").

Embedding requests can say whether the texts are search queries or passages (`purpose`); the Ollama provider then adds the task instruction `nomic-embed-text` was trained with.

The procedure question framework now holds only hint words that fit any procedure; `frameworkItems(framework, topicHints)` adds a topic's own words. `selectEvidence` takes `wholeTerm` so topic pages count a document only when it names the topic in full.

PMC searches now rank by relevance, as PubMed searches did, and PMC articles drop declaration sections (funding, ethics, consent, competing interests, AI use) by heading as well as by type.

`itemFitRank` ranks a page's items for a passage by embedding similarity to each item's question, and on topic pages (`wholeTerm`) a word of the topic's name no longer counts as a hint.

`splitAspectQuery` separates a question about one part of a topic ("procedure for heart attack") into the topic and the aspect asked about.

`ImageSearchService` finds figures from open-access articles through NLM Open-i, like a web search's image results: only figures licensed for commercial reuse (CC0, CC BY, CC BY-SA), each with its caption, article and license, filterable by type (photo, x-ray, CT, MRI, ultrasound, microscopy, diagram). Queries about a patient are not sent.

`findStepFigures` finds openly licensed figures that show one step of a procedure (an incision, a cut, an anastomosis), judged by caption: photos and drawings of the operation, not imaging, histology, animal studies, charts or the healed wound. `MedicalImage.panels` says what a multi-panel figure shows.

Image search leaves charts and diagrams out unless asked for (`charts`, or `type: diagram`), judged by caption since Open-i labels many of them photos; `isChart` makes that call.

`GuidelineFetcher` (source `guidelines`) finds clinical practice guidelines from the last ten years through PubMed, storing a guideline in full when its PubMed Central copy is openly licensed for commercial reuse and as an abstract excerpt otherwise (most society guidelines). PubMed records carry their `pmcid`, and `PmcOaFetcher.fetchOpenAccess` downloads only the open-access articles among a list. Live retrieval can give a slow source its own budget (`sourceTimeoutsMs`) and query it only when named (`onRequestOnly`).

A source that answers "too many requests" (HTTP 429) is asked once more a second later, since NCBI's limit is shared by every client using it.
