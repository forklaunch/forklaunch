// The search endpoint's request and response shapes.
import type {
  CitablePassageDto,
  LiveSourceResultDto
} from '@forklaunch/interfaces-mlse/types';

export type SearchRequestDto = {
  query: string;
  limit?: number;
  sourceKeys?: string[];
  caseReportsOnly?: boolean;
  // ISO date; documents published before it are left out
  publishedAfter?: string;
  // also query live sources at search time (default true)
  live?: boolean;
  // licensed sources are searched only for an organization holding an
  // active license; without one they are left out
  organizationId?: string;
  // also ask PubMed for review articles and guidelines on the topic, for
  // overview answers
  preferReviews?: boolean;
};

export type SearchResultDto = CitablePassageDto & {
  score: number;
  // which retrievers found this passage
  matchedBy: ('keyword' | 'vector' | 'live')[];
};

export type SearchResponseDto = {
  query: string;
  // the query plus MeSH preferred terms and synonyms it matched
  expandedTerms: string[];
  results: SearchResultDto[];
  liveSources: LiveSourceResultDto[];
};
