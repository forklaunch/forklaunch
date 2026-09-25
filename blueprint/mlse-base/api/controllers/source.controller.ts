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
const contentSourceProvider = ci.resolve(tokens.ContentSourceProvider);
const sourceFetchers = ci.resolve(tokens.SourceFetchers);
const ingestionJobProducerFactory = ci.scopedResolver(tokens.IngestionJobProducer);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

// Keeps a single refresh bounded; larger backfills run as several jobs.
const MAX_REFRESH_LIMIT = 200;

const SourceDescriptorSchema = {
  id: string,
  name: string,
  tier: string,
  licenseTerms: string,
  commercialUse: schemaValidator.boolean,
  liveQuery: schemaValidator.boolean
};

export const listSources = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'List Sources',
    access: 'internal',
    summary:
      'Lists the medical content sources MLSE draws on, with their license terms and whether each is queried live',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: array(SourceDescriptorSchema)
    }
  },
  async (_req, res) => {
    const sources = contentSourceProvider.describe();
    openTelemetryCollector.debug('Listing content sources', {
      count: sources.length
    });
    res.status(200).json(sources);
  }
);

export const refreshSource = handlers.post(
  schemaValidator,
  '/:sourceKey/refresh',
  {
    name: 'Refresh Source',
    access: 'internal',
    summary:
      'Queues a corpus refresh: fetch documents matching a term from one source, then license-check, chunk, embed and store them',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: {
      sourceKey: string
    },
    body: {
      term: string,
      limit: optional(number)
    },
    responses: {
      202: {
        jobId: string,
        sourceKey: string,
        term: string,
        limit: number
      },
      400: string,
      404: string
    }
  },
  async (req, res) => {
    const { sourceKey } = req.params;
    const term = req.body.term.trim();
    const limit = req.body.limit ?? 20;

    if (!sourceFetchers.has(sourceKey)) {
      res
        .status(404)
        .send(
          `'${sourceKey}' is not a fetchable source (MeSH is loaded from its descriptor file)`
        );
      return;
    }
    if (!term) {
      res.status(400).send('term must not be empty');
      return;
    }
    if (!Number.isInteger(limit) || limit < 1 || limit > MAX_REFRESH_LIMIT) {
      res.status(400).send(`limit must be an integer from 1 to ${MAX_REFRESH_LIMIT}`);
      return;
    }

    const now = new Date();
    const jobId = v4();
    await ingestionJobProducerFactory().enqueueJob({
      id: jobId,
      sourceKey,
      term,
      limit,
      retryCount: 0,
      processed: false,
      createdAt: now,
      updatedAt: now
    });

    openTelemetryCollector.info('Queued corpus refresh', { jobId, sourceKey, term, limit });
    res.status(202).json({ jobId, sourceKey, term, limit });
  }
);
