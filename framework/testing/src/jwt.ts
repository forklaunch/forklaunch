import { createServer, Server } from 'node:http';
import { AddressInfo } from 'node:net';
import { exportJWK, generateKeyPair, JWK, SignJWT } from 'jose';

/**
 * Claims a test session token carries. They mirror the RS256 session tokens
 * the Rust IAM issues: `sub`, `tenant_id`, `roles`, `permissions`, plus any
 * extra claims a route's `sessionSchema` or `decodeResource` reads.
 */
export interface TestJwtClaims {
  /** The user the token is for. */
  sub: string;
  /** Tenant (organization) the session is bound to. */
  tenantId?: string;
  roles?: readonly string[];
  permissions?: readonly string[];
  /** Lifetime in seconds (default 300). Negative values mint an expired token. */
  expiresIn?: number;
  /** Additional claims, merged last. */
  [claim: string]: unknown;
}

export interface TestJwtIssuerOptions {
  /** `kid` of the signing key (default `test-key`). */
  keyId?: string;
  /** `iss` claim (default `forklaunch-test`). */
  issuer?: string;
  /** `aud` claim (default `forklaunch-test`). */
  audience?: string;
  /** Path the JWKS is served at (default `/api/auth/jwks`, as IAM serves it). */
  jwksPath?: string;
}

/**
 * A local RS256 issuer for tests: it serves its public JWK set over HTTP, so
 * routes configured with `jwksPublicKeyUrl` verify its tokens exactly as they
 * verify a real IAM's, and mints tokens for any claims.
 */
export interface TestJwtIssuer {
  /** URL to set as `JWKS_PUBLIC_KEY_URL`. */
  readonly jwksUrl: string;
  /** The public JWK set served at `jwksUrl`. */
  readonly jwks: { keys: JWK[] };
  /** Mints a signed RS256 token. */
  issue(claims: TestJwtClaims): Promise<string>;
  /** Mints a token and returns it as an `Authorization` header value. */
  bearer(claims: TestJwtClaims): Promise<`Bearer ${string}`>;
  /** Stops the JWKS server. */
  close(): Promise<void>;
}

/**
 * Starts a JWKS server on an ephemeral localhost port and returns an issuer
 * whose tokens it verifies.
 *
 * @example
 * ```typescript
 * const issuer = await startTestJwtIssuer();
 * process.env.JWKS_PUBLIC_KEY_URL = issuer.jwksUrl;
 * const authorization = await issuer.bearer({
 *   sub: userId,
 *   permissions: ['platform:read']
 * });
 * ```
 */
export async function startTestJwtIssuer(
  options: TestJwtIssuerOptions = {}
): Promise<TestJwtIssuer> {
  const {
    keyId = 'test-key',
    issuer = 'forklaunch-test',
    audience = 'forklaunch-test',
    jwksPath = '/api/auth/jwks'
  } = options;

  const { publicKey, privateKey } = await generateKeyPair('RS256', {
    extractable: true
  });
  const publicJwk = await exportJWK(publicKey);
  const jwks = {
    keys: [{ ...publicJwk, kid: keyId, use: 'sig', alg: 'RS256' }]
  };
  const body = JSON.stringify(jwks);

  const server: Server = createServer((req, res) => {
    if (req.method === 'GET' && req.url?.split('?')[0] === jwksPath) {
      res.writeHead(200, {
        'content-type': 'application/json',
        'cache-control': 'max-age=300'
      });
      res.end(body);
      return;
    }
    res.writeHead(404);
    res.end();
  });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve());
  });
  // Keep-alive sockets from fetch must not hold the test process open.
  server.unref();
  const { port } = server.address() as AddressInfo;

  const issue = async ({
    sub,
    tenantId,
    roles,
    permissions,
    expiresIn = 300,
    ...extra
  }: TestJwtClaims): Promise<string> => {
    const now = Math.floor(Date.now() / 1000);
    return new SignJWT({
      ...(tenantId !== undefined ? { tenant_id: tenantId } : {}),
      ...(roles !== undefined ? { roles: [...roles] } : {}),
      ...(permissions !== undefined ? { permissions: [...permissions] } : {}),
      ...extra
    })
      .setProtectedHeader({ alg: 'RS256', kid: keyId, typ: 'JWT' })
      .setSubject(sub)
      .setIssuer(issuer)
      .setAudience(audience)
      .setIssuedAt(now)
      .setExpirationTime(now + expiresIn)
      .setJti(crypto.randomUUID())
      .sign(privateKey);
  };

  return {
    jwksUrl: `http://127.0.0.1:${port}${jwksPath}`,
    jwks,
    issue,
    bearer: async (claims): Promise<`Bearer ${string}`> =>
      `Bearer ${await issue(claims)}`,
    close: () =>
      new Promise<void>((resolve) => {
        server.closeAllConnections?.();
        server.close(() => resolve());
      })
  };
}

/**
 * Auth surfacing functions that read roles and permissions from the token
 * itself (the `roles` / `permissions` claims {@link TestJwtIssuer} mints), for
 * mounting routers in tests without an IAM to surface them from.
 */
export const surfaceFromTestClaims = {
  surfaceRoles: async (payload: {
    roles?: unknown;
    [claim: string]: unknown;
  }) =>
    new Set(
      Array.isArray(payload.roles)
        ? payload.roles.filter((r): r is string => typeof r === 'string')
        : []
    ),
  surfacePermissions: async (payload: {
    permissions?: unknown;
    [claim: string]: unknown;
  }) =>
    new Set(
      Array.isArray(payload.permissions)
        ? payload.permissions.filter((p): p is string => typeof p === 'string')
        : []
    )
};
