import { LicenseScope } from './contentSource.types';

// A passage with the document facts an answer needs to cite it.
export type CitablePassageDto = {
  // stable id: the stored chunk id, or 'live:<source>:<externalId>:<ordinal>'
  passageId: string;
  origin: 'corpus' | 'live';
  sourceKey: string;
  externalId: string;
  title: string;
  url: string;
  publishedAt?: string;
  isCaseReport: boolean;
  licenseScope: LicenseScope;
  sectionPath: string;
  text: string;
};

export type RerankCandidateDto = {
  id: string;
  text: string;
};

export type RerankScoreDto = {
  id: string;
  score: number;
};

export type LiveSourceStatus = 'ok' | 'cached' | 'timeout' | 'error';

export type LiveSourceResultDto = {
  sourceKey: string;
  status: LiveSourceStatus;
  documents: number;
  error?: string;
};

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
