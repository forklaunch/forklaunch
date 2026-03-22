import { PERMISSIONS, ROLES } from '@forklaunch/blueprint-core';
import { InferEntity, RequiredEntityData } from '@mikro-orm/core';
import { v4 as uuidv4 } from 'uuid';
import { OrganizationStatus } from '../domain/enum/organizationStatus.enum';
import { Organization, Role, User } from './entities';
import { Permission } from './entities/permission.entity';

//! Begin seed data - RBAC Permissions
export const platformReadPermission = {
  id: uuidv4(),
  slug: PERMISSIONS.PLATFORM_READ,
  createdAt: new Date(),
  updatedAt: new Date()
} satisfies RequiredEntityData<InferEntity<typeof Permission>>;

export const platformWritePermission = {
  id: uuidv4(),
  slug: PERMISSIONS.PLATFORM_WRITE,
  createdAt: new Date(),
  updatedAt: new Date()
} satisfies RequiredEntityData<InferEntity<typeof Permission>>;

//! RBAC Roles
export const viewerRole = {
  id: uuidv4(),
  name: ROLES.VIEWER,
  permissions: [platformReadPermission.id],
  createdAt: new Date(),
  updatedAt: new Date()
} satisfies RequiredEntityData<InferEntity<typeof Role>>;

export const editorRole = {
  id: uuidv4(),
  name: ROLES.EDITOR,
  permissions: [platformReadPermission.id, platformWritePermission.id],
  createdAt: new Date(),
  updatedAt: new Date()
} satisfies RequiredEntityData<InferEntity<typeof Role>>;

export const adminRole = {
  id: uuidv4(),
  name: ROLES.ADMIN,
  permissions: [platformReadPermission.id, platformWritePermission.id],
  createdAt: new Date(),
  updatedAt: new Date()
} satisfies RequiredEntityData<InferEntity<typeof Role>>;

export const systemRole = {
  id: uuidv4(),
  name: ROLES.SYSTEM,
  permissions: [platformReadPermission.id, platformWritePermission.id],
  createdAt: new Date(),
  updatedAt: new Date()
} satisfies RequiredEntityData<InferEntity<typeof Role>>;

export const user = {
  id: uuidv4(),
  email: 'test@test.com',
  firstName: 'Test',
  lastName: 'User',
  roles: [adminRole.id],
  providerFields: null,
  createdAt: new Date(),
  updatedAt: new Date()
} satisfies RequiredEntityData<InferEntity<typeof User>>;

export const organization = {
  name: 'Test',
  users: [user.id],
  subscription: 'test',
  domain: 'test.com',
  status: OrganizationStatus.ACTIVE,
  providerFields: null,
  createdAt: new Date(),
  updatedAt: new Date()
} satisfies RequiredEntityData<InferEntity<typeof Organization>>;
