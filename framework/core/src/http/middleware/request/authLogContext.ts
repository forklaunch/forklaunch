import { createHash } from 'crypto';
import { decodeJwt, decodeProtectedHeader, errors as joseErrors } from 'jose';

/**
 * What an authorization failure log line may carry about the credential.
 *
 * Never the credential itself. A bearer token is usable until it expires
 * and a Basic header is a password, so neither belongs in a log stream that
 * outlives the request. What support actually needs is why it failed, who
 * it claimed to be, and a way to tell two failures from one caller apart.
 */
export type AuthLogContext = {
  /** Whether an Authorization value was present at all. */
  hasToken: boolean;
  /**
   * First 8 hex chars of sha256(header value). Correlates retries of one
   * token and lets a caller match a token they hold against a log line
   * without the log holding the token. Not reversible.
   */
  tokenFingerprint?: string;
  /**
   * Decoded, NOT verified, JWT claims. On a failed verification these are
   * whatever the caller wrote, so they are labelled claimed rather than
   * user and must never be treated as identity. `email` is left out on
   * purpose: `sub` identifies the account without putting PII in logs.
   */
  claimed?: {
    sub?: string;
    organizationId?: string;
    iss?: string;
    exp?: number;
    kid?: string;
  };
};

const FINGERPRINT_LENGTH = 8;
const MAX_CLAIM_LENGTH = 128;

function bounded(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0
    ? value.slice(0, MAX_CLAIM_LENGTH)
    : undefined;
}

/**
 * Builds the loggable context for an Authorization header value. Accepts
 * the full header value (`Bearer eyJ…`, `Basic …`, `HMAC …`) or a bare
 * token; JWT claims are only attempted when the value looks like a JWT.
 */
export function authLogContext(headerValue: unknown): AuthLogContext {
  if (typeof headerValue !== 'string' || headerValue.length === 0) {
    return { hasToken: false };
  }

  const context: AuthLogContext = {
    hasToken: true,
    tokenFingerprint: createHash('sha256')
      .update(headerValue)
      .digest('hex')
      .slice(0, FINGERPRINT_LENGTH)
  };

  const parts = headerValue.trim().split(/\s+/);
  const candidate = parts[parts.length - 1];
  if (candidate.split('.').length !== 3) {
    return context;
  }

  try {
    const payload = decodeJwt(candidate);
    const header = decodeProtectedHeader(candidate);
    context.claimed = {
      sub: bounded(payload.sub),
      organizationId: bounded(payload.organizationId),
      iss: bounded(payload.iss),
      exp: typeof payload.exp === 'number' ? payload.exp : undefined,
      kid: bounded(header.kid)
    };
  } catch {
    // Not a decodable JWT; the fingerprint is all we can offer.
  }

  return context;
}

/**
 * A stable, short reason for a JWT verification failure, derived from the
 * jose error class. This is the field that was missing from the logs: an
 * expired token and a forged one used to produce identical lines.
 */
export function jwtFailureReason(error: unknown): string {
  if (error instanceof joseErrors.JWTExpired) return 'jwt_expired';
  if (error instanceof joseErrors.JWTClaimValidationFailed) {
    return `jwt_claim_${error.claim}`;
  }
  if (error instanceof joseErrors.JWSSignatureVerificationFailed) {
    return 'jwt_bad_signature';
  }
  if (error instanceof joseErrors.JWKSNoMatchingKey) return 'jwks_no_key';
  if (error instanceof joseErrors.JWKSTimeout) return 'jwks_unavailable';
  if (
    error instanceof joseErrors.JWTInvalid ||
    error instanceof joseErrors.JWSInvalid
  ) {
    return 'jwt_malformed';
  }
  if (error instanceof joseErrors.JOSEError) return error.code.toLowerCase();
  return 'unknown';
}
