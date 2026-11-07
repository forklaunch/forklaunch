import {
  array,
  handlers,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  GovernanceConflictError,
  GovernanceNotFoundError
} from '../../domain/services/governance.service';

const governanceServiceFactory = ci.scopedResolver(tokens.GovernanceService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

const hmacAuth = { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } };

const LicenseSchema = {
  id: string,
  organizationId: string,
  sourceKey: string,
  licensee: string,
  reference: optional(string),
  status: string,
  validFrom: string,
  validUntil: optional(string)
};

const FlagSchema = {
  id: string,
  sourceKey: string,
  externalId: string,
  title: string,
  reason: string,
  status: string,
  flaggedBy: string,
  resolvedBy: optional(string),
  resolution: optional(string),
  createdAt: string
};

// Governance errors as HTTP statuses; anything else propagates.
function governanceStatus(error: unknown): { status: 404 | 409; message: string } | undefined {
  if (error instanceof GovernanceNotFoundError) {
    return { status: 404, message: error.message };
  }
  if (error instanceof GovernanceConflictError) {
    return { status: 409, message: error.message };
  }
  return undefined;
}

export const listSourceAccess = handlers.get(
  schemaValidator,
  '/source-access',
  {
    name: 'List Source Access',
    access: 'internal',
    summary:
      'Every source and whether an organization can search it (licensed sources need an active license)',
    auth: hmacAuth,
    query: { organizationId: optional(string) },
    responses: {
      200: array({
        sourceKey: string,
        name: string,
        requiresLicense: schemaValidator.boolean,
        accessible: schemaValidator.boolean
      })
    }
  },
  async (req, res) => {
    res.status(200).json(await governanceServiceFactory().sourceAccess(req.query.organizationId));
  }
);

export const registerLicensedSource = handlers.post(
  schemaValidator,
  '/source',
  {
    name: 'Register Licensed Source',
    access: 'internal',
    summary:
      'Registers a licensed source whose fetcher (wrapped in LicensedContentAdapter) is in the service code; it stays hidden from every organization without a license',
    auth: hmacAuth,
    body: {
      sourceKey: string,
      name: string,
      tier: string,
      licenseTerms: string,
      commercialUse: schemaValidator.boolean
    },
    responses: {
      200: { sourceKey: string, requiresLicense: schemaValidator.boolean },
      400: string,
      404: string,
      409: string
    }
  },
  async (req, res) => {
    if (!/^[a-z][a-z0-9_]{1,39}$/.test(req.body.sourceKey)) {
      res.status(400).send('sourceKey must be lowercase letters, digits and underscores');
      return;
    }
    try {
      res.status(200).json(await governanceServiceFactory().registerLicensedSource(req.body));
    } catch (error) {
      const mapped = governanceStatus(error);
      if (!mapped) throw error;
      res.status(mapped.status).send(mapped.message);
    }
  }
);

export const listContentLicenses = handlers.get(
  schemaValidator,
  '/content-license',
  {
    name: 'List Content Licenses',
    access: 'internal',
    summary: 'The content licenses an organization holds',
    auth: hmacAuth,
    query: { organizationId: string },
    responses: { 200: array(LicenseSchema) }
  },
  async (req, res) => {
    res.status(200).json(await governanceServiceFactory().listLicenses(req.query.organizationId));
  }
);

export const createContentLicense = handlers.post(
  schemaValidator,
  '/content-license',
  {
    name: 'Create Content License',
    access: 'internal',
    summary:
      'Records that an organization holds a license to a licensed source, making it searchable for them',
    auth: hmacAuth,
    body: {
      organizationId: string,
      sourceKey: string,
      licensee: string,
      reference: optional(string),
      validFrom: optional(string),
      validUntil: optional(string),
      createdBy: string
    },
    responses: { 200: LicenseSchema, 404: string, 409: string }
  },
  async (req, res) => {
    try {
      res.status(200).json(await governanceServiceFactory().createLicense(req.body));
    } catch (error) {
      const mapped = governanceStatus(error);
      if (!mapped) throw error;
      res.status(mapped.status).send(mapped.message);
    }
  }
);

export const revokeContentLicense = handlers.post(
  schemaValidator,
  '/content-license/:id/revoke',
  {
    name: 'Revoke Content License',
    access: 'internal',
    summary: 'Revokes a license; the source is hidden from that organization at once',
    auth: hmacAuth,
    params: { id: string },
    body: { organizationId: string },
    responses: { 200: LicenseSchema, 404: string, 409: string }
  },
  async (req, res) => {
    try {
      res
        .status(200)
        .json(await governanceServiceFactory().revokeLicense(req.body.organizationId, req.params.id));
    } catch (error) {
      const mapped = governanceStatus(error);
      if (!mapped) throw error;
      res.status(mapped.status).send(mapped.message);
    }
  }
);

export const flagContent = handlers.post(
  schemaValidator,
  '/content-flag',
  {
    name: 'Flag Content',
    access: 'internal',
    summary:
      'Flags a document (by passage id, or source key and external id) as wrong or unsafe; it is left out of search, answers and topic pages until a reviewer decides',
    auth: hmacAuth,
    body: {
      passageId: optional(string),
      sourceKey: optional(string),
      externalId: optional(string),
      reason: string,
      flaggedBy: string
    },
    responses: { 200: FlagSchema, 400: string, 404: string, 409: string }
  },
  async (req, res) => {
    if (!req.body.passageId && !(req.body.sourceKey && req.body.externalId)) {
      res.status(400).send('give passageId, or sourceKey and externalId');
      return;
    }
    try {
      res.status(200).json(await governanceServiceFactory().flag(req.body));
    } catch (error) {
      const mapped = governanceStatus(error);
      if (!mapped) throw error;
      res.status(mapped.status).send(mapped.message);
    }
  }
);

export const listContentFlags = handlers.get(
  schemaValidator,
  '/content-flag',
  {
    name: 'List Content Flags',
    access: 'internal',
    summary: 'Content flags, optionally filtered by status (open, resolved, rejected)',
    auth: hmacAuth,
    query: { status: optional(string) },
    responses: { 200: array(FlagSchema) }
  },
  async (req, res) => {
    res.status(200).json(await governanceServiceFactory().listFlags(req.query.status));
  }
);

export const resolveContentFlag = handlers.post(
  schemaValidator,
  '/content-flag/:id/resolve',
  {
    name: 'Resolve Content Flag',
    access: 'internal',
    summary:
      'Closes a flag as resolved (the document was corrected or removed) or rejected (the flag was unfounded)',
    auth: hmacAuth,
    params: { id: string },
    body: { outcome: string, resolvedBy: string, resolution: string },
    responses: { 200: FlagSchema, 400: string, 404: string, 409: string }
  },
  async (req, res) => {
    const { outcome, resolvedBy, resolution } = req.body;
    if (outcome !== 'resolved' && outcome !== 'rejected') {
      res.status(400).send('outcome must be resolved or rejected');
      return;
    }
    try {
      res
        .status(200)
        .json(await governanceServiceFactory().resolveFlag(req.params.id, outcome, resolvedBy, resolution));
    } catch (error) {
      const mapped = governanceStatus(error);
      if (!mapped) throw error;
      res.status(mapped.status).send(mapped.message);
    }
  }
);

export const approveTopic = handlers.post(
  schemaValidator,
  '/topic/:slug/approve',
  {
    name: 'Approve Topic',
    access: 'internal',
    summary:
      'Marks a topic page clinician-approved; refused while its question framework is still a draft',
    auth: hmacAuth,
    params: { slug: string },
    body: { approvedBy: string },
    responses: {
      200: { slug: string, status: string, approvedBy: string },
      404: string,
      409: string
    }
  },
  async (req, res) => {
    try {
      res.status(200).json(await governanceServiceFactory().approveTopic(req.params.slug, req.body.approvedBy));
    } catch (error) {
      const mapped = governanceStatus(error);
      if (!mapped) throw error;
      res.status(mapped.status).send(mapped.message);
    }
  }
);
