import type { AuthCacheService } from '@forklaunch/blueprint-core';
import { generateHmacAuthHeaders } from '@forklaunch/core/http';
import { universalSdk } from '@forklaunch/universal-sdk';
import { IamSdkClient } from './sdk';

export { generateHmacAuthHeaders } from '@forklaunch/core/http';

const sdkCache = new Map<string, IamSdkClient>();

async function getIamSdk(iamUrl: string): Promise<IamSdkClient> {
  let sdk = sdkCache.get(iamUrl);
  if (!sdk) {
    sdk = await universalSdk<IamSdkClient>({
      host: iamUrl,
      registryOptions: { path: 'api/v1/openapi' }
    });
    sdkCache.set(iamUrl, sdk);
  }
  return sdk;
}

/**
 * Create a surfaceRoles function that fetches user roles from IAM service
 * using HMAC authentication and caches results.
 */
export async function createSurfaceRoles(params: {
  authCacheService: AuthCacheService;
  iamUrl: string;
  hmacSecretKey: string;
}): Promise<(payload: { sub?: string }) => Promise<Set<string>>> {
  const { authCacheService, iamUrl, hmacSecretKey } = params;
  const iamSdk = await getIamSdk(iamUrl);

  return async (payload: { sub?: string }) => {
    if (!payload.sub) {
      return new Set<string>();
    }

    const cached = await authCacheService.getCachedRoles(payload.sub);
    if (cached) {
      return cached;
    }

    try {
      // Path must match req.path (route-relative, without router prefix)
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

      const roles = new Set<string>(
        response.response.map((role: { name: string }) => role.name)
      );

      await authCacheService.setCachedRoles(payload.sub, roles);
      return roles;
    } catch (error) {
      console.error('[surfaceRoles] Error surfacing roles:', error);
      return new Set<string>();
    }
  };
}

/**
 * Create a surfacePermissions function that fetches user permissions from IAM service
 * using HMAC authentication and caches results.
 */
export async function createSurfacePermissions(params: {
  authCacheService: AuthCacheService;
  iamUrl: string;
  hmacSecretKey: string;
}): Promise<(payload: { sub?: string }) => Promise<Set<string>>> {
  const { authCacheService, iamUrl, hmacSecretKey } = params;
  const iamSdk = await getIamSdk(iamUrl);

  return async (payload: { sub?: string }) => {
    if (!payload.sub) {
      return new Set<string>();
    }

    const cached = await authCacheService.getCachedPermissions(payload.sub);
    if (cached) {
      return cached;
    }

    try {
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

      const permissions = new Set<string>(
        response.response.map((permission: { slug: string }) => permission.slug)
      );

      await authCacheService.setCachedPermissions(payload.sub, permissions);
      return permissions;
    } catch (error) {
      console.error('[surfacePermissions] Error surfacing permissions:', error);
      return new Set<string>();
    }
  };
}

/**
 * Create a surfaceRoles function that fetches user roles from local database
 * via user service and caches results.
 */
export function createSurfaceRolesLocally(params: {
  authCacheService: AuthCacheService;
  userService: {
    surfaceRoles: (params: { id: string }) => Promise<Array<{ name: string }>>;
  };
}): (payload: { sub?: string }) => Promise<Set<string>> {
  const { authCacheService, userService } = params;

  return async (payload: { sub?: string }) => {
    if (!payload.sub) {
      return new Set<string>();
    }

    const cached = await authCacheService.getCachedRoles(payload.sub);
    if (cached) {
      return cached;
    }

    try {
      const rolesArray = await userService.surfaceRoles({ id: payload.sub });
      const roles = new Set<string>(rolesArray.map((role) => role.name));
      await authCacheService.setCachedRoles(payload.sub, roles);
      return roles;
    } catch (error) {
      console.error('Failed to surface roles locally:', error);
      return new Set<string>();
    }
  };
}

/**
 * Create a surfacePermissions function that fetches user permissions from local database
 * via user service and caches results.
 */
export function createSurfacePermissionsLocally(params: {
  authCacheService: AuthCacheService;
  userService: {
    surfacePermissions: (params: {
      id: string;
    }) => Promise<Array<{ slug: string }>>;
  };
}): (payload: { sub?: string }) => Promise<Set<string>> {
  const { authCacheService, userService } = params;

  return async (payload: { sub?: string }) => {
    if (!payload.sub) {
      return new Set<string>();
    }

    const cached = await authCacheService.getCachedPermissions(payload.sub);
    if (cached) {
      return cached;
    }

    try {
      const permissionsArray = await userService.surfacePermissions({
        id: payload.sub
      });
      const permissions = new Set<string>(
        permissionsArray.map((permission) => permission.slug)
      );
      await authCacheService.setCachedPermissions(payload.sub, permissions);
      return permissions;
    } catch (error) {
      console.error('Failed to surface permissions locally:', error);
      return new Set<string>();
    }
  };
}
