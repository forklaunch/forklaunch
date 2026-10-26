/**
 * The full request path (router mount included) that IAM's HMAC verifier
 * checks for a user's surfaced roles or permissions. Signing the
 * router-relative `/:id/surface-*` form is rejected with 403.
 */
export function surfacingPath(
  userId: string,
  resource: 'roles' | 'permissions'
): string {
  return `/user/${encodeURIComponent(userId)}/surface-${resource}`;
}
