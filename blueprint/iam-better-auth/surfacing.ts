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
 * using organization-bound HMAC authentication. Authorization reads fresh membership;
 * the older user-only cache cannot distinguish organizations or revoked membership.
 */
export async function createSurfaceRoles(params: {
  authCacheService: AuthCacheService;
  iamUrl: string;
  hmacSecretKey: string;
}): Promise<
  (payload: {
    sub?: string;
    activeOrganizationId?: string;
  }) => Promise<Set<string>>
> {
  const { iamUrl, hmacSecretKey } = params;
  const iamSdk = await getIamSdk(iamUrl);

  return async (payload: { sub?: string; activeOrganizationId?: string }) => {
    if (
      typeof payload.sub !== 'string' ||
      !payload.sub ||
      typeof payload.activeOrganizationId !== 'string' ||
      !payload.activeOrganizationId
    ) {
      return new Set<string>();
    }

    try {
      // Path must match req.path (route-relative, without router prefix)
      const headers = generateHmacAuthHeaders({
        secretKey: hmacSecretKey,
        method: 'GET',
        path: `/${encodeURIComponent(payload.sub)}/organizations/${encodeURIComponent(payload.activeOrganizationId)}/surface-roles`
      });

      const response = await iamSdk.user.surfaceRoles({
        params: {
          id: payload.sub,
          organizationId: payload.activeOrganizationId
        },
        headers
      });

      if (response.code !== 200 || !response.response) {
        return new Set<string>();
      }

      const roles = new Set<string>(
        response.response.map((role: { name: string }) => role.name)
      );

      return roles;
    } catch {
      console.error('[surfaceRoles] Membership lookup failed');
      return new Set<string>();
    }
  };
}

/**
 * Create a surfacePermissions function that fetches user permissions from IAM service
 * using organization-bound HMAC authentication. Authorization reads fresh membership;
 * the older user-only cache cannot distinguish organizations or revoked membership.
 */
export async function createSurfacePermissions(params: {
  authCacheService: AuthCacheService;
  iamUrl: string;
  hmacSecretKey: string;
}): Promise<
  (payload: {
    sub?: string;
    activeOrganizationId?: string;
  }) => Promise<Set<string>>
> {
  const { iamUrl, hmacSecretKey } = params;
  const iamSdk = await getIamSdk(iamUrl);

  return async (payload: { sub?: string; activeOrganizationId?: string }) => {
    if (
      typeof payload.sub !== 'string' ||
      !payload.sub ||
      typeof payload.activeOrganizationId !== 'string' ||
      !payload.activeOrganizationId
    ) {
      return new Set<string>();
    }

    try {
      const headers = generateHmacAuthHeaders({
        secretKey: hmacSecretKey,
        method: 'GET',
        path: `/${encodeURIComponent(payload.sub)}/organizations/${encodeURIComponent(payload.activeOrganizationId)}/surface-permissions`
      });

      const response = await iamSdk.user.surfacePermissions({
        params: {
          id: payload.sub,
          organizationId: payload.activeOrganizationId
        },
        headers
      });

      if (response.code !== 200 || !response.response) {
        return new Set<string>();
      }

      const permissions = new Set<string>(
        response.response.map((permission: { slug: string }) => permission.slug)
      );

      return permissions;
    } catch {
      console.error('[surfacePermissions] Membership lookup failed');
      return new Set<string>();
    }
  };
}

/**
 * Create a surfaceRoles function that fetches user roles from local database
 * via the user service and the organization bound to the verified token.
 */
export function createSurfaceRolesLocally(params: {
  authCacheService: AuthCacheService;
  userService: {
    surfaceRoles: (params: {
      id: string;
      organizationId: string;
    }) => Promise<Array<{ name: string }>>;
  };
}): (payload: {
  sub?: string;
  activeOrganizationId?: string;
}) => Promise<Set<string>> {
  const { userService } = params;

  return async (payload: { sub?: string; activeOrganizationId?: string }) => {
    if (
      typeof payload.sub !== 'string' ||
      !payload.sub ||
      typeof payload.activeOrganizationId !== 'string' ||
      !payload.activeOrganizationId
    ) {
      return new Set<string>();
    }

    try {
      const rolesArray = await userService.surfaceRoles({
        id: payload.sub,
        organizationId: payload.activeOrganizationId
      });
      const roles = new Set<string>(rolesArray.map((role) => role.name));
      return roles;
    } catch {
      console.error('Local membership lookup failed');
      return new Set<string>();
    }
  };
}

/**
 * Create a surfacePermissions function that fetches user permissions from local database
 * via the user service and the organization bound to the verified token.
 */
export function createSurfacePermissionsLocally(params: {
  authCacheService: AuthCacheService;
  userService: {
    surfacePermissions: (params: {
      id: string;
      organizationId: string;
    }) => Promise<Array<{ slug: string }>>;
  };
}): (payload: {
  sub?: string;
  activeOrganizationId?: string;
}) => Promise<Set<string>> {
  const { userService } = params;

  return async (payload: { sub?: string; activeOrganizationId?: string }) => {
    if (
      typeof payload.sub !== 'string' ||
      !payload.sub ||
      typeof payload.activeOrganizationId !== 'string' ||
      !payload.activeOrganizationId
    ) {
      return new Set<string>();
    }

    try {
      const permissionsArray = await userService.surfacePermissions({
        id: payload.sub,
        organizationId: payload.activeOrganizationId
      });
      const permissions = new Set<string>(
        permissionsArray.map((permission) => permission.slug)
      );
      return permissions;
    } catch {
      console.error('Local permission lookup failed');
      return new Set<string>();
    }
  };
}
