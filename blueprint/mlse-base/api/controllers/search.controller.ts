import {
  array,
  handlers,
  number,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { v4 } from 'uuid';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const searchServiceFactory = ci.scopedResolver(tokens.SearchService);
const ingestionJobProducerFactory = ci.scopedResolver(tokens.IngestionJobProducer);
const ttlCache = ci.resolve(tokens.TtlCache);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

// A term that live retrieval answered is queued for ingestion at most this
// often per source, so the stored corpus follows what doctors search for.
const WRITE_THROUGH_INTERVAL_MS = 24 * 60 * 60 * 1000;

const SearchResultSchema = {
  passageId: string,
  origin: string,
  sourceKey: string,
  externalId: string,
  title: string,
  url: string,
  publishedAt: optional(string),
  isCaseReport: schemaValidator.boolean,
  licenseScope: string,
  sectionPath: string,
  text: string,
  score: number,
  matchedBy: array(string)
};

const parseBoolean = (value: string | undefined, fallback: boolean) =>
  value === undefined ? fallback : ['1', 'true', 'yes'].includes(value.toLowerCase());

export const search = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'Search',
    access: 'internal',
    summary:
      'Hybrid search over the corpus (keyword, vector, MeSH synonyms) plus live medical sources; returns citable passages',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    query: {
      q: string,
      limit: optional(string),
      sources: optional(string),
      caseReportsOnly: optional(string),
      publishedAfter: optional(string),
      live: optional(string)
    },
    responses: {
      200: {
        query: string,
        expandedTerms: array(string),
        results: array(SearchResultSchema),
        liveSources: array({
          sourceKey: string,
          status: string,
          documents: number,
          error: optional(string)
        })
      },
      400: string
    }
  },
  async (req, res) => {
    const query = req.query.q.trim();
    if (!query) {
      res.status(400).send('q must not be empty');
      return;
    }
    if (query.length > 500) {
      res.status(400).send('q must be at most 500 characters');
      return;
    }
    const limit = req.query.limit ? Number(req.query.limit) : 10;
    if (!Number.isInteger(limit) || limit < 1 || limit > 50) {
      res.status(400).send('limit must be an integer from 1 to 50');
      return;
    }
    if (req.query.publishedAfter && !/^\d{4}(-\d{2}(-\d{2})?)?$/.test(req.query.publishedAfter)) {
      res.status(400).send('publishedAfter must be an ISO date (YYYY, YYYY-MM or YYYY-MM-DD)');
      return;
    }

    const response = await searchServiceFactory().search({
      query,
      limit,
      sourceKeys: req.query.sources
        ?.split(',')
        .map((s) => s.trim())
        .filter(Boolean),
      caseReportsOnly: parseBoolean(req.query.caseReportsOnly, false),
      publishedAfter: req.query.publishedAfter,
      live: parseBoolean(req.query.live, true)
    });

    await enqueueWriteThrough(query, response.liveSources);
    res.status(200).json(response);
  }
);

async function enqueueWriteThrough(
  term: string,
  liveSources: { sourceKey: string; status: string; documents: number }[]
): Promise<void> {
  const answered = liveSources.filter((s) => s.status === 'ok' && s.documents > 0);
  for (const source of answered) {
    const key = `mlse:live-enqueued:${source.sourceKey}:${term.toLowerCase()}`;
    try {
      if (await ttlCache.peekRecord(key)) {
        continue;
      }
      await ttlCache.putRecord({ key, value: true, ttlMilliseconds: WRITE_THROUGH_INTERVAL_MS });
      const now = new Date();
      await ingestionJobProducerFactory().enqueueJob({
        id: v4(),
        sourceKey: source.sourceKey,
        term,
        retryCount: 0,
        processed: false,
        createdAt: now,
        updatedAt: now
      });
    } catch (error) {
      // the search already answered; a queueing problem must not fail it
      openTelemetryCollector.error('Could not queue write-through ingestion', error);
    }
  }
}
