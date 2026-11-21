import { LicenseScope } from './contentSource.types';

// Retrieval types shared by the implementation and its interfaces. The
// module's HTTP request and response shapes live in the module itself.

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
