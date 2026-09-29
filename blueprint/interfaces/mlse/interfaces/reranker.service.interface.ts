import { RerankCandidateDto, RerankScoreDto } from '../types/search.types';

// Re-orders search candidates by relevance to the query. The default is a
// lexical scorer; a trained cross-encoder can replace it without touching
// the search pipeline.
export interface Reranker {
  rerank: (
    query: string,
    candidates: RerankCandidateDto[]
  ) => Promise<RerankScoreDto[]>;
}
