/**
 * File: relay.schema.ts
 *
 * Wire contract for the managed-apps OAuth relay session-ingest endpoint. The
 * `tokens` shape is intentionally permissive: the platform relay forwards
 * whatever the upstream identity provider returned, and unknown extra fields
 * are dropped by validation without affecting the HMAC signature (which is
 * verified over the raw body before validation runs).
 *
 * The `tokens` fields below are the common OAuth/OIDC set. Adjust them to match
 * the identity provider your app integrates with; the generic relay core does
 * not care what is inside `tokens`, it only hands the object to your
 * `establishSessionFromRelayTokens` hook.
 */

import { number, optional, string } from '@forklaunch/blueprint-core';

export const SessionIngestSchema = {
  // the single-use nonce the instance minted into its OAuth state
  nonce: string,
  tokens: {
    access_token: optional(string),
    token_type: optional(string),
    expires_in: optional(number),
    scope: optional(string),
    id_token: optional(string),
    refresh_token: optional(string)
  }
};

export const SessionIngestResponseSchema = {
  redirectPath: string
};

/**
 * TypeScript view of the relay-exchanged tokens handed to the app hook. Kept in
 * sync with `SessionIngestSchema.tokens` above; widen it as your provider
 * returns more fields.
 */
export type RelayTokens = {
  access_token?: string;
  token_type?: string;
  expires_in?: number;
  scope?: string;
  id_token?: string;
  refresh_token?: string;
  [key: string]: unknown;
};
