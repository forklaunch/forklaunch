/**
 * File: claim-mint.controller.ts
 *
 * The instance-side endpoint for the SECOND handover.
 *
 * The platform's claim hands a customer the infrastructure. This hands them
 * the product. The platform cannot run your ceremony — only your app knows
 * what owning it means — so it calls here the moment its own claim commits
 * and expects a URL back. The app-specific half is `mintAppClaimLink` in
 * domain/hooks/appClaimHooks.ts; everything generic is below.
 *
 * Two things this verifies before your hook runs:
 *
 *   1. The platform's HMAC signature, against THIS instance's per-instance key
 *      — the same key the platform verifies when this instance calls IT. The
 *      keyId is `platform`, so the key is registered beside your own internal
 *      key rather than replacing it: an operator can still mint by hand.
 *   2. The signature's stated PURPOSE. One secret now signs calls in both
 *      directions, so a signature says what it was made for, and this endpoint
 *      refuses any other purpose. Today the two directions differ by path,
 *      which makes confusion impossible by accident rather than by design.
 *
 * Root-basePath router (see relay.routes.ts) so the HMAC-verified `req.path`
 * is the full path the platform signs.
 */

import {
  handlers,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { mintAppClaimLink } from '../../domain/hooks/appClaimHooks';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const INSTANCE_HMAC_KEY = ci.resolve(tokens.INSTANCE_HMAC_KEY);
const emFactory = ci.scopedResolver(tokens.EntityManager);

/** What a platform-signed mint declares itself to be for. */
const APP_CLAIM_MINT_PURPOSE = 'app-claim-mint';

export const mintClaimToken = handlers.post(
  schemaValidator,
  '/claim/mint',
  {
    name: 'Mint Claim Token',
    access: 'internal',
    summary:
      "Mint this product's own claim link for a freshly claimed instance",
    auth: {
      hmac: {
        secretKeys: {
          // An operator minting by hand with the app's internal key.
          default: HMAC_SECRET_KEY,
          // The platform, minting the moment its own claim commits.
          ...(INSTANCE_HMAC_KEY ? { platform: INSTANCE_HMAC_KEY } : {})
        }
      }
    },
    body: {
      purpose: optional(string),
      instanceId: optional(string)
    },
    responses: {
      200: { claimUrl: string, expiresAt: optional(string) },
      400: string,
      // The hook is still a stub. Said plainly so the platform records a
      // reason rather than a failure, and you keep minting by hand.
      501: string,
      // Declined — most often because the product is already claimed.
      409: string
    }
  },
  async (req, res) => {
    const purpose = req.body?.purpose;
    if (purpose !== undefined && purpose !== APP_CLAIM_MINT_PURPOSE) {
      openTelemetryCollector.warn(
        `Refusing claim mint signed for '${purpose}'`
      );
      res.status(400).send('This signature was not made for minting a claim');
      return;
    }

    const em = emFactory();
    const link = await mintAppClaimLink(req.body?.instanceId ?? '', em);
    if (!link) {
      res
        .status(409)
        .send(
          'No claim link minted. Either this product is already claimed, or mintAppClaimLink is still a stub (domain/hooks/appClaimHooks.ts).'
        );
      return;
    }
    await em.flush();

    openTelemetryCollector.info('Minted app claim link');
    res.status(200).json({
      claimUrl: link.claimUrl,
      ...(link.expiresAt ? { expiresAt: link.expiresAt.toISOString() } : {})
    });
  }
);
