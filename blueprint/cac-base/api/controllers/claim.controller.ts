import {
  handlers,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { UniqueConstraintViolationException } from '@mikro-orm/core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.ClaimService);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

// coder:manage_claims — not coder:submit_claim as originally sketched in
// plan §3: cac-base never submits a claim anywhere (§8, §14), so a
// "submit" permission would promise something these routes don't do.
// Covers both building and scrubbing a claim; a real IAM deployment seeds
// this slug the same way it seeds its own platform:read/platform:write
// permissions (§3 "Migrations").
const MANAGE_CLAIMS_PERMISSIONS = new Set(['coder:manage_claims']);

// Claim engine + three-layer scrubbing (§6) — mock codes only, per Phase 2 /
// PR 3 scope (plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §10, §14).
// Protected + JWT, not internal/HMAC — these are the actual coder-facing
// actions a real adopter's front-end calls on behalf of a logged-in coder
// (RBAC verification pass, §14 PR 5).
//
// sessionSchema + explicit organizationId passed into every service call —
// belt AND suspenders, not either/or. The service methods filter their own
// queries by organizationId (found via the e2e suite: a valid token for one
// organization could read and resolve another organization's
// claims/denials — see plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §12).
// That alone left the DI-level EntityManager itself unscoped: the
// framework's MikroORM tenant filter fails OPEN when no tenant context is
// set, and PHI would be encrypted under one shared key for every org
// instead of each org's own key. `serviceFactory({ context: { tenantId } })`
// is what actually engages both — see wrapEmWithTenantContext's own doc
// comment in framework/core/src/persistence/tenantEm.ts.
export const buildClaim = handlers.post(
  schemaValidator,
  '/build',
  {
    name: 'Build Claim',
    access: 'protected',
    summary:
      "Builds a claim from an encounter's charges and diagnoses",
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      sessionSchema: {
        organizationId: string
      },
      allowedPermissions: MANAGE_CLAIMS_PERMISSIONS
    },
    body: {
      encounterId: string
    },
    responses: {
      200: {
        id: string,
        status: string,
        codeSetType: string
      },
      404: string,
      409: string
    }
  },
  async (req, res) => {
    const { encounterId } = req.body;
    const organizationId = req.session?.organizationId;
    openTelemetryCollector.debug('Building claim', { encounterId });
    try {
      const claim = await serviceFactory({
        context: { tenantId: organizationId }
      }).buildClaim(organizationId, encounterId);
      if (!claim) {
        res.status(404).send(`Encounter '${encounterId}' not found`);
        return;
      }
      res
        .status(200)
        .json({ id: claim.id, status: claim.status, codeSetType: claim.codeSetType });
    } catch (error: unknown) {
      // One claim per encounter (claim_encounter_id_unique, migrations/) —
      // a double-submit races two claims from the same encounter otherwise.
      if (error instanceof UniqueConstraintViolationException) {
        res
          .status(409)
          .send(`A claim already exists for encounter '${encounterId}'`);
        return;
      }
      throw error;
    }
  }
);

export const scrubClaim = handlers.post(
  schemaValidator,
  '/:id/scrub',
  {
    name: 'Scrub Claim',
    access: 'protected',
    summary:
      'Runs a claim through the NCCI PTP / NCCI MUE / LCD-NCD scrubbing engine',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      sessionSchema: {
        organizationId: string
      },
      allowedPermissions: MANAGE_CLAIMS_PERMISSIONS
    },
    params: {
      id: string
    },
    responses: {
      200: {
        status: string,
        denials: schemaValidator.array({
          carcCode: string,
          category: string
        })
      },
      404: string
    }
  },
  async (req, res) => {
    const { id } = req.params;
    const organizationId = req.session?.organizationId;
    openTelemetryCollector.debug('Scrubbing claim', { id });
    const result = await serviceFactory({
      context: { tenantId: organizationId }
    }).scrubClaim(organizationId, id);
    if (!result) {
      res.status(404).send(`Claim '${id}' not found`);
      return;
    }
    res.status(200).json({
      status: result.status,
      denials: result.denials.map((denial) => ({
        carcCode: denial.carcCode,
        category: denial.category
      }))
    });
  }
);
