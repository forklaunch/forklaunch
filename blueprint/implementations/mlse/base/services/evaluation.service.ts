export type SourceRef = {
  sourceKey: string;
  externalId: string;
};

export type GoldQuestion = {
  id: string;
  query: string;
  // documents a clinician expects a good answer to draw on
  expectedSources: SourceRef[];
};

export type QuestionScore = {
  id: string;
  query: string;
  found: number;
  expected: number;
  completeness: number;
  missing: SourceRef[];
};

const refKey = (ref: SourceRef) => `${ref.sourceKey}:${ref.externalId}`;

/**
 * Retrieval completeness: the share of a question's expected documents that
 * appear anywhere in the top-k results. Documents, not passages, are the unit,
 * because one document may be cited through any of its passages.
 */
export function retrievalCompleteness(
  question: GoldQuestion,
  results: SourceRef[],
  k: number
): QuestionScore {
  const top = new Set(results.slice(0, k).map(refKey));
  const unique = [...new Map(question.expectedSources.map((r) => [refKey(r), r])).values()];
  const missing = unique.filter((ref) => !top.has(refKey(ref)));
  const found = unique.length - missing.length;
  return {
    id: question.id,
    query: question.query,
    found,
    expected: unique.length,
    completeness: unique.length === 0 ? 1 : found / unique.length,
    missing
  };
}

export function summarizeEvaluation(scores: QuestionScore[]): {
  questions: number;
  meanCompleteness: number;
  fullyAnswered: number;
} {
  const questions = scores.length;
  return {
    questions,
    meanCompleteness:
      questions === 0 ? 0 : scores.reduce((sum, s) => sum + s.completeness, 0) / questions,
    fullyAnswered: scores.filter((s) => s.completeness === 1).length
  };
}
