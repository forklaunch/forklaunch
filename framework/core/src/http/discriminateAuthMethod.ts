import { AnySchemaValidator } from '@forklaunch/validator';
import { JWK, JWTPayload, jwtVerify } from 'jose';
import { ParsedQs } from 'qs';
import { createHmacToken } from './createHmacToken';
import {
  hmacNonceKey,
  hmacNonceTtlMs,
  isHmacTimestampFresh,
  resolveHmacMaxSkewMs,
  resolveHmacNonceStore,
  timingSafeEqualStrings
} from './hmacReplayProtection';
import {
  jwtAlgorithmsForPublicKey,
  jwtAlgorithmsForSharedSecret,
  jwtClaimChecks
} from './jwtAlgorithms';
import { isBasicAuthMethod } from './guards/isBasicAuthMethod';
import { isHmacMethod } from './guards/isHmacMethod';
import { isJwtAuthMethod } from './guards/isJwtAuthMethod';
import { OpenTelemetryCollector } from './telemetry/openTelemetryCollector';
import { VersionedRequests } from './types/apiDefinition.types';
import {
  AuthMethods,
  BasicAuthMethods,
  DecodeResource,
  ParamsDictionary
} from './types/contractDetails.types';
import { MetricsDefinition } from './types/openTelemetryCollector.types';

const DEFAULT_TTL = process.env.JWKS_TTL
  ? parseInt(process.env.JWKS_TTL)
  : 60 * 1000 * 5;

const cachedJwks = {
  value: null as JWK[] | null,
  lastUpdated: null as Date | null,
  ttl: DEFAULT_TTL
};

/**
 * Retrieves and caches the JSON Web Key Set (JWKS) from a given public key URL.
 *
 * This function fetches the JWKS from the specified URL and caches the result in memory
 * to avoid unnecessary network requests. The cache is considered valid for a duration
 * specified by the `cache-control` header in the JWKS HTTP response (in seconds), or
 * falls back to a default TTL if the header is not present. If the cache is still valid,
 * the cached keys are returned immediately.
 *
 * @param {string} jwksPublicKeyUrl - The URL to fetch the JWKS from.
 * @returns {Promise<JWK[]>} A promise that resolves to an array of JWK objects.
 *
 * @example
 * const jwks = await getCachedJwks('https://example.com/.well-known/jwks.json');
 * // Use jwks for JWT verification, etc.
 */
export async function getCachedJwks(jwksPublicKeyUrl: string): Promise<JWK[]> {
  if (
    cachedJwks.value &&
    cachedJwks.lastUpdated &&
    Date.now() - cachedJwks.lastUpdated.getTime() < cachedJwks.ttl
  ) {
    return cachedJwks.value;
  } else {
    const jwksResponse = await fetch(jwksPublicKeyUrl);
    const text = await jwksResponse.text();
    if (!text) {
      if (cachedJwks.value) {
        return cachedJwks.value;
      }
      throw new Error(
        `JWKS endpoint returned empty response (status ${jwksResponse.status})`
      );
    }
    const jwks = (JSON.parse(text) as { keys: JWK[] }).keys;
    cachedJwks.value = jwks;
    cachedJwks.lastUpdated = new Date();
    cachedJwks.ttl =
      parseInt(
        jwksResponse.headers.get('cache-control')?.split('=')[1] ??
          `${DEFAULT_TTL / 1000}`
      ) * 1000;
    return jwks;
  }
}

/**
 * Discriminates between different authentication methods and returns a typed result.
 *
 * This function analyzes the provided authentication configuration and determines
 * whether it uses basic authentication or JWT authentication. It returns a
 * discriminated union type that includes the authentication type and the
 * corresponding authentication configuration.
 *
 * @template SV - A type that extends AnySchemaValidator
 * @template P - A type that extends ParamsDictionary
 * @template ReqBody - A type that extends Record<string, unknown>
 * @template ReqQuery - A type that extends ParsedQs
 * @template ReqHeaders - A type that extends Record<string, unknown>
 * @template BaseRequest - The base request type
 *
 * @param auth - The authentication methods configuration object
 * @returns A discriminated union object with either:
 *   - `{ type: 'basic', auth: BasicAuthMethods['basic'] }` for basic authentication
 *   - `{ type: 'jwt', auth?: JwtAuthMethods['jwt'] }` for JWT authentication (auth is optional)
 *
 * @example
 * ```typescript
 * const authConfig = {
 *   basic: {
 *     login: (username: string, password: string) => boolean
 *   }
 * };
 * const result = discriminateAuthMethod(authConfig);
 * // result.type === 'basic'
 * // result.auth === authConfig.basic
 * ```
 */
export async function discriminateAuthMethod<
  SV extends AnySchemaValidator,
  P extends ParamsDictionary,
  ReqBody extends Record<string, unknown>,
  ReqQuery extends ParsedQs,
  ReqHeaders extends Record<string, string>,
  VersionedReqs extends VersionedRequests,
  BaseRequest
>(
  auth: AuthMethods<
    SV,
    P,
    ReqBody,
    ReqQuery,
    ReqHeaders,
    VersionedReqs,
    BaseRequest
  >,
  openTelemetryCollector?: OpenTelemetryCollector<MetricsDefinition>
): Promise<
  | {
      type: 'basic';
      auth: {
        decodeResource?: DecodeResource;
        login: BasicAuthMethods['basic']['login'];
      };
    }
  | {
      type: 'jwt';
      auth:
        | {
            decodeResource?: DecodeResource;
          }
        | {
            verificationFunction: (
              token: string
            ) => Promise<JWTPayload | undefined>;
          };
    }
  | {
      type: 'hmac';
      auth: {
        secretKeys: Record<string, string>;
        verificationFunction: ({
          body,
          path,
          method,
          timestamp,
          nonce,
          signature,
          secretKey,
          keyId
        }: {
          method: string;
          path: string;
          body?: unknown;
          timestamp: Date;
          nonce: string;
          signature: string;
          secretKey: string;
          keyId?: string;
        }) => Promise<boolean | undefined>;
      };
    }
> {
  let authMethod;
  if (isBasicAuthMethod(auth)) {
    authMethod = {
      type: 'basic' as const,
      auth: {
        decodeResource: auth.decodeResource,
        login: auth.basic.login
      }
    };
  } else if (isJwtAuthMethod(auth)) {
    const jwt = auth.jwt;
    let verificationFunction: (
      token: string
    ) => Promise<JWTPayload | undefined>;
    // Issuer and audience are checked only when the app configured them.
    // Resolved per verification so a bad setting rejects the token (and is
    // logged with the failure) instead of crashing the request.
    const claimChecks = () => jwtClaimChecks(jwt);
    if ('signatureKey' in jwt) {
      verificationFunction = async (token) => {
        // The algorithm list is always pinned: HS256 unless configured.
        const { payload } = await jwtVerify(
          token,
          new Uint8Array(Buffer.from(jwt.signatureKey)),
          {
            ...claimChecks(),
            algorithms: jwtAlgorithmsForSharedSecret(jwt.algorithms)
          }
        );
        return payload;
      };
    } else {
      let jwks: JWK[];
      if ('jwksPublicKeyUrl' in jwt) {
        jwks = await getCachedJwks(jwt.jwksPublicKeyUrl);
      } else if ('jwksPublicKey' in jwt) {
        jwks = [jwt.jwksPublicKey];
      }
      verificationFunction = async (token) => {
        // Keep the last failure so the middleware can log WHY (expired vs
        // bad signature) instead of a bare "invalid token". The caller's
        // catch maps any throw to the same 403 as an undefined return.
        let lastError: unknown;
        for (const key of jwks) {
          try {
            // Pinned per key to asymmetric algorithms only, so a token
            // cannot pick `none` or an HMAC algorithm keyed on public
            // material (algorithm confusion).
            const { payload } = await jwtVerify(token, key, {
              ...claimChecks(),
              algorithms: jwtAlgorithmsForPublicKey(key, jwt.algorithms)
            });
            return payload;
          } catch (error) {
            lastError = error;
            cachedJwks.value = null;
            cachedJwks.lastUpdated = null;
            cachedJwks.ttl = DEFAULT_TTL;
            continue;
          }
        }
        if (lastError !== undefined) throw lastError;
      };
    }
    authMethod = {
      type: 'jwt' as const,
      auth: {
        decodeResource: auth.decodeResource,
        verificationFunction
      }
    };
  } else if (isHmacMethod(auth)) {
    const maxSkewMs = resolveHmacMaxSkewMs(auth.hmac.maxSkewMs);
    const configuredNonceStore = auth.hmac.nonceStore;
    authMethod = {
      type: 'hmac' as const,
      auth: {
        secretKeys: auth.hmac.secretKeys,
        /**
         * Accepts a signed request only when its timestamp is inside the
         * skew window, its signature matches (constant-time compare) and
         * its nonce has not been used before. The nonce is claimed last, and
         * only for a valid signature, so a `false` for one candidate path
         * never burns the nonce for the next.
         */
        verificationFunction: async ({
          method,
          path,
          body,
          timestamp,
          nonce,
          signature,
          secretKey,
          keyId
        }: {
          method: string;
          path: string;
          body?: unknown;
          timestamp: Date;
          nonce: string;
          signature: string;
          secretKey: string;
          /** Scopes the nonce to the signing key. */
          keyId?: string;
        }) => {
          const reject = (reason: string) => {
            // Never log the expected signature or the body: the first is a
            // valid credential for this exact request, the second may hold
            // protected data.
            openTelemetryCollector?.debug('[HMAC Verification Failed]', {
              reason,
              method,
              path,
              keyId,
              timestamp:
                timestamp instanceof Date && !Number.isNaN(timestamp.getTime())
                  ? timestamp.toISOString()
                  : 'invalid',
              bodyType: body ? typeof body : 'undefined'
            });
            return false;
          };

          if (typeof secretKey !== 'string' || secretKey.length === 0) {
            return reject('unknown key');
          }
          if (
            typeof nonce !== 'string' ||
            nonce.length === 0 ||
            typeof signature !== 'string'
          ) {
            return reject('missing nonce or signature');
          }

          const now = Date.now();
          if (!isHmacTimestampFresh(timestamp, now, maxSkewMs)) {
            return reject('stale or invalid timestamp');
          }

          const computedSignature = createHmacToken({
            method,
            path,
            body,
            timestamp,
            nonce,
            secretKey
          });
          if (!timingSafeEqualStrings(computedSignature, signature)) {
            return reject('signature mismatch');
          }

          let firstUse: boolean;
          try {
            firstUse = await resolveHmacNonceStore(configuredNonceStore).claim(
              hmacNonceKey(keyId, nonce),
              hmacNonceTtlMs(timestamp, now, maxSkewMs)
            );
          } catch (error) {
            // A nonce store that cannot answer fails closed.
            openTelemetryCollector?.error(
              '[HMAC Nonce Store Failed]',
              error instanceof Error ? error.message : String(error)
            );
            return reject('nonce store unavailable');
          }
          if (firstUse !== true) {
            return reject('replayed nonce');
          }

          return true;
        }
      }
    };
  }
  if (authMethod == null) {
    throw new Error('Invalid auth method');
  }

  return authMethod;
}
