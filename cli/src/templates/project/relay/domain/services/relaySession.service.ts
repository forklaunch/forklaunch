/**
 * File: relaySession.service.ts
 *
 * The instance half of the managed-apps OAuth relay. The platform relay runs
 * OAuth on every instance's behalf and forwards the finished session to
 * `/relay/session-ingest`; this service does the three things the instance owes
 * back:
 *
 *  1. Replay guard. The relay's `nonce` is honoured exactly once. The guard is
 *     the unique `nonce` column on RelaySessionHandoff: the insert itself is the
 *     single-use test, so two callbacks carrying the same nonce can never both
 *     proceed.
 *  2. App handoff. The tokens are passed to `establishSessionFromRelayTokens`
 *     (the one app-specific hook), which stores them and resolves which user to
 *     sign in. That runs only AFTER the nonce is claimed, so it can never run
 *     twice for one nonce.
 *  3. Session handoff. This owns the better-auth session, but a server-to-server
 *     ingest cannot set the browser's cookie. So it mints a one-time ticket and
 *     returns `/relay/handoff?ticket=<id>`; the relay 302s the browser there
 *     (first-party), and THAT request sets the session cookie. The ticket is
 *     single-use and short-lived.
 *
 * Everything here is generic. The only app-specific decisions live behind the
 * `establishSessionFromRelayTokens` hook.
 */

import { createHmac, randomBytes } from 'node:crypto';
import { UniqueConstraintViolationException } from '@mikro-orm/core';
import type { EntityManager } from '@mikro-orm/core';
import { RelaySessionHandoff } from '../../persistence/entities/relaySessionHandoff.entity';
import { Session } from '../../persistence/entities/session.entity';
import { establishSessionFromRelayTokens } from '../hooks/relayHooks';
import type { RelayTokens } from '../schemas/relay.schema';
import {
  DEFAULT_REDIRECT_PATH,
  sanitizeRedirectPath
} from '../utils/redirect-path.util';

/** How long a minted handoff ticket stays redeemable. */
const HANDOFF_TTL_MS = 5 * 60 * 1000;
/** How long a minted session lasts - matches better-auth's session.expiresIn. */
const SESSION_TTL_MS = 24 * 60 * 60 * 1000;

/** Raised when a nonce that was already consumed is presented again. */
export class RelayReplayError extends Error {
  constructor() {
    super('relay nonce already consumed');
    this.name = 'RelayReplayError';
  }
}

/** The cookie the handoff endpoint must set for the browser to be signed in. */
export interface SessionCookie {
  name: string;
  value: string;
  attributes: {
    secure?: boolean;
    httpOnly?: boolean;
    sameSite?: 'lax' | 'strict' | 'none' | boolean;
    path?: string;
    maxAge?: number;
  };
}

/** Reads better-auth's own cookie name + secret without depending on its shape. */
export interface BetterAuthCookieContext {
  secret: string;
  sessionTokenName: string;
  sessionTokenAttributes: SessionCookie['attributes'];
}

/** Minimal logger surface, so the service does not couple to the otel generic. */
export interface RelayLogger {
  info: (message: string, ...args: unknown[]) => void;
  error: (message: string, ...args: unknown[]) => void;
}

/**
 * Serialize a minted session cookie into a `Set-Cookie` header value. The
 * cookie value is already url-encoded and signed; this only appends the
 * attributes better-auth would have used (read from its own context).
 */
export function serializeSessionCookie(cookie: SessionCookie): string {
  const parts = [`${cookie.name}=${cookie.value}`];
  const a = cookie.attributes;
  parts.push(`Path=${a.path ?? '/'}`);
  if (a.maxAge !== undefined) {
    parts.push(`Max-Age=${a.maxAge}`);
  }
  if (a.httpOnly) {
    parts.push('HttpOnly');
  }
  if (a.secure) {
    parts.push('Secure');
  }
  if (a.sameSite) {
    const s = a.sameSite === true ? 'Strict' : a.sameSite;
    parts.push(`SameSite=${s.charAt(0).toUpperCase()}${s.slice(1)}`);
  }
  return parts.join('; ');
}

export class RelaySessionService {
  constructor(
    private readonly em: EntityManager,
    private readonly cookieContext: () => Promise<BetterAuthCookieContext>,
    private readonly logger: RelayLogger
  ) {}

  async ingest(params: {
    nonce: string;
    tokens: RelayTokens;
  }): Promise<{ redirectPath: string }> {
    this.logger.info('Completing relay-exchanged session');

    // The nonce insert IS the replay guard: a duplicate raises a unique
    // violation, which we surface as a replay rather than a 500. It is claimed
    // BEFORE the app hook runs, so the hook can never run twice for one nonce.
    const handoff = this.em.create(RelaySessionHandoff, {
      nonce: params.nonce,
      ownerUserId: null,
      activeOrganizationId: null,
      redirectTo: DEFAULT_REDIRECT_PATH,
      expiresAt: new Date(Date.now() + HANDOFF_TTL_MS),
      consumedAt: null
    });
    try {
      this.em.persist(handoff);
      await this.em.flush();
    } catch (error) {
      if (error instanceof UniqueConstraintViolationException) {
        throw new RelayReplayError();
      }
      throw error;
    }

    // Only after the nonce is claimed do we run the app hook (store tokens +
    // resolve the owner). Its result decides who gets signed in and where the
    // browser lands.
    const establishment = await establishSessionFromRelayTokens(
      params.tokens,
      this.em
    );

    handoff.ownerUserId = establishment.ownerUserId ?? null;
    handoff.activeOrganizationId = establishment.activeOrganizationId ?? null;
    handoff.redirectTo = sanitizeRedirectPath(establishment.redirectPath);
    this.em.persist(handoff);
    await this.em.flush();

    return { redirectPath: `/relay/handoff?ticket=${handoff.id}` };
  }

  async redeemTicket(
    ticketId: string
  ): Promise<{ cookie: SessionCookie | null; redirectTo: string } | null> {
    const handoff = await this.em.findOne(RelaySessionHandoff, { id: ticketId });
    if (
      !handoff ||
      handoff.consumedAt !== null ||
      handoff.expiresAt.getTime() <= Date.now()
    ) {
      return null;
    }

    handoff.consumedAt = new Date();
    this.em.persist(handoff);
    await this.em.flush();

    const redirectTo = sanitizeRedirectPath(handoff.redirectTo);
    if (!handoff.ownerUserId) {
      return { cookie: null, redirectTo };
    }

    const cookie = await this.mintSessionCookie(
      handoff.ownerUserId,
      handoff.activeOrganizationId ?? null
    );
    return { cookie, redirectTo };
  }

  private async mintSessionCookie(
    ownerUserId: string,
    activeOrganizationId: string | null
  ): Promise<SessionCookie> {
    const token = randomBytes(32).toString('base64url');
    const session = this.em.create(Session, {
      user: ownerUserId,
      token,
      expiresAt: new Date(Date.now() + SESSION_TTL_MS),
      ipAddress: null,
      userAgent: null,
      activeOrganizationId,
      activeTeamId: null
    });
    this.em.persist(session);
    await this.em.flush();

    // better-auth looks sessions up by their raw token, so a directly-inserted
    // row is honoured on the next request. The cookie is signed exactly the way
    // better-auth signs it: `encodeURIComponent(`${token}.${hmacSha256(token)}`)`.
    const { secret, sessionTokenName, sessionTokenAttributes } =
      await this.cookieContext();
    const signature = createHmac('sha256', secret)
      .update(token)
      .digest('base64');
    const value = encodeURIComponent(`${token}.${signature}`);

    return { name: sessionTokenName, value, attributes: sessionTokenAttributes };
  }
}
