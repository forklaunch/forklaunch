import { PERMISSIONS, ROLES } from '@forklaunch/blueprint-core';

// Organization membership permissions only. An organization owner is not a
// platform administrator, and these statements do not alter role allowlists.
export const organizationRoleStatements = {
  owner: { platform: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE] },
  [ROLES.ADMIN]: {
    platform: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE]
  },
  [ROLES.EDITOR]: {
    platform: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE]
  },
  [ROLES.VIEWER]: { platform: [PERMISSIONS.PLATFORM_READ] },
  [ROLES.SYSTEM]: {
    platform: [PERMISSIONS.PLATFORM_READ, PERMISSIONS.PLATFORM_WRITE]
  }
};

export function builtinOrganizationPermissions(role: string): string[] {
  if (!Object.prototype.hasOwnProperty.call(organizationRoleStatements, role))
    return [];
  return [
    ...organizationRoleStatements[
      role as keyof typeof organizationRoleStatements
    ].platform
  ];
}
