/**
 * File: relaySessionHandoff.entity.ts
 *
 * Dual-purpose row for the managed-apps OAuth relay:
 *   1. Replay guard - `nonce` is unique, so the insert itself is the single-use
 *      test. Two callbacks carrying the same nonce can never both proceed.
 *   2. One-time handoff ticket - keyed by `id`, redeemed exactly once by the
 *      `/relay/handoff` GET, which sets the session cookie and 302s the browser.
 *
 * Every column is `compliance('none')`: this is short-lived, opaque routing
 * material. No OAuth tokens or PII are ever stored here.
 */

import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const RelaySessionHandoff = defineComplianceEntity({
  name: 'RelaySessionHandoff',
  properties: {
    ...sqlBaseProperties,
    // the relay nonce, honoured once - unique so the insert is the replay guard
    nonce: fp.string().unique().compliance('none'),
    // the user this handoff will sign in (null until the hook resolves an owner)
    ownerUserId: fp.string().nullable().compliance('none'),
    // the active organization stamped onto the minted session (role surfacing)
    activeOrganizationId: fp.string().nullable().compliance('none'),
    // where the browser lands after the cookie is set (always root-relative)
    redirectTo: fp.string().compliance('none'),
    // when the handoff ticket stops being redeemable
    expiresAt: fp.datetime().compliance('none'),
    // stamped the first time the ticket is redeemed; a redeemed ticket is dead
    consumedAt: fp.datetime().nullable().compliance('none')
  }
});

export type RelaySessionHandoff = InferEntity<typeof RelaySessionHandoff>;
