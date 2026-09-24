import type { WorkerEventEntity } from '@forklaunch/interfaces-worker/types';

/**
 * A request to refresh one source's corpus, carried on the ingestion queue.
 * The worker picks these up; fetchers per source land with corpus ingestion.
 */
export type IngestionJob = WorkerEventEntity & {
  sourceKey: string;
};
