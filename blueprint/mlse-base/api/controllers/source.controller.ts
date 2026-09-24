import {
  array,
  handlers,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const contentSourceProvider = ci.resolve(tokens.ContentSourceProvider);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

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
