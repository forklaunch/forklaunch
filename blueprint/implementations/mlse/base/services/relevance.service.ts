import { clinicalTermsFor } from './layTerms.service';
import { queryTerms } from './ranking.service';

export type QueryConcepts = {
  // clinical terms the query names (from everyday terms or MeSH); a passage
  // naming every word of one of them is about the query
  phrases: string[][];
  // the query's own words, used when it names no clinical term
  queryWords: string[];
  // true when the query used an everyday term: its own words ("heartattack")
  // then do not count, only the clinical term does
  layMapped: boolean;
};

const MIN_QUERY_COVERAGE = 0.6;

// Trial eligibility rules mention conditions only to exclude people with
// them; they are not evidence about the condition.
function isEligibilitySection(sectionPath: string): boolean {
  const path = sectionPath.toLowerCase();
  return ['eligib', 'inclusion', 'exclusion', 'criteria'].some((word) => path.includes(word));
}

export function queryConcepts(query: string, expandedTerms: string[]): QueryConcepts {
  const lay = clinicalTermsFor(query);
  const normalizedQuery = query.trim().toLowerCase();
  const phrases = [...lay, ...expandedTerms.filter((t) => t.trim().toLowerCase() !== normalizedQuery)]
    .map((term) => queryTerms(term))
    .filter((words) => words.length > 0);
  return { phrases, queryWords: queryTerms(query), layMapped: lay.length > 0 };
}

export type KeySentence = { text: string; passageId: string };

// A sentence ends at . ! or ? followed by a space and a capital letter.
function sentencesOf(text: string): string[] {
  const out: string[] = [];
  let start = 0;
  for (let i = 0; i < text.length - 2; i++) {
    const ch = text[i];
    if ((ch === '.' || ch === '!' || ch === '?') && text[i + 1] === ' ' && text[i + 2] >= 'A' && text[i + 2] <= 'Z') {
      out.push(text.slice(start, i + 1).trim());
      start = i + 2;
    }
  }
  out.push(text.slice(start).trim());
  return out.filter((s) => s.length > 0);
}

/**
 * The sentences from the passages that best answer the query, quoted as
 * written: used when the AI's draft does not pass verification, so the doctor
 * still sees what the sources say rather than nothing. A sentence must name
 * the query's clinical term or most of its words, and at most one sentence is
 * taken per passage.
 */
export function keySentences(
  passages: { passageId: string; text: string; sectionPath?: string }[],
  concepts: QueryConcepts,
  limit = 4
): KeySentence[] {
  const conceptWords = new Set(concepts.phrases.flat());
  const queryWords = new Set(concepts.layMapped ? [] : concepts.queryWords);
  const scored: (KeySentence & { score: number })[] = [];
  for (const passage of passages) {
    if (passage.sectionPath !== undefined && isEligibilitySection(passage.sectionPath)) continue;
    let best: (KeySentence & { score: number }) | undefined;
    for (const sentence of sentencesOf(passage.text)) {
      if (sentence.length < 40 || sentence.length > 400) continue;
      const words = new Set(queryTerms(sentence));
      const namesConcept = concepts.phrases.some((phrase) => phrase.every((w) => words.has(w)));
      const conceptHits = [...conceptWords].filter((w) => words.has(w)).length;
      const queryHits = [...queryWords].filter((w) => words.has(w)).length;
      const coversQuery = queryWords.size > 0 && queryHits / queryWords.size >= MIN_QUERY_COVERAGE;
      if (!namesConcept && !coversQuery) continue;
      const score = (namesConcept ? 10 : 0) + conceptHits + queryHits;
      if (!best || score > best.score) best = { text: sentence, passageId: passage.passageId, score };
    }
    if (best) scored.push(best);
  }
  return scored
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map(({ text, passageId }) => ({ text, passageId }));
}

/**
 * Whether a passage is about the query, decided before any AI sees it. Search
 * ranks by similarity, and the most similar passage in a small corpus can be
 * about something else entirely (a gynaecological case report for "heart
 * attack"). A passage counts only if:
 * - its document's title names one of the query's clinical terms, or its
 *   text names one in at least two sentences (a single mention is usually
 *   in passing: "patients with a prior myocardial infarction were excluded");
 * - or, for a query naming no clinical term, it contains most of the
 *   query's words.
 * Trial eligibility sections never count.
 */
export function passageIsAbout(
  passage: { title: string; sectionPath: string; text: string },
  concepts: QueryConcepts
): boolean {
  if (isEligibilitySection(passage.sectionPath)) {
    return false;
  }
  const titleWords = new Set(queryTerms(passage.title));
  const sentenceWords = sentencesOf(passage.text).map((s) => new Set(queryTerms(s)));
  for (const phrase of concepts.phrases) {
    if (phrase.every((w) => titleWords.has(w))) return true;
    if (sentenceWords.filter((words) => phrase.every((w) => words.has(w))).length >= 2) return true;
  }
  if (concepts.layMapped || concepts.queryWords.length === 0) {
    return false;
  }
  const words = new Set(queryTerms(`${passage.title} ${passage.sectionPath} ${passage.text}`));
  const covered = concepts.queryWords.filter((w) => words.has(w)).length;
  return covered / concepts.queryWords.length >= MIN_QUERY_COVERAGE;
}
