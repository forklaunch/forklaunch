export function hasRoleChecks(maybeRoledAuth: unknown): boolean {
  if (typeof maybeRoledAuth !== 'object' || maybeRoledAuth === null) {
    return false;
  }

  const hasAllowedRoles =
    'allowedRoles' in maybeRoledAuth &&
    maybeRoledAuth.allowedRoles instanceof Set &&
    maybeRoledAuth.allowedRoles.size > 0;

  const hasForbiddenRoles =
    'forbiddenRoles' in maybeRoledAuth &&
    maybeRoledAuth.forbiddenRoles instanceof Set &&
    maybeRoledAuth.forbiddenRoles.size > 0;

  return hasAllowedRoles || hasForbiddenRoles;
}