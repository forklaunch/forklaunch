import {
  FetchedDocumentDto,
  SourceQueryDto
} from '../types/document.types';

// Fetches documents for one source. Implementations only translate the
// source's API into FetchedDocumentDto; licensing, deduplication, chunking
// and storage happen afterwards, the same way for every source.
export interface SourceFetcher {
  readonly sourceKey: string;
  // `signal` cancels the fetch, its requests included
  fetchDocuments: (
    query: SourceQueryDto,
    options?: { signal?: AbortSignal }
  ) => Promise<FetchedDocumentDto[]>;
}
