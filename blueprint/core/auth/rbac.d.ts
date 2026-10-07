export declare const PERMISSIONS: {
  readonly PLATFORM_READ: 'platform:read';
  readonly PLATFORM_WRITE: 'platform:write';
};
export type PERMISSIONS = (typeof PERMISSIONS)[keyof typeof PERMISSIONS];
export declare const PLATFORM_READ_PERMISSIONS: Set<'platform:read'>;
export declare const PLATFORM_WRITE_PERMISSIONS: Set<
  'platform:read' | 'platform:write'
>;
export declare const ROLES: {
  readonly VIEWER: 'viewer';
  readonly EDITOR: 'editor';
  readonly ADMIN: 'admin';
  readonly SYSTEM: 'system';
};
export type ROLES = (typeof ROLES)[keyof typeof ROLES];
export declare const PLATFORM_SYSTEM_ROLES: Set<'system'>;
export declare const PLATFORM_ADMIN_ROLES: Set<'admin' | 'system'>;
export declare const PLATFORM_EDITOR_ROLES: Set<'admin' | 'editor' | 'system'>;
export declare const PLATFORM_VIEWER_ROLES: Set<
  'admin' | 'editor' | 'system' | 'viewer'
>;
export declare const ROLE_PERMISSIONS: Record<ROLES, PERMISSIONS[]>;
//# sourceMappingURL=rbac.d.ts.map
