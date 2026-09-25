import {
  WorkerFailureHandler,
  WorkerProcessFunction
} from '@forklaunch/interfaces-worker/types';
import { ci, tokens } from './bootstrapper';
import { IngestionJob } from './domain/types/ingestionJob.types';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const contentSourceProvider = ci.resolve(tokens.ContentSourceProvider);
const ingestionServiceFactory = ci.scopedResolver(tokens.IngestionService);

// Documents fetched per job when the request does not say.
const DEFAULT_JOB_LIMIT = 20;

// Keep the source registry in step with the provider before any job runs,
// so lastRefreshedAt has a row to land on.
const ready = ingestionServiceFactory()
  .syncSources(contentSourceProvider.describe())
  .catch((error) => {
    openTelemetryCollector.error('Could not sync the source registry', error);
  });

/**
 * Consumes corpus-refresh requests: fetch documents for the job's term from
 * its source, then license-check, deduplicate, chunk, embed and store them.
 * A failing job is returned to the queue for retry, never dropped silently.
 */
const processIngestionJobs: WorkerProcessFunction<IngestionJob> = async (
  jobs
) => {
  await ready;
  const failedJobs: { value: IngestionJob; error: Error }[] = [];

  for (const job of jobs) {
    try {
      // a fresh scoped service (and EntityManager) per job
      const result = await ingestionServiceFactory().ingest({
        sourceKey: job.sourceKey,
        term: job.term,
        limit: job.limit ?? DEFAULT_JOB_LIMIT
      });
      openTelemetryCollector.info('Corpus refresh completed', result);
      job.processed = true;
    } catch (error) {
      failedJobs.push({
        value: job,
        error: error instanceof Error ? error : new Error(String(error))
      });
    }
  }

  return failedJobs;
};

const processFailures: WorkerFailureHandler<IngestionJob> = async (jobs) => {
  jobs.forEach((job) => {
    openTelemetryCollector.error(
      'Corpus ingestion job failed',
      job.error,
      job.value
    );
  });
};

const ingestionJobConsumerFactory = ci.resolve(tokens.IngestionJobConsumer);
const consumer = ingestionJobConsumerFactory(
  processIngestionJobs,
  processFailures
);

consumer.start();
openTelemetryCollector.info('MLSE worker started, consuming ingestion queue');
