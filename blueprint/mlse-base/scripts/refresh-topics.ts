import { v4 } from 'uuid';
import { ci, tokens } from '../bootstrapper';
import { Topic } from '../persistence/entities/topic.entity';

/**
 * Queues ingestion of the papers each topic page is built from: its corpus
 * queries (papers whose major subject is the topic, and those on its
 * technique) against PubMed Central, PubMed and PubMed's guidelines.
 * Assemble the page once the worker has stored them.
 *
 *   pnpm topic:refresh                    every topic with corpus queries
 *   pnpm topic:refresh appendectomy ...   only these
 *
 *   CORPUS_LIMIT  papers per source and query (default: 25)
 */
const NCBI_SOURCES = ['pmc_oa', 'pubmed', 'guidelines'];

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const sourceFetchers = ci.resolve(tokens.SourceFetchers);
const producer = ci.scopedResolver(tokens.IngestionJobProducer)();

async function main() {
  const slugs = process.argv.slice(2);
  const em = ci.scopedResolver(tokens.EntityManager)();
  const topics = (
    await em.find(Topic, slugs.length > 0 ? { slug: { $in: slugs } } : {})
  ).filter((topic) => topic.corpusQueries.length > 0);
  const missing = slugs.filter((slug) => !topics.some((topic) => topic.slug === slug));
  if (missing.length > 0) {
    throw new Error(`Unknown topics, or topics without corpus queries: ${missing.join(', ')}`);
  }
  if (topics.length === 0) {
    throw new Error('No topic has corpus queries; nothing to refresh');
  }

  const sources = NCBI_SOURCES.filter((key) => sourceFetchers.has(key));
  const limit = Number(process.env.CORPUS_LIMIT ?? 25);
  const now = new Date();
  const jobs = topics.flatMap((topic) =>
    topic.corpusQueries.flatMap((term) =>
      sources.map((sourceKey) => ({
        id: v4(),
        sourceKey,
        term,
        limit,
        retryCount: 0,
        processed: false,
        createdAt: now,
        updatedAt: now
      }))
    )
  );
  await producer.enqueueBatchJobs(jobs);
  openTelemetryCollector.info('Queued topic corpus jobs', {
    jobs: jobs.length,
    sources,
    topics: topics.map((topic) => topic.slug)
  });
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error('[refresh-topics] Fatal error', error);
    process.exit(1);
  });
