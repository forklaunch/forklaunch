import { getEnvVar } from '@forklaunch/common';
import {
  BlueprintTestHarness,
  clearTestDatabase,
  DatabaseType,
  TEST_TOKENS,
  TestSetupResult
} from '@forklaunch/testing';
import { EntityManager, MikroORM } from '@mikro-orm/core';
import dotenv from 'dotenv';
import * as path from 'path';

export { TEST_TOKENS, TestSetupResult };

let harness: BlueprintTestHarness;

dotenv.config({ path: path.join(__dirname, '../.env.test') });

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  harness = new BlueprintTestHarness({
    getConfig: async () => {
      const { default: config } = await import('../mikro-orm.config');
      return config;
    },
    databaseType: getEnvVar('DATABASE_TYPE') as DatabaseType,
    useMigrations: true,
    migrationsPath: path.join(__dirname, '../migrations')
  });

  return await harness.setup();
};

export const cleanupTestDatabase = async (): Promise<void> => {
  if (harness) {
    await harness.cleanup();
  }
};

export async function clearDatabase(options?: {
  orm?: MikroORM;
  redis?: TestSetupResult['redis'];
}): Promise<void> {
  await clearTestDatabase(options);
}

export const setupTestData = async (em: EntityManager) => {
  const { Permission } = await import(
    '../persistence/entities/permission.entity'
  );
  const { Role } = await import('../persistence/entities/role.entity');
  const { User } = await import('../persistence/entities/user.entity');
  const { Organization } = await import(
    '../persistence/entities/organization.entity'
  );
  const { OrganizationStatus } = await import(
    '../domain/enum/organizationStatus.enum'
  );

  // Create test organization
  const createdOrganization = em.create(Organization, {
    id: '123e4567-e89b-12d3-a456-426614174001',
    name: 'Test Organization',
    domain: 'test.com',
    subscription: 'premium',
    status: OrganizationStatus.ACTIVE,
    providerFields: null,
    createdAt: new Date(),
    updatedAt: new Date()
  });

  // Create test permission
  const createdPermission = em.create(Permission, {
    id: '123e4567-e89b-12d3-a456-426614174002',
    slug: 'read:users',
    createdAt: new Date(),
    updatedAt: new Date()
  });

  // Create test role
  const createdRole = em.create(Role, {
    id: '123e4567-e89b-12d3-a456-426614174000',
    name: 'admin',
    permissions: [createdPermission],
    createdAt: new Date(),
    updatedAt: new Date()
  });

  // Create test user
  em.create(User, {
    id: '123e4567-e89b-12d3-a456-426614174000',
    email: 'test@example.com',
    firstName: 'John',
    lastName: 'Doe',
    phoneNumber: '+1234567890',
    organization: createdOrganization,
    roles: [createdRole],
    subscription: 'enterprise',
    providerFields: [],
    createdAt: new Date(),
    updatedAt: new Date()
  });

  await em.flush();
};

// Mock data constants
export const mockUserData = {
  email: 'newuser@example.com',
  password: 'password123',
  firstName: 'New',
  lastName: 'User',
  organization: '123e4567-e89b-12d3-a456-426614174001',
  roles: ['123e4567-e89b-12d3-a456-426614174000'],
  phoneNumber: '+1234567890',
  subscription: 'premium'
};

export const mockUpdateUserData = {
  id: '123e4567-e89b-12d3-a456-426614174000',
  email: 'updated@example.com',
  firstName: 'Jane',
  lastName: 'Smith',
  roles: ['123e4567-e89b-12d3-a456-426614174000'],
  phoneNumber: '+0987654321'
};

export const mockOrganizationData = {
  name: 'New Organization',
  domain: 'neworg.com',
  subscription: 'enterprise',
  status: 'active',
  logoUrl: 'https://example.com/logo.png'
};

export const mockUpdateOrganizationData = {
  id: '123e4567-e89b-12d3-a456-426614174001',
  name: 'Updated Organization',
  domain: 'updated.com',
  subscription: 'basic',
  status: 'inactive',
  logoUrl: 'https://example.com/updated-logo.png'
};

export const mockPermissionData = {
  slug: 'write:users'
};

export const mockUpdatePermissionData = {
  id: '123e4567-e89b-12d3-a456-426614174002',
  slug: 'write:organizations',
  createdAt: new Date(),
  updatedAt: new Date()
};

export const mockRoleData = {
  name: 'editor',
  permissions: ['123e4567-e89b-12d3-a456-426614174002']
};

export const mockUpdateRoleData = {
  id: '123e4567-e89b-12d3-a456-426614174000',
  name: 'super-admin',
  permissions: ['123e4567-e89b-12d3-a456-426614174002']
};

// Expected response templates
export const mockRoleResponse = [
  {
    id: '123e4567-e89b-12d3-a456-426614174000',
    name: 'admin',
    permissions: [
      {
        id: '123e4567-e89b-12d3-a456-426614174002',
        slug: 'read:users',
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      }
    ],
    createdAt: expect.any(Date),
    updatedAt: expect.any(Date)
  }
];

export const mockPermissionResponse = [
  {
    id: '123e4567-e89b-12d3-a456-426614174002',
    slug: 'read:users',
    createdAt: expect.any(Date),
    updatedAt: expect.any(Date)
  }
];

export const mockOrganizationResponse = {
  id: '123e4567-e89b-12d3-a456-426614174001',
  name: 'Test Organization',
  domain: 'test.com',
  subscription: 'premium',
  status: 'active',
  createdAt: expect.any(Date),
  updatedAt: expect.any(Date)
};
