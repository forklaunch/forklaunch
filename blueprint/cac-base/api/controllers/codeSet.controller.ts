import { handlers, schemaValidator, string } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const codeSetProvider = ci.resolve(tokens.CodeSetProvider);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

export const describeCodeSet = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'Describe Code Set',
    access: 'internal',
    summary:
      'Reports which procedure code-set provider is active (mock vs. licensed CPT)',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: {
        codeSetType: string,
        licensed: schemaValidator.boolean
      }
    }
  },
  async (_req, res) => {
    const descriptor = codeSetProvider.describe();
    openTelemetryCollector.debug('Describing active code set', descriptor);
    res.status(200).json(descriptor);
  }
);

export const lookupProcedureCode = handlers.get(
  schemaValidator,
  '/:code',
  {
    name: 'Lookup Procedure Code',
    access: 'internal',
    summary:
      'Looks up a procedure code against the currently active code-set provider',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: {
      code: string
    },
    responses: {
      200: {
        code: string,
        description: string
      },
      404: string
    }
  },
  async (req, res) => {
    const { code } = req.params;
    openTelemetryCollector.debug('Looking up procedure code', { code });
    const result = await codeSetProvider.lookupProcedureCode({ code });

    if (!result) {
      res.status(404).send(`Procedure code '${code}' not found`);
      return;
    }

    res.status(200).json(result);
  }
);
