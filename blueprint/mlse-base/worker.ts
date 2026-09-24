import {
  WorkerFailureHandler,
  WorkerProcessFunction
} from '@forklaunch/interfaces-worker/types';
import { ci, tokens } from './bootstrapper';
import { IngestionJob } from './domain/types/ingestionJob.types';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const contentSourceProvider = ci.resolve(tokens.ContentSourceProvider);

const knownSources = new Set(
  contentSourceProvider.describe().map((source) => source.id)
);

/**
 * Consumes corpus-refresh requests. Source fetchers (openFDA, DailyMed,
 * ClinicalTrials.gov, MeSH, PubMed, PMC OA) plug in here with corpus
 * ingestion; until then a job for a known source is acknowledged and one for
 * an unknown source is reported as a failure rather than silently dropped.
 */
const processIngestionJobs: WorkerProcessFunction<IngestionJob> = async (
  jobs
) => {
  const failedJobs: { value: IngestionJob; error: Error }[] = [];

  for (const job of jobs) {
    if (!knownSources.has(job.sourceKey)) {
      failedJobs.push({
        value: job,
        error: new Error(`Unknown content source '${job.sourceKey}'`)
      });
      continue;
    }

    openTelemetryCollector.info('Received corpus refresh request', {
      sourceKey: job.sourceKey
    });
    job.processed = true;
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
