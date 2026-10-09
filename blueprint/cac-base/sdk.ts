import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import {
  buildClaim,
  describeCodeSet,
  eraseUserData,
  exportUserData,
  getClaimAnalyticsSummary,
  getDenial,
  listDenials,
  lookupProcedureCode,
  resolveDenial,
  scrubClaim,
  validateHcpcsCode,
  validateIcd10Code
} from './api/controllers';

export type CacSdk = {
  compliance: {
    eraseUserData: typeof eraseUserData;
    exportUserData: typeof exportUserData;
  };
  codeSet: {
    describeCodeSet: typeof describeCodeSet;
    lookupProcedureCode: typeof lookupProcedureCode;
  };
  claim: {
    buildClaim: typeof buildClaim;
    scrubClaim: typeof scrubClaim;
  };
  denial: {
    listDenials: typeof listDenials;
    getDenial: typeof getDenial;
    resolveDenial: typeof resolveDenial;
  };
  analytics: {
    getClaimAnalyticsSummary: typeof getClaimAnalyticsSummary;
  };
  codeValidation: {
    validateIcd10Code: typeof validateIcd10Code;
    validateHcpcsCode: typeof validateHcpcsCode;
  };
};

export const cacSdkClient = {
  compliance: {
    eraseUserData,
    exportUserData
  },
  codeSet: {
    describeCodeSet,
    lookupProcedureCode
  },
  claim: {
    buildClaim,
    scrubClaim
  },
  denial: {
    listDenials,
    getDenial,
    resolveDenial
  },
  analytics: {
    getClaimAnalyticsSummary
  },
  codeValidation: {
    validateIcd10Code,
    validateHcpcsCode
  }
} satisfies CacSdk;

export type CacSdkClient = MapToSdk<SchemaValidator, CacSdk>;
