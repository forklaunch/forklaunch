import { queryTerms } from './ranking.service';

export type EvidenceCandidate = {
  passageId: string;
  documentKey: string;
  title: string;
  sectionPath: string;
  text: string;
  score: number;
};

export type SelectedEvidence<T extends EvidenceCandidate> = T & {
  hintHits: number;
};

/**
 * Chooses the passages that answer one framework item.
 *
 * - The passage text must contain at least one of the item's hint words; a
 *   matching section title alone ("Eligibility") is not enough.
 * - Its document must concern the topic (the topic's words appear in the
 *   title or the passage).
 * - Passages covering more hint words rank first; a hint in the section
 *   title and the search score break ties.
 * - At most one passage per document, so an item cites several sources
 *   instead of one source filling every slot.
 */
export function selectEvidence<T extends EvidenceCandidate>(
  candidates: T[],
  options: { hints: string[]; topicTerms: string[]; limit: number }
): SelectedEvidence<T>[] {
  const hints = new Set(options.hints.flatMap((hint) => queryTerms(hint)));
  const topicWords = new Set(options.topicTerms.flatMap((term) => queryTerms(term)));

  const scored = candidates
    .map((candidate) => {
      const textWords = new Set(queryTerms(candidate.text));
      const hintHits = [...hints].filter((h) => textWords.has(h)).length;
      const pathHit = queryTerms(candidate.sectionPath).some((w) => hints.has(w)) ? 1 : 0;
      const aboutTopic = queryTerms(`${candidate.title} ${candidate.text}`).some((w) => topicWords.has(w));
      return { candidate, hintHits, rank: hintHits * 2 + pathHit + candidate.score, aboutTopic };
    })
    .filter((s) => s.aboutTopic && s.hintHits > 0)
    .sort((a, b) => b.rank - a.rank || a.candidate.passageId.localeCompare(b.candidate.passageId));

  const usedDocuments = new Set<string>();
  const selected: SelectedEvidence<T>[] = [];
  for (const { candidate, hintHits } of scored) {
    if (usedDocuments.has(candidate.documentKey)) {
      continue;
    }
    usedDocuments.add(candidate.documentKey);
    selected.push({ ...candidate, hintHits });
    if (selected.length >= options.limit) {
      break;
    }
  }
  return selected;
}
