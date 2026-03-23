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
    migrationsPath: path.join(__dirname, '../migrations'),
    customEnvVars: {
      CORS_ORIGINS: getEnvVar('CORS_ORIGINS')
    }
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
  const { User } = await import('../persistence/entities/user.entity');
  const { Organization } = await import(
    '../persistence/entities/organization.entity'
  );
  const { Member } = await import('../persistence/entities/member.entity');
  const { OrganizationRole } = await import(
    '../persistence/entities/organizationRole.entity'
  );
  const { Session } = await import('../persistence/entities/session.entity');

  // Create test organization
  em.create(Organization, {
    id: '123e4567-e89b-12d3-a456-426614174001',
    name: 'Test Organization',
    slug: 'test-organization',
    metadata: null,
    domain: 'test.com',
    subscription: 'premium',
    status: 'active'
  });

  // Create test user
  em.create(User, {
    id: '123e4567-e89b-12d3-a456-426614174000',
    email: 'test@example.com',
    emailVerified: true,
    name: 'John Doe',
    firstName: 'John',
    lastName: 'Doe',
    phoneNumber: '+1234567890',
    subscription: 'enterprise',
    providerFields: null
  });

  // Create member (user belongs to org as admin)
  em.create(Member, {
    id: '123e4567-e89b-12d3-a456-426614174010',
    organizationId: '123e4567-e89b-12d3-a456-426614174001',
    userId: '123e4567-e89b-12d3-a456-426614174000',
    role: 'admin'
  });

  // Create org roles (admin has platform_read + platform_write)
  em.create(OrganizationRole, {
    id: '123e4567-e89b-12d3-a456-426614174020',
    organizationId: '123e4567-e89b-12d3-a456-426614174001',
    role: 'admin',
    permission: 'platform_read'
  });

  em.create(OrganizationRole, {
    id: '123e4567-e89b-12d3-a456-426614174021',
    organizationId: '123e4567-e89b-12d3-a456-426614174001',
    role: 'admin',
    permission: 'platform_write'
  });

  // Create active session for user with active org
  em.create(Session, {
    id: '123e4567-e89b-12d3-a456-426614174030',
    user: '123e4567-e89b-12d3-a456-426614174000',
    token: 'test-session-token',
    expiresAt: new Date(Date.now() + 1000 * 60 * 60 * 24),
    ipAddress: '127.0.0.1',
    userAgent: 'test',
    activeOrganizationId: '123e4567-e89b-12d3-a456-426614174001'
  });

  await em.flush();
};

// Expected response templates
export const mockRoleResponse = [{ name: 'admin' }];

export const mockPermissionResponse = [
  { slug: 'platform_read' },
  { slug: 'platform_write' }
];
