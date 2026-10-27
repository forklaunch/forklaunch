import {
  array,
  handlers,
  number,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { classifyQuery, IMAGE_TYPES, ImageType } from '@forklaunch/implementation-mlse-base/services';
import { createHash } from 'node:crypto';
import { v4 } from 'uuid';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const searchServiceFactory = ci.scopedResolver(tokens.SearchService);
const savedSearchServiceFactory = ci.scopedResolver(tokens.SavedSearchService);
const querySuggestionService = ci.resolve(tokens.QuerySuggestionService);
const imageSearchService = ci.resolve(tokens.ImageSearchService);
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
      live: optional(string),
      // enables the organization's licensed sources; with userId, records
      // the search in the user's history
      organizationId: optional(string),
      userId: optional(string)
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

    // Classified before anything leaves the service: only literature
    // questions go to live sources, the ingestion queue or history text. A
    // query about one patient is searched in the stored corpus only.
    const { queryClass } = classifyQuery(query);
    // a dose question names a drug, not a patient: searched like literature
    // (only the answer path quotes the label instead of the AI)
    const isLiterature = queryClass === 'literature_lookup' || queryClass === 'dosage_question';

    const sourceKeys = req.query.sources
      ?.split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    if (sourceKeys && sourceKeys.length > 20) {
      res.status(400).send('sources may name at most 20 sources');
      return;
    }

    const response = await searchServiceFactory().search({
      query,
      limit,
      sourceKeys,
      caseReportsOnly: parseBoolean(req.query.caseReportsOnly, false),
      publishedAfter: req.query.publishedAfter,
      live: isLiterature && parseBoolean(req.query.live, true),
      ...(req.query.organizationId ? { organizationId: req.query.organizationId } : {})
    });

    if (isLiterature) {
      await enqueueWriteThrough(query, response.liveSources);
    }
    if (req.query.organizationId && req.query.userId) {
      await savedSearchServiceFactory().recordSearch(
        req.query.organizationId,
        req.query.userId,
        isLiterature ? query : null,
        queryClass
      );
    }
    res.status(200).json(response);
  }
);

const textQuery = (value: string) => {
  const text = value.trim();
  if (!text) return { error: 'q must not be empty' };
  if (text.length > 200) return { error: 'q must be at most 200 characters' };
  return { text };
};

export const suggestions = handlers.get(
  schemaValidator,
  '/suggestions',
  {
    name: 'Search Suggestions',
    access: 'internal',
    summary:
      'Condition, procedure and medicine names that complete a query as it is typed, from NLM and the built-in term list',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    query: {
      q: string
    },
    responses: {
      200: {
        suggestions: array(string)
      },
      400: string
    }
  },
  async (req, res) => {
    const query = textQuery(req.query.q);
    if (query.error !== undefined) {
      res.status(400).send(query.error);
      return;
    }
    res.status(200).json({ suggestions: await querySuggestionService.complete(query.text) });
  }
);

export const spelling = handlers.get(
  schemaValidator,
  '/spelling',
  {
    name: 'Search Spelling',
    access: 'internal',
    summary:
      'A corrected query ("heart atack" -> "heart attack") for "Did you mean", or no correction when the query looks right',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    query: {
      q: string
    },
    responses: {
      200: {
        correction: optional(string)
      },
      400: string
    }
  },
  async (req, res) => {
    const query = textQuery(req.query.q);
    if (query.error !== undefined) {
      res.status(400).send(query.error);
      return;
    }
    const correction = await querySuggestionService.correct(query.text);
    res.status(200).json(correction ? { correction } : {});
  }
);

export const images = handlers.get(
  schemaValidator,
  '/images',
  {
    name: 'Search Images',
    access: 'internal',
    summary:
      'Figures from open-access articles for a query (NLM Open-i), each with its caption, article and license; only licenses that allow commercial reuse',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    query: {
      q: string,
      type: optional(string),
      limit: optional(string)
    },
    responses: {
      200: {
        status: string,
        images: array({
          id: string,
          caption: string,
          thumbnailUrl: string,
          imageUrl: string,
          title: string,
          articleUrl: string,
          pmcid: string,
          journal: optional(string),
          year: optional(string),
          authors: optional(string),
          modality: optional(string),
          license: string,
          licenseUrl: string
        })
      },
      400: string
    }
  },
  async (req, res) => {
    const query = textQuery(req.query.q);
    if (query.error !== undefined) {
      res.status(400).send(query.error);
      return;
    }
    const type = req.query.type;
    if (type !== undefined && !(type in IMAGE_TYPES)) {
      res.status(400).send(`type must be one of ${Object.keys(IMAGE_TYPES).join(', ')}`);
      return;
    }
    const limit = req.query.limit ? Number(req.query.limit) : 24;
    if (!Number.isInteger(limit) || limit < 1 || limit > 48) {
      res.status(400).send('limit must be an integer from 1 to 48');
      return;
    }
    res.status(200).json(
      await imageSearchService.search(query.text, { limit, ...(type ? { type: type as ImageType } : {}) })
    );
  }
);

async function enqueueWriteThrough(
  term: string,
  liveSources: { sourceKey: string; status: string; documents: number }[]
): Promise<void> {
  const answered = liveSources.filter((s) => s.status === 'ok' && s.documents > 0);
  for (const source of answered) {
    const key = `mlse:live-enqueued:${source.sourceKey}:${createHash('sha256').update(term.toLowerCase()).digest('hex')}`;
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
