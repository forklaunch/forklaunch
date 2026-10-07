// Base permission types - can be extended to include more granular operations
export const PERMISSIONS = {
  PLATFORM_READ: 'platform:read',
  PLATFORM_WRITE: 'platform:write'
};
export const PLATFORM_READ_PERMISSIONS = new Set([PERMISSIONS.PLATFORM_READ]);
export const PLATFORM_WRITE_PERMISSIONS = new Set([
  ...PLATFORM_READ_PERMISSIONS,
  PERMISSIONS.PLATFORM_WRITE
]);
// Role definitions with associated permissions
export const ROLES = {
  VIEWER: 'viewer',
  EDITOR: 'editor',
  ADMIN: 'admin',
  SYSTEM: 'system'
};
export const PLATFORM_SYSTEM_ROLES = new Set([ROLES.SYSTEM]);
export const PLATFORM_ADMIN_ROLES = new Set([
  ...PLATFORM_SYSTEM_ROLES,
  ROLES.ADMIN
]);
export const PLATFORM_EDITOR_ROLES = new Set([
  ...PLATFORM_ADMIN_ROLES,
  ROLES.EDITOR
]);
export const PLATFORM_VIEWER_ROLES = new Set([
  ...PLATFORM_EDITOR_ROLES,
  ROLES.VIEWER
]);
// Permission mappings for each role
export const ROLE_PERMISSIONS = {
  [ROLES.VIEWER]: [PERMISSIONS.PLATFORM_READ],
  [ROLES.EDITOR]: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE],
  [ROLES.ADMIN]: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE],
  [ROLES.SYSTEM]: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE]
};
//# sourceMappingURL=rbac.js.map
