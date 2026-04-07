import { safeStringify } from '@forklaunch/common';
import { AnySchemaValidator } from '@forklaunch/validator';
import { JWK, JWTPayload, jwtVerify } from 'jose';
import { ParsedQs } from 'qs';
import { createHmacToken } from './createHmacToken';
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
          secretKey
        }: {
          method: string;
          path: string;
          body?: unknown;
          timestamp: Date;
          nonce: string;
          signature: string;
          secretKey: string;
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
    if ('signatureKey' in jwt) {
      verificationFunction = async (token) => {
        const { payload } = await jwtVerify(
          token,
          new Uint8Array(Buffer.from(jwt.signatureKey))
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
        for (const key of jwks) {
          try {
            const { payload } = await jwtVerify(token, key);
            return payload;
          } catch {
            cachedJwks.value = null;
            cachedJwks.lastUpdated = null;
            cachedJwks.ttl = DEFAULT_TTL;
            continue;
          }
        }
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
    authMethod = {
      type: 'hmac' as const,
      auth: {
        secretKeys: auth.hmac.secretKeys,
        verificationFunction: async ({
          method,
          path,
          body,
          timestamp,
          nonce,
          signature,
          secretKey
        }: {
          method: string;
          path: string;
          body?: unknown;
          timestamp: Date;
          nonce: string;
          signature: string;
          secretKey: string;
        }) => {
          const computedSignature = createHmacToken({
            method,
            path,
            body,
            timestamp,
            nonce,
            secretKey
          });

          const isValid = computedSignature === signature;

          if (!isValid) {
            const errorInfo = {
              method,
              path,
              timestamp: timestamp.toISOString(),
              nonce,
              receivedSignature: signature,
              computedSignature
            };

            openTelemetryCollector?.debug('[HMAC Verification Failed]', {
              ...errorInfo,
              bodyType: body ? typeof body : 'undefined',
              body: body ? safeStringify(body) : 'undefined'
            });
          }

          return isValid;
        }
      }
    };
  }
  if (authMethod == null) {
    throw new Error('Invalid auth method');
  }

  return authMethod;
}
