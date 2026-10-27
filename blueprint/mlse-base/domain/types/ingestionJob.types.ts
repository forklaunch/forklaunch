import type { WorkerEventEntity } from '@forklaunch/interfaces-worker/types';

/**
 * A request to refresh one source's corpus for a search term, carried on the
 * ingestion queue and processed by worker.ts.
 */
export type IngestionJob = WorkerEventEntity & {
  sourceKey: string;
  term: string;
  // documents to fetch; the worker applies a default when omitted
  limit?: number;
};
