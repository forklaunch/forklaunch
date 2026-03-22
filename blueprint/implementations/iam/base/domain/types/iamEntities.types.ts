import {
  Organization,
  Permission,
  Role,
  User
} from '../../persistence/entities';

// organization entity mapper types
export type OrganizationEntities<OrganizationStatus> = {
  OrganizationMapper: {
    '~entity': (typeof Organization)['~entity'] & {
      status: OrganizationStatus[keyof OrganizationStatus];
    };
  };
  CreateOrganizationMapper: {
    '~entity': (typeof Organization)['~entity'] & {
      status: OrganizationStatus[keyof OrganizationStatus];
    };
  };
  UpdateOrganizationMapper: {
    '~entity': (typeof Organization)['~entity'] & {
      status: OrganizationStatus[keyof OrganizationStatus];
    };
  };
};

// permission entity mapper types
export type PermissionEntities = {
  PermissionMapper: {
    '~entity': (typeof Permission)['~entity'];
  };
  CreatePermissionMapper: {
    '~entity': (typeof Permission)['~entity'];
  };
  UpdatePermissionMapper: {
    '~entity': (typeof Permission)['~entity'];
  };
  RoleEntityMapper: {
    '~entity': (typeof Role)['~entity'];
  };
};

// role entity mapper types
export type RoleEntities = {
  RoleMapper: {
    '~entity': (typeof Role)['~entity'];
  };
  CreateRoleMapper: {
    '~entity': (typeof Role)['~entity'];
  };
  UpdateRoleMapper: {
    '~entity': (typeof Role)['~entity'];
  };
};

// user entity mapper types
export type UserEntities = {
  UserMapper: {
    '~entity': (typeof User)['~entity'];
  };
  CreateUserMapper: {
    '~entity': (typeof User)['~entity'];
  };
  UpdateUserMapper: {
    '~entity': (typeof User)['~entity'];
  };
};
