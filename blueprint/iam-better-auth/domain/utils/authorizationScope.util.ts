/** Authorization context comes only from the already verified JWT/session payload. */
export interface AuthorizationScope {
  userId: string;
  organizationId: string;
  sessionId: string;
}

export interface AuthorizationPayload {
  sub?: unknown;
  activeOrganizationId?: unknown;
  sessionId?: unknown;
}

export function getAuthorizationScope(
  payload: AuthorizationPayload
): AuthorizationScope | null {
  if (!payload || typeof payload !== 'object') return null;
  const values = [payload.sub, payload.activeOrganizationId, payload.sessionId];
  if (
    !values.every(
      (value) => typeof value === 'string' && /^[A-Za-z0-9_-]+$/.test(value)
    )
  ) {
    return null;
  }
  return {
    userId: payload.sub as string,
    organizationId: payload.activeOrganizationId as string,
    sessionId: payload.sessionId as string
  };
}

/** Every scope component is bound into the HMAC-signed route path. */
export function authorizationScopePath(
  scope: AuthorizationScope,
  resource: 'roles' | 'permissions'
): string {
  return `/${encodeURIComponent(scope.userId)}/organizations/${encodeURIComponent(scope.organizationId)}/sessions/${encodeURIComponent(scope.sessionId)}/surface-${resource}`;
}
