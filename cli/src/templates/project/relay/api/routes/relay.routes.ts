/**
 * File: relay.routes.ts
 *
 * Root-basePath router for the managed-apps relay session-ingest endpoint, so
 * the HMAC-verified `req.path` is the full `/relay/session-ingest` the platform
 * relay signs. The browser-facing `/relay/handoff` redirect lives as a raw
 * route in server.ts (it must Set-Cookie + 302, which a typed handler does not).
 */

import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
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
