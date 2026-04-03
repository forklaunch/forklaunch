import { handlers, schemaValidator, string } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  SampleWorkerRequestMapper,
  SampleWorkerResponseMapper
} from '../../domain/mappers/sampleWorker.mappers';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);
const serviceFactory = ci.scopedResolver(tokens.SampleWorkerService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

// GET endpoint handler that returns a simple message
export const sampleWorkerGet = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'sampleWorkerGet',
    access: 'internal',
    summary: 'Get sample worker',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: {
      id: string
    },
    responses: {
      200: SampleWorkerResponseMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('SampleWorkerGet', req.params);
    res.status(200).json({
      message: 'SampleWorker',
      processed: false,
      retryCount: 0
    });
  }
);

// POST endpoint handler that processes request body and returns response from service
export const sampleWorkerPost = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'sampleWorkerPost',
    access: 'internal',
    summary: 'Create sample worker',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: SampleWorkerRequestMapper.schema,
    responses: {
      200: SampleWorkerResponseMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('SampleWorkerPost', req.body);
    res.status(200).json(await serviceFactory().sampleWorkerPost(req.body));
  }
);
