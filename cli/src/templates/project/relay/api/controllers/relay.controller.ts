/**
 * File: relay.controller.ts
 *
 * The instance-side endpoint of the managed-apps OAuth relay. The platform
 * relay completes OAuth on this instance's behalf and forwards the finished
 * session here over the internal mesh, signed with THIS instance's per-instance
 * HMAC key (keyId = the instance id). We verify that signature, run the app
 * hook, and return a one-time, root-relative handoff path the relay can 302 the
 * browser to (where the session cookie is actually set - see the
 * `/relay/handoff` route in server.ts).
 *
 * Root-basePath router (see relay.routes.ts) so the HMAC-verified `req.path` is
 * the full `/relay/session-ingest` the relay signs (a `/relay` basePath would
 * strip to `/session-ingest` and fail verification).
 */

import { handlers, schemaValidator, string } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  SessionIngestResponseSchema,
  SessionIngestSchema
} from '../../domain/schemas/relay.schema';
import { RelayReplayError } from '../../domain/services/relaySession.service';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const INSTANCE_ID = ci.resolve(tokens.INSTANCE_ID);
const INSTANCE_HMAC_KEY = ci.resolve(tokens.INSTANCE_HMAC_KEY);
const serviceFactory = ci.scopedResolver(tokens.RelaySessionService);

export const sessionIngest = handlers.post(
  schemaValidator,
  '/relay/session-ingest',
  {
    name: 'Relay Session Ingest',
    access: 'internal',
    summary:
      'Accept a relay-exchanged OAuth session and return a one-time handoff path',
    auth: {
      hmac: {
        // The relay signs with keyId = this instance's id and the per-instance
        // HMAC key. Falls back to the shared internal key under `default` so a
        // self-hosted instance (no instance identity) still has a valid config.
        secretKeys: {
          [INSTANCE_ID || 'default']: INSTANCE_HMAC_KEY || HMAC_SECRET_KEY
        }
      }
    },
    body: SessionIngestSchema,
    responses: {
      200: SessionIngestResponseSchema,
      409: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.info('Relay session ingest received');
    try {
      const { redirectPath } = await serviceFactory().ingest({
        nonce: req.body.nonce,
        tokens: req.body.tokens
      });
      res.status(200).json({ redirectPath });
    } catch (error) {
      if (error instanceof RelayReplayError) {
        res.status(409).send('relay nonce already consumed');
        return;
      }
      throw error;
    }
  }
);
