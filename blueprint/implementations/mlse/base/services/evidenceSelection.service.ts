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
 *   title or the passage). With `wholeTerm`, every word of one of the
 *   topic's names must appear: for topic pages, so a "laparoscopic
 *   appendectomy" paper is not evidence for laparoscopic cholecystectomy.
 *   Overview answers keep word matching, since their names include the
 *   doctor's own wording.
 * - A passage whose own heading names the item ("What is the treatment
 *   for a heart attack?" for Treatment) ranks first; then passages covering
 *   more hint words; the search score breaks ties.
 * - At most one passage per document, so an item cites several sources
 *   instead of one source filling every slot.
 */
export function selectEvidence<T extends EvidenceCandidate>(
  candidates: T[],
  // perDocument: passages one document may contribute (default 1)
  options: { hints: string[]; topicTerms: string[]; limit: number; perDocument?: number; wholeTerm?: boolean }
): SelectedEvidence<T>[] {
  const hints = new Set(options.hints.flatMap((hint) => queryTerms(hint)));
  const topicWords = new Set(options.topicTerms.flatMap((term) => queryTerms(term)));
  const topicNames = options.topicTerms.map((term) => queryTerms(term)).filter((words) => words.length > 0);
  const concernsTopic = (text: string) => {
    const words = queryTerms(text);
    if (!options.wholeTerm) {
      return words.some((w) => topicWords.has(w));
    }
    const present = new Set(words);
    return topicNames.some((name) => name.every((w) => present.has(w)));
  };

  const scored = candidates
    .map((candidate) => {
      const textWords = new Set(queryTerms(candidate.text));
      const hintHits = [...hints].filter((h) => textWords.has(h)).length;
      const pathHit = queryTerms(candidate.sectionPath).some((w) => hints.has(w)) ? 1 : 0;
      const aboutTopic = concernsTopic(`${candidate.title} ${candidate.text}`);
      return { candidate, hintHits, rank: hintHits * 2 + pathHit * 4 + candidate.score, aboutTopic };
    })
    .filter((s) => s.aboutTopic && s.hintHits > 0)
    .sort((a, b) => b.rank - a.rank || a.candidate.passageId.localeCompare(b.candidate.passageId));

  const usedDocuments = new Map<string, number>();
  const selected: SelectedEvidence<T>[] = [];
  for (const { candidate, hintHits } of scored) {
    const used = usedDocuments.get(candidate.documentKey) ?? 0;
    if (used >= (options.perDocument ?? 1)) {
      continue;
    }
    usedDocuments.set(candidate.documentKey, used + 1);
    selected.push({ ...candidate, hintHits });
    if (selected.length >= options.limit) {
      break;
    }
  }
  return selected;
}
