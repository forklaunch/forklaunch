import { Reranker } from '@forklaunch/interfaces-mlse/interfaces';
import {
  RerankCandidateDto,
  RerankScoreDto
} from '@forklaunch/interfaces-mlse/types';

export type FusedRank = {
  id: string;
  score: number;
  // indexes of the input lists that contained the id
  lists: number[];
};

/**
 * Reciprocal rank fusion: combines ranked lists from different retrievers
 * (keyword, vector, live) without having to calibrate their scores against
 * each other. An item's score is the sum of 1 / (k + rank) over the lists
 * that contain it; k = 60 is the value from the original paper.
 */
export function reciprocalRankFusion(
  rankedLists: string[][],
  k = 60
): FusedRank[] {
  const fused = new Map<string, FusedRank>();
  rankedLists.forEach((list, listIndex) => {
    const seen = new Set<string>();
    list.forEach((id, rank) => {
      if (seen.has(id)) {
        return;
      }
      seen.add(id);
      const entry = fused.get(id) ?? { id, score: 0, lists: [] };
      entry.score += 1 / (k + rank + 1);
      entry.lists.push(listIndex);
      fused.set(id, entry);
    });
  });
  return [...fused.values()].sort((a, b) => b.score - a.score || a.id.localeCompare(b.id));
}

const STOPWORDS = new Set([
  'a', 'an', 'and', 'are', 'as', 'at', 'be', 'by', 'for', 'from', 'in', 'is',
  'it', 'of', 'on', 'or', 'that', 'the', 'to', 'was', 'were', 'what', 'when',
  'which', 'who', 'with', 'how', 'why', 'does', 'do', 'after', 'before'
]);

// Lowercase word stems: a light plural/-ing strip, enough for scoring overlap.
export function queryTerms(text: string): string[] {
  const terms = text
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((t) => t.length > 1 && !STOPWORDS.has(t))
    .map((t) =>
      t.length > 4 && t.endsWith('ies')
        ? `${t.slice(0, -3)}y`
        : t.length > 4 && t.endsWith('ing')
          ? t.slice(0, -3)
          : t.length > 3 && t.endsWith('s') && !t.endsWith('ss')
            ? t.slice(0, -1)
            : t
    );
  return [...new Set(terms)];
}

/**
 * Default re-ranker: the share of query terms a passage contains, with a
 * small bonus for terms that appear together in the same order. Deliberately
 * simple and deterministic; a trained cross-encoder can implement the same
 * interface later.
 */
export class LexicalReranker implements Reranker {
  async rerank(
    query: string,
    candidates: RerankCandidateDto[]
  ): Promise<RerankScoreDto[]> {
    const terms = queryTerms(query);
    if (terms.length === 0) {
      return candidates.map((c) => ({ id: c.id, score: 0 }));
    }
    const phrase = terms.join(' ');
    return candidates.map((candidate) => {
      const words = queryTerms(candidate.text);
      const present = new Set(words);
      const coverage = terms.filter((t) => present.has(t)).length / terms.length;
      const phraseBonus = terms.length > 1 && words.join(' ').includes(phrase) ? 0.25 : 0;
      return { id: candidate.id, score: Math.min(1, coverage + phraseBonus) };
    });
  }
}
