import type { AuthCacheService } from '@forklaunch/blueprint-core';
import { generateHmacAuthHeaders } from '@forklaunch/core/http';
import { universalSdk } from '@forklaunch/universal-sdk';

// Cross-service call to IAM for real coder/biller permissions/roles (§3
// "Cross-service calls to IAM"), replacing the surfacePermissions/
// surfaceRoles placeholders in server.ts — without this, the RBAC
// declarations added to claim/denial/analytics routes (§14 PR 5) can
// never actually pass for anyone.
//
// Adapted from iam-base/surfacing.ts's createSurfacePermissions/
// createSurfaceRoles, not imported from it: IamSdkClient's real type is
// local to iam-base's own package (derived from its own controller
// signatures via MapToSdk), not published for cac-base to import. This
// hand-written interface only needs to match the two route paths it
// actually calls — universalSdk resolves everything at runtime from
// IAM's live OpenAPI spec, so the type here is a compile-time convenience,
// not something IAM has to conform to structurally.
//
// Results are cached via AuthCacheService (§12 item 8, closed) — a 5min
// TTL, Redis-backed, same shape iam-base's own pattern uses — so repeat
// requests from the same user within the window skip the IAM round-trip
// entirely. A cache miss or lookup error falls through to the live IAM
// call unchanged; a cache write failure is swallowed by AuthCacheService
// itself (fail-open — caching is a perf optimization, never a correctness
// dependency).
interface IamSurfaceSdk {
  user: {
    surfacePermissions: (args: {
      params: { id: string };
      headers: Record<string, string>;
    }) => Promise<{
      code: number;
      response?: Array<{ slug: string }>;
    }>;
    surfaceRoles: (args: {
      params: { id: string };
      headers: Record<string, string>;
    }) => Promise<{
      code: number;
      response?: Array<{ name: string }>;
    }>;
  };
}

const sdkCache = new Map<string, IamSurfaceSdk>();

async function getIamSdk(iamUrl: string): Promise<IamSurfaceSdk> {
  let sdk = sdkCache.get(iamUrl);
  if (!sdk) {
    sdk = await universalSdk<IamSurfaceSdk>({
      host: iamUrl,
      registryOptions: { path: 'api/v1/openapi' }
    });
    sdkCache.set(iamUrl, sdk);
  }
  return sdk;
}

export function createSurfacePermissions(params: {
  authCacheService: AuthCacheService;
  iamUrl: string;
  hmacSecretKey: string;
}): (payload: { sub?: string }) => Promise<Set<string>> {
  const { authCacheService, iamUrl, hmacSecretKey } = params;

  return async (payload) => {
    if (!payload.sub) {
      return new Set<string>();
    }

    const cached = await authCacheService.getCachedPermissions(payload.sub);
    if (cached) {
      return cached;
    }

    try {
      const iamSdk = await getIamSdk(iamUrl);
      const headers = generateHmacAuthHeaders({
        secretKey: hmacSecretKey,
        method: 'GET',
        path: `/${payload.sub}/surface-permissions`
      });

      const response = await iamSdk.user.surfacePermissions({
        params: { id: payload.sub },
        headers
      });

      if (response.code !== 200 || !response.response) {
        return new Set<string>();
      }

      const permissions = new Set(
        response.response.map((permission) => permission.slug)
      );
      await authCacheService.setCachedPermissions(payload.sub, permissions);
      return permissions;
    } catch (error) {
      console.error('[surfacePermissions] Error surfacing permissions:', error);
      return new Set<string>();
    }
  };
}

export function createSurfaceRoles(params: {
  authCacheService: AuthCacheService;
  iamUrl: string;
  hmacSecretKey: string;
}): (payload: { sub?: string }) => Promise<Set<string>> {
  const { authCacheService, iamUrl, hmacSecretKey } = params;

  return async (payload) => {
    if (!payload.sub) {
      return new Set<string>();
    }

    const cached = await authCacheService.getCachedRoles(payload.sub);
    if (cached) {
      return cached;
    }

    try {
      const iamSdk = await getIamSdk(iamUrl);
      const headers = generateHmacAuthHeaders({
        secretKey: hmacSecretKey,
        method: 'GET',
        path: `/${payload.sub}/surface-roles`
      });

      const response = await iamSdk.user.surfaceRoles({
        params: { id: payload.sub },
        headers
      });

      if (response.code !== 200 || !response.response) {
        return new Set<string>();
      }

      const roles = new Set(response.response.map((role) => role.name));
      await authCacheService.setCachedRoles(payload.sub, roles);
      return roles;
    } catch (error) {
      console.error('[surfaceRoles] Error surfacing roles:', error);
      return new Set<string>();
    }
  };
}
