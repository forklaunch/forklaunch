/**
 * File: appClaimHooks.ts
 *
 * ============================================================================
 *  THE ONE APP-SPECIFIC HOOK for the second handover. Edit this file.
 * ============================================================================
 *
 * Handing a customer a managed app happens twice. The PLATFORM's claim gives
 * them the infrastructure — they set a passphrase and the instance becomes
 * theirs. YOUR claim gives them the product: an admin account, a verified
 * phone, a first user — whatever "you own this" means inside your app.
 *
 * The platform cannot perform the second one, because only your app knows what
 * its ceremony is. So it asks: the moment its own claim commits, it calls the
 * endpoint scaffolded beside this file, and expects back a URL to send the
 * customer to. One message to the customer instead of two, and nobody has to
 * pull a signing key out of the instance's configuration to mint a link by
 * hand.
 *
 * The generic half is already done for you in claim-mint.controller.ts:
 *   - the platform's HMAC signature is verified against the per-instance key,
 *   - the signature's stated purpose is checked, so a signature made for a
 *     different call cannot be replayed here.
 *
 * What it CANNOT know is app-specific: what a claim token is in your app,
 * where the customer opens it, and when it expires. Fill that in below.
 *
 * Until you do, the endpoint answers 501 and the platform records that the
 * product declined to mint — which leaves you minting by hand, exactly where
 * you were before installing this module. Nothing breaks; the customer's
 * infrastructure claim has already succeeded by the time this runs.
 */

import type { EntityManager } from '@mikro-orm/core';

export interface AppClaimLink {
  /**
   * Where the customer finishes taking ownership. Absolute, and reachable by
   * them — usually your frontend, not the instance's API host.
   */
  claimUrl: string;
  /** When the link stops working. Optional; omit for a link that does not expire. */
  expiresAt?: Date;
}

/**
 * Mint your product's claim link, or return null to decline.
 *
 * Decline when the ceremony cannot or should not happen — most importantly
 * when the product has ALREADY been claimed. A second live link into an
 * already-owned product is a way into someone else's data, so returning null
 * there is the safe answer, not an error.
 *
 * @param instanceId The platform's id for this instance, for your own records.
 * @param em A request-scoped entity manager; whatever you persist is flushed
 *           by the controller.
 */
export async function mintAppClaimLink(
  _instanceId: string,
  _em: EntityManager
): Promise<AppClaimLink | null> {
  // ---------------------------------------------------------------------
  // TODO: replace this with your ceremony.
  //
  // A worked shape, from the app this module was generalized from:
  //
  //   1. Refuse if already claimed:
  //        if (await claimService.isClaimed()) return null;
  //   2. Mint a single-use, expiring token and store its HASH (never the
  //      token itself — a database leak must not hand over live links):
  //        const token = randomBytes(32).toString('base64url');
  //        await store.put({ hash: sha256(token), expiresAt });
  //   3. Return where the customer opens it. Note this is the FRONTEND host,
  //      not this service's:
  //        return { claimUrl: `${FRONTEND_URL}/claim/${token}`, expiresAt };
  // ---------------------------------------------------------------------
  return null;
}
