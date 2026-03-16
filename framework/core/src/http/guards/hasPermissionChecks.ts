export function hasPermissionChecks(maybePermissionedAuth: unknown) {
  if (
    typeof maybePermissionedAuth !== 'object' ||
    maybePermissionedAuth === null
  ) {
    return false;
  }

  const auth = maybePermissionedAuth as Record<string, unknown>;

  return (
    ('allowedPermissions' in auth && auth.allowedPermissions != null) ||
    ('forbiddenPermissions' in auth && auth.forbiddenPermissions != null)
  );
}
