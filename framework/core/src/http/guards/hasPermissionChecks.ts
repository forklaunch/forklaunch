export function hasPermissionChecks(maybePermissionedAuth: unknown) {
  if (
    typeof maybePermissionedAuth !== 'object' ||
    maybePermissionedAuth === null
  ) {
    return false;
  }

  const hasAllowedPermissions =
    'allowedPermissions' in maybePermissionedAuth &&
    maybePermissionedAuth.allowedPermissions instanceof Set &&
    maybePermissionedAuth.allowedPermissions.size > 0;

  const hasForbiddenPermissions =
    'forbiddenPermissions' in maybePermissionedAuth &&
    maybePermissionedAuth.forbiddenPermissions instanceof Set &&
    maybePermissionedAuth.forbiddenPermissions.size > 0;

  return hasAllowedPermissions || hasForbiddenPermissions;
}
