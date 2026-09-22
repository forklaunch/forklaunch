# Medical Literature Search Engine — Brief

**The pitch.** A search engine for doctors and hospitals: ask it "open heart surgery" and get a structured, fully-cited answer instead of a pile of links. Every fact traces back to a real source. It never tells a patient what to do — it tells a clinician what the evidence says.

**Why we can move fast on this.** We already have the backend infrastructure this needs — auth, billing, multi-tenant data isolation, async workers, typed APIs — proven on our existing medical-coding product. This isn't a from-scratch build; it's a new module on infrastructure we already run in production.

**What's actually hard, and why:**

1. **Search.** We don't have this kind of search infrastructure yet. Cheapest path: build it on the database we already run, not a new search service — at least for v1.
2. **Licensing.** Real medical terminology (the standard codes doctors and drug databases use) and non-government research papers require actual license agreements — the same kind of problem we already solved for billing codes on the coding product. This is a legal/business task, not an engineering one, and it gates what we can launch with.
3. **Trust.** The whole product only works if it never makes something up. We're designing mechanical fact-checking — not just asking the AI to "be careful" — before anything reaches a doctor.

**Competition — the question to answer before we build.** A tool called OpenEvidence already does something close to this. Before committing engineering time, someone should actually use it and UpToDate (the incumbent) for real clinical questions and compare. If they already do this well, our edge isn't the AI — it's selling it bundled into hospitals that are already our customers for billing and identity.

**What this is.** Pure medical literature — diseases, their treatment, the full surgical process end to end. Not patient data, not a chart, not a record. That's a permanent product boundary, not a launch restriction we'll lift later.

**What we'd build first.** A small, fully-licensed slice — drug labels, clinical trial registry data, a few guidelines we have permission to use. API only, no app screens yet — those come once we know the answers are good.

**The three risks that could actually kill this:**
- Copying full-text research papers without a license — real legal exposure, not a hypothetical one.
- Being treated as a regulated medical device if it edges into recommending treatment for a specific patient — we're designing around this boundary, but it needs real legal sign-off, not an engineering guess.
- The "trust" layer — the fact-checking — takes longer to get right than it looks. It's the hardest and most important part of the build, and most of the rest depends on it.

**Next steps:**
- Invest a small amount of time in hands-on competitive testing (OpenEvidence, UpToDate) before writing code — cheap, fast, and could change the plan.
- Start the licensing conversations for medical terminology and any non-government sources — this runs on its own timeline, outside engineering, and should start now if we're serious.
- Decide whether shipping API-only with no app screens is acceptable for v1, while we prove the answers are actually good.

*Full engineering plan and risk register available on request — this is the short version.*
