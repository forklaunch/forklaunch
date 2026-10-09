import type { AuthCacheService } from '@forklaunch/blueprint-core';
import { generateHmacAuthHeaders } from '@forklaunch/core/http';
import { universalSdk } from '@forklaunch/universal-sdk';
import type { IamSdkClient } from './sdk';
import {
  authorizationScopePath,
  getAuthorizationScope,
  type AuthorizationPayload,
  type AuthorizationScope
} from './domain/utils/authorizationScope.util';

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

type RemoteParameters = {
  authCacheService: AuthCacheService;
  iamUrl: string;
  hmacSecretKey: string;
};

// Deliberately never cache authorization decisions. Each call checks current
// session and membership, so removal/role changes take effect immediately.
// Keep authCacheService in the factory contract for generated service compatibility.
export async function createSurfaceRoles(params: RemoteParameters) {
  const sdk = await getIamSdk(params.iamUrl);
  return async (payload: AuthorizationPayload): Promise<Set<string>> => {
    const scope = getAuthorizationScope(payload);
    if (!scope) return new Set();
    try {
      const response = await sdk.user.surfaceScopedRoles({
        params: {
          id: scope.userId,
          organizationId: scope.organizationId,
          sessionId: scope.sessionId
        },
        headers: generateHmacAuthHeaders({
          secretKey: params.hmacSecretKey,
          method: 'GET',
          path: authorizationScopePath(scope, 'roles')
        })
      });
      return response.code === 200 && response.response
        ? new Set(response.response.map((role: { name: string }) => role.name))
        : new Set();
    } catch {
      return new Set();
    }
  };
}

export async function createSurfacePermissions(params: RemoteParameters) {
  const sdk = await getIamSdk(params.iamUrl);
  return async (payload: AuthorizationPayload): Promise<Set<string>> => {
    const scope = getAuthorizationScope(payload);
    if (!scope) return new Set();
    try {
      const response = await sdk.user.surfaceScopedPermissions({
        params: {
          id: scope.userId,
          organizationId: scope.organizationId,
          sessionId: scope.sessionId
        },
        headers: generateHmacAuthHeaders({
          secretKey: params.hmacSecretKey,
          method: 'GET',
          path: authorizationScopePath(scope, 'permissions')
        })
      });
      return response.code === 200 && response.response
        ? new Set(
            response.response.map(
              (permission: { slug: string }) => permission.slug
            )
          )
        : new Set();
    } catch {
      return new Set();
    }
  };
}

// Older user-only local services cannot satisfy this explicit scoped contract.
export function createSurfaceRolesLocally(params: {
  authCacheService: AuthCacheService;
  userService: {
    surfaceRolesScoped: (
      scope: AuthorizationScope
    ) => Promise<Array<{ name: string }>>;
  };
}) {
  return async (payload: AuthorizationPayload): Promise<Set<string>> => {
    const scope = getAuthorizationScope(payload);
    if (!scope) return new Set();
    try {
      return new Set(
        (await params.userService.surfaceRolesScoped(scope)).map(
          (role) => role.name
        )
      );
    } catch {
      return new Set();
    }
  };
}

export function createSurfacePermissionsLocally(params: {
  authCacheService: AuthCacheService;
  userService: {
    surfacePermissionsScoped: (
      scope: AuthorizationScope
    ) => Promise<Array<{ slug: string }>>;
  };
}) {
  return async (payload: AuthorizationPayload): Promise<Set<string>> => {
    const scope = getAuthorizationScope(payload);
    if (!scope) return new Set();
    try {
      return new Set(
        (await params.userService.surfacePermissionsScoped(scope)).map(
          (permission) => permission.slug
        )
      );
    } catch {
      return new Set();
    }
  };
}
