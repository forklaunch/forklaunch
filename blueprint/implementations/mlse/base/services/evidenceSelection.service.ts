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
 *   doctor's own wording. With `wholeTerm`, a hint that is a word of the
 *   topic's name does not count either: every passage on a cesarean
 *   delivery says "delivery", whichever item it answers.
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
  const topicWords = new Set(options.topicTerms.flatMap((term) => queryTerms(term)));
  const hints = new Set(
    options.hints.flatMap((hint) => queryTerms(hint)).filter((h) => !options.wholeTerm || !topicWords.has(h))
  );
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

/**
 * Where an item ranks among a page's items for one passage, by how close the
 * passage's embedding is to each item's question (1 = the item it fits
 * best). A passage counts as evidence for an item only if the item ranks
 * near the top: an introduction ("Cesarean delivery is the most common major
 * operation…") is about as close to every question and answers none, so it
 * rarely ranks any one item first. Similarity alone cannot tell: on real
 * papers, passages that answer their item and passages that do not were
 * equally similar to it (median cosine 0.70 for both).
 */
export function itemFitRank(passage: number[], questions: Map<string, number[]>, itemKey: string): number {
  const own = questions.get(itemKey);
  if (!own) {
    return 1;
  }
  const similarity = cosine(passage, own);
  let rank = 1;
  for (const [key, question] of questions) {
    if (key !== itemKey && cosine(passage, question) > similarity) {
      rank++;
    }
  }
  return rank;
}

function cosine(a: number[], b: number[]): number {
  let dot = 0;
  let normA = 0;
  let normB = 0;
  for (let i = 0; i < a.length; i++) {
    dot += a[i] * (b[i] ?? 0);
    normA += a[i] * a[i];
    normB += (b[i] ?? 0) * (b[i] ?? 0);
  }
  return normA && normB ? dot / Math.sqrt(normA * normB) : 0;
}
