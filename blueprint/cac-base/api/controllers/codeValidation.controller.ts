import {
  handlers,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.CodeValidationService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

// Free code sets only — ICD-10-CM and HCPCS, backed by scripts/refresh-code-sets.ts
// (§7). No CPT here: ForkLaunch never holds real CPT content, and this is
// exactly the surface the forklaunch-platform validation UI (§10) is meant
// to call.
export const validateIcd10Code = handlers.get(
  schemaValidator,
  '/icd10/:code',
  {
    name: 'Validate ICD-10-CM Code',
    access: 'internal',
    summary: 'Checks whether a code is a known ICD-10-CM diagnosis code',
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
        valid: schemaValidator.boolean,
        code: string,
        description: optional(string)
      }
    }
  },
  async (req, res) => {
    const { code } = req.params;
    openTelemetryCollector.debug('Validating ICD-10-CM code', { code });
    const result = await serviceFactory().validateIcd10(code);
    res.status(200).json(result);
  }
);

export const validateHcpcsCode = handlers.get(
  schemaValidator,
  '/hcpcs/:code',
  {
    name: 'Validate HCPCS Code',
    access: 'internal',
    summary: 'Checks whether a code is a known HCPCS Level II code',
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
        valid: schemaValidator.boolean,
        code: string,
        description: optional(string)
      }
    }
  },
  async (req, res) => {
    const { code } = req.params;
    openTelemetryCollector.debug('Validating HCPCS code', { code });
    const result = await serviceFactory().validateHcpcs(code);
    res.status(200).json(result);
  }
);
