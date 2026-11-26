import { v4 } from 'uuid';
import { ci, tokens } from '../bootstrapper';

/**
 * Queues a corpus refresh for every configured topic across every fetchable
 * source. Run on a schedule by the hosting environment (for example a
 * Kubernetes CronJob); the worker does the fetching.
 *
 *   CORPUS_TOPICS   comma-separated search terms (required)
 *   CORPUS_SOURCES  comma-separated source keys (default: all fetchable)
 *   CORPUS_LIMIT    documents per source and topic (default: 20)
 */
const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const sourceFetchers = ci.resolve(tokens.SourceFetchers);
const producer = ci.scopedResolver(tokens.IngestionJobProducer)();

const list = (value: string | undefined) =>
  (value ?? '')
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);

async function main() {
  const topics = list(process.env.CORPUS_TOPICS);
  if (topics.length === 0) {
    throw new Error('CORPUS_TOPICS is empty; nothing to refresh');
  }

  const requested = list(process.env.CORPUS_SOURCES);
  const unknown = requested.filter((key) => !sourceFetchers.has(key));
  if (unknown.length > 0) {
    throw new Error(`Unknown or non-fetchable sources: ${unknown.join(', ')}`);
  }
  const sources = requested.length > 0 ? requested : sourceFetchers.keys();
  const limit = Number(process.env.CORPUS_LIMIT ?? 20);

  const now = new Date();
  const jobs = sources.flatMap((sourceKey) =>
    topics.map((term) => ({
      id: v4(),
      sourceKey,
      term,
      limit,
      retryCount: 0,
      processed: false,
      createdAt: now,
      updatedAt: now
    }))
  );
  await producer.enqueueBatchJobs(jobs);
  openTelemetryCollector.info('Queued corpus refresh jobs', {
    jobs: jobs.length,
    sources,
    topics
  });
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error('[refresh-corpus] Fatal error', error);
    process.exit(1);
  });
