import { SchemaValidator } from '@forklaunch/blueprint-core';
import { Schema } from '@forklaunch/validator';
import {
  Organization,
  Permission,
  Role,
  User
} from '../../persistence/entities';
import {
  CreateOrganizationMapper,
  OrganizationMapper,
  UpdateOrganizationMapper
} from '../mappers/organization.mappers';
import {
  CreatePermissionMapper,
  PermissionMapper,
  UpdatePermissionMapper
} from '../mappers/permission.mappers';
import {
  CreateRoleMapper,
  RoleEntityMapper,
  RoleMapper,
  UpdateRoleMapper
} from '../mappers/role.mappers';
import {
  CreateUserMapper,
  UpdateUserMapper,
  UserMapper
} from '../mappers/user.mappers';

// organization mappers
export type OrganizationMapperTypes = {
  OrganizationMapper: typeof Organization;
  CreateOrganizationMapper: typeof Organization;
  UpdateOrganizationMapper: typeof Organization;
};

// organization dto types
export type OrganizationDtoTypes = {
  OrganizationMapper: Schema<typeof OrganizationMapper.schema, SchemaValidator>;
  CreateOrganizationMapper: Schema<
    typeof CreateOrganizationMapper.schema,
    SchemaValidator
  >;
  UpdateOrganizationMapper: Schema<
    typeof UpdateOrganizationMapper.schema,
    SchemaValidator
  >;
};

// permission mappers
export type PermissionMapperTypes = {
  PermissionMapper: typeof Permission;
  CreatePermissionMapper: typeof Permission;
  UpdatePermissionMapper: typeof Permission;
  RoleEntityMapper: typeof Role;
};

// permission dto types
export type PermissionDtoTypes = {
  PermissionMapper: Schema<typeof PermissionMapper.schema, SchemaValidator>;
  CreatePermissionMapper: Schema<
    typeof CreatePermissionMapper.schema,
    SchemaValidator
  >;
  UpdatePermissionMapper: Schema<
    typeof UpdatePermissionMapper.schema,
    SchemaValidator
  >;
  RoleEntityMapper: Schema<typeof RoleEntityMapper.schema, SchemaValidator>;
};

// role mappers
export type RoleMapperTypes = {
  RoleMapper: typeof Role;
  CreateRoleMapper: typeof Role;
  UpdateRoleMapper: typeof Role;
};

// role dto types
export type RoleDtoTypes = {
  RoleMapper: Schema<typeof RoleMapper.schema, SchemaValidator>;
  CreateRoleMapper: Schema<typeof CreateRoleMapper.schema, SchemaValidator>;
  UpdateRoleMapper: Schema<typeof UpdateRoleMapper.schema, SchemaValidator>;
};

// user mappers
export type UserMapperTypes = {
  UserMapper: typeof User;
  CreateUserMapper: typeof User;
  UpdateUserMapper: typeof User;
};

// user dto types
export type UserDtoTypes = {
  UserMapper: Schema<typeof UserMapper.schema, SchemaValidator>;
  CreateUserMapper: Schema<typeof CreateUserMapper.schema, SchemaValidator>;
  UpdateUserMapper: Schema<typeof UpdateUserMapper.schema, SchemaValidator>;
};
