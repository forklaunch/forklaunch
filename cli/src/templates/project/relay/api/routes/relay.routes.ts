/**
 * File: relay.routes.ts
 *
 * Root-basePath router for this app's managed-apps lifecycle endpoints — the
 * relay's session ingest, and the claim mint the platform calls at handover —
 * so the HMAC-verified `req.path` is the full path the platform signs. The browser-facing `/relay/handoff` redirect lives as a raw
 * route in server.ts (it must Set-Cookie + 302, which a typed handler does not).
 */

import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { mintClaimToken } from '../controllers/claim-mint.controller';
import { sessionIngest } from '../controllers/relay.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const relayRouter = forklaunchRouter(
  '/',
  schemaValidator,
  openTelemetryCollector
);

export const sessionIngestRoute = relayRouter.post(
  '/relay/session-ingest',
  sessionIngest
);

/**
 * The second handover: the platform asks this product to mint its own claim
 * link once the platform's claim commits. Same root basePath, so the
 * HMAC-verified `req.path` is the full path the platform signs.
 */
export const mintClaimTokenRoute = relayRouter.post(
  '/claim/mint',
  mintClaimToken
);
