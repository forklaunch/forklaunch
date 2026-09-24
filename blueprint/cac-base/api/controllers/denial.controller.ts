import {
  handlers,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import type { WorklistStatus } from '../../domain/enum/worklistStatus.enum';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.DenialWorklistService);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

// biller:view_denials / biller:manage_denials — not biller:view_remittance
// as originally sketched in plan §3: there's no remittance data anymore
// (§8, §14), only the scrubbing engine's own findings. Split read vs.
// resolve so a read-only biller role can exist without also granting
// write access, per §3's least-privilege framing.
const VIEW_DENIALS_PERMISSIONS = new Set(['biller:view_denials']);
const MANAGE_DENIALS_PERMISSIONS = new Set(['biller:manage_denials']);

const denialResponseSchema = {
  id: string,
  claimId: string,
  carcCode: string,
  category: string,
  worklistStatus: string,
  resolvedAt: optional(string)
};

// Denial worklist API — the one piece of the old "eligibility & remittance"
// phase that never depended on Stedi (plan §12 item 12, §14). Every row
// here comes from ScrubbingService (§6); this is a query/status layer over
// Denial rows the claim engine already creates, not a new source of them.
// Protected + JWT, not internal/HMAC — biller-facing, same reasoning as
// claim.controller.ts (RBAC verification pass, §14 PR 5).
export const listDenials = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'List Denials',
    access: 'protected',
    summary: 'Lists denial worklist entries, optionally filtered',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      sessionSchema: {
        organizationId: string
      },
      allowedPermissions: VIEW_DENIALS_PERMISSIONS
    },
    query: {
      claimId: optional(string),
      worklistStatus: optional(string)
    },
    responses: {
      200: schemaValidator.array(denialResponseSchema)
    }
  },
  async (req, res) => {
    const { claimId, worklistStatus } = req.query;
    const organizationId = req.session?.organizationId;
    openTelemetryCollector.debug('Listing denials', {
      claimId,
      worklistStatus
    });
    const denials = await serviceFactory({ context: { tenantId: organizationId } }).listDenials(organizationId, {
      claimId,
      worklistStatus: worklistStatus as WorklistStatus | undefined
    });
    res.status(200).json(
      denials.map((denial) => ({
        id: denial.id,
        claimId: denial.claim.id,
        carcCode: denial.carcCode,
        category: denial.category,
        worklistStatus: denial.worklistStatus,
        resolvedAt: denial.resolvedAt?.toISOString()
      }))
    );
  }
);

export const getDenial = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Denial',
    access: 'protected',
    summary: 'Fetches a single denial worklist entry',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      sessionSchema: {
        organizationId: string
      },
      allowedPermissions: VIEW_DENIALS_PERMISSIONS
    },
    params: {
      id: string
    },
    responses: {
      200: denialResponseSchema,
      404: string
    }
  },
  async (req, res) => {
    const { id } = req.params;
    const organizationId = req.session?.organizationId;
    const denial = await serviceFactory({ context: { tenantId: organizationId } }).getDenial(organizationId, id);

    if (!denial) {
      res.status(404).send(`Denial '${id}' not found`);
      return;
    }

    res.status(200).json({
      id: denial.id,
      claimId: denial.claim.id,
      carcCode: denial.carcCode,
      category: denial.category,
      worklistStatus: denial.worklistStatus,
      resolvedAt: denial.resolvedAt?.toISOString()
    });
  }
);

export const resolveDenial = handlers.post(
  schemaValidator,
  '/:id/resolve',
  {
    name: 'Resolve Denial',
    access: 'protected',
    summary: 'Marks a denial worklist entry resolved',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      sessionSchema: {
        organizationId: string
      },
      allowedPermissions: MANAGE_DENIALS_PERMISSIONS
    },
    params: {
      id: string
    },
    responses: {
      200: denialResponseSchema,
      404: string
    }
  },
  async (req, res) => {
    const { id } = req.params;
    const organizationId = req.session?.organizationId;
    const denial = await serviceFactory({ context: { tenantId: organizationId } }).resolveDenial(organizationId, id);

    if (!denial) {
      res.status(404).send(`Denial '${id}' not found`);
      return;
    }

    openTelemetryCollector.info('Resolved denial via API', { id });
    res.status(200).json({
      id: denial.id,
      claimId: denial.claim.id,
      carcCode: denial.carcCode,
      category: denial.category,
      worklistStatus: denial.worklistStatus,
      resolvedAt: denial.resolvedAt?.toISOString()
    });
  }
);
