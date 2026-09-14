import {
  handlers,
  number,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.AnalyticsService);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

// Aggregate/analytics, not PHI-bearing — a looser permission than the
// per-claim/per-denial endpoints, per plan §3's "PHI-bearing read
// endpoints get stricter allowedPermissions than aggregate/analytics
// endpoints" framing.
const VIEW_ANALYTICS_PERMISSIONS = new Set(['admin:view_analytics']);

// Reports clean-claim-rate and denial-rate (§11) — the two success metrics
// this module can actually compute from data it owns. Average-days-to-
// payment is dropped entirely (needs real remittance timing, out of scope
// per §8/§14, not just deferred). See §14 PR 5.
export const getClaimAnalyticsSummary = handlers.get(
  schemaValidator,
  '/claims/summary',
  {
    name: 'Get Claim Analytics Summary',
    access: 'protected',
    summary:
      'Reports clean-claim-rate and denial-rate over scrubbed claims, optionally date-ranged',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      sessionSchema: {
        organizationId: string
      },
      allowedPermissions: VIEW_ANALYTICS_PERMISSIONS
    },
    query: {
      since: optional(string),
      until: optional(string)
    },
    responses: {
      200: {
        totalScrubbedClaims: number,
        cleanClaimRate: number,
        denialRate: number,
        denialsByCategory: schemaValidator.record(string, number)
      },
      400: string
    }
  },
  async (req, res) => {
    const { since, until } = req.query;
    const organizationId = req.session?.organizationId;
    openTelemetryCollector.debug('Computing claim analytics summary', {
      since,
      until
    });

    const sinceDate = since ? new Date(since) : undefined;
    const untilDate = until ? new Date(until) : undefined;
    if (
      (sinceDate && Number.isNaN(sinceDate.getTime())) ||
      (untilDate && Number.isNaN(untilDate.getTime()))
    ) {
      res.status(400).send('since/until must be valid ISO date strings');
      return;
    }

    const summary = await serviceFactory({ context: { tenantId: organizationId } }).getClaimSummary(organizationId, {
      since: sinceDate,
      until: untilDate
    });
    res.status(200).json({
      totalScrubbedClaims: summary.totalScrubbedClaims,
      cleanClaimRate: summary.cleanClaimRate,
      denialRate: summary.denialRate,
      denialsByCategory: summary.denialsByCategory
    });
  }
);
