// SQL User, Organization and Session IDs use sqlBaseProperties UUIDs.
// Validate before database lookup; malformed internal inputs must not cause SQL errors.
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
      (value) =>
        typeof value === 'string' &&
        value.length === 36 &&
        /^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/i.test(value)
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

/**
 * The full request path (including the `/user` router mount) the HMAC
 * verifier checks; every scope component is bound into it.
 */
export function authorizationScopePath(
  scope: AuthorizationScope,
  resource: 'roles' | 'permissions'
): string {
  return `/user/${encodeURIComponent(scope.userId)}/organizations/${encodeURIComponent(scope.organizationId)}/sessions/${encodeURIComponent(scope.sessionId)}/surface-${resource}`;
}
