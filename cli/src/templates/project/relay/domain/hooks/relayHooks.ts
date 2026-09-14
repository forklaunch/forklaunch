/**
 * File: relayHooks.ts
 *
 * ============================================================================
 *  THE ONE APP-SPECIFIC HOOK. This is the only file you must edit.
 * ============================================================================
 *
 * The generic relay core (relaySession.service.ts) has already:
 *   - verified the platform relay's HMAC signature,
 *   - replay-guarded the nonce (single-use), and
 *   - is ready to mint the better-auth session cookie + 302 the browser.
 *
 * What it CANNOT know is app-specific:
 *   1. WHERE the relay-exchanged OAuth tokens should live (your token store).
 *   2. WHICH user the resulting session should sign in, and their active org.
 *   3. WHERE the browser should land afterwards.
 *
 * Fill those in below. Until you do, ingest succeeds and the browser is sent to
 * `/` with no session (safe default), because no owner is resolved.
 *
 * Reference implementation to model this on (Health Vault, the app this module
 * was generalized from): the hook forwarded the tokens to a separate vault
 * service over the internal mesh (an HMAC-signed POST), and resolved the owner
 * from the instance's claim ceremony (the user who claimed the instance) plus
 * their organization membership. A simpler app can store the tokens inline on
 * an entity right here using the `em` handed in, and resolve the owner from
 * whatever "who owns this instance" means in your domain.
 */

import type { EntityManager } from '@mikro-orm/core';
import type { RelayTokens } from '../schemas/relay.schema';

export interface RelaySessionEstablishment {
  /** Root-relative path the browser lands on after the cookie is set. */
  redirectPath: string;
  /**
   * The user the minted session signs in. Leave null/undefined to complete the
   * handoff WITHOUT a session (browser is redirected but not signed in) - the
   * safe default while this hook is still a stub.
   */
  ownerUserId?: string | null;
  /** Active organization stamped onto the session (drives role surfacing). */
  activeOrganizationId?: string | null;
}

/**
 * Store the relay-exchanged tokens and decide who to sign in.
 *
 * Runs exactly once per nonce (the replay guard has already claimed it before
 * this is called), so it is safe to perform writes here.
 */
export async function establishSessionFromRelayTokens(
  tokens: RelayTokens,
  em: EntityManager
): Promise<RelaySessionEstablishment> {
  // TODO(app): persist `tokens` in your OAuth/token store using `em`, then
  // resolve the owner user + active organization to sign in. See the file
  // header for the reference implementation this was generalized from.
  void tokens;
  void em;

  return { redirectPath: '/' };
}
