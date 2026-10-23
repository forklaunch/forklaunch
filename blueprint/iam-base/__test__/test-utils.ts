import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { getEnvVar } from '@forklaunch/common';
import {
  BlueprintTestHarness,
  clearTestDatabase,
  DatabaseType,
  surfaceFromTestClaims,
  TEST_TOKENS,
  TestSetupResult
} from '@forklaunch/testing';
import { EntityManager } from '@mikro-orm/core';
import dotenv from 'dotenv';
import { existsSync, readdirSync } from 'fs';
import * as path from 'path';

export { TEST_TOKENS, TestSetupResult };

let harness: BlueprintTestHarness;

dotenv.config({ path: path.join(__dirname, '../.env.test') });

const migrationsPath = path.join(__dirname, '../migrations');
// A generated app starts with an empty migrations directory (`pnpm
// migrate:init` writes the first one for the chosen database), so build the
// schema from the entities until a migration exists.
const hasMigrations =
  existsSync(migrationsPath) &&
  readdirSync(migrationsPath).some((file) =>
    /^Migration.*\.(js|ts)$/.test(file)
  );

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  harness = new BlueprintTestHarness({
    getConfig: async () => {
      const { default: config } = await import('../mikro-orm.config');
      // MikroORM.init() mutates options.discovery.skipSyncDiscovery = true on
      // the object it receives. mikro-orm.config exports a single shared object
      // that the app's own DI container also builds a MikroORM from, so letting
      // the harness mutate it leaves the app's `new MikroORM(config)` with an
      // undefined `.em` (every route then crashes on `Orm.em.fork`). Hand the
      // harness its own discovery object so the mutation can't leak.
      return { ...config, discovery: { ...config.discovery } };
    },
    databaseType: getEnvVar('DATABASE_TYPE') as DatabaseType,
    useMigrations: hasMigrations,
    migrationsPath,
    // Serve a JWKS for JWKS_PUBLIC_KEY_URL and mint session tokens (setup.jwt).
    jwt: true
  });

  return await harness.setup();
};

export const cleanupTestDatabase = async (): Promise<void> => {
  if (harness) {
    await harness.cleanup();
  }
};

/**
 * Gives a router the auth options the IAM server mounts it with, but with
 * roles and permissions read from the test token's `roles` / `permissions`
 * claims. Local SDK calls made with {@link withMiddlewares} then run the real
 * JWT verification, session decoding and RBAC checks.
 */
export async function mountWithClaimsAuth(router: object): Promise<void> {
  const { ci, tokens } = await import('../bootstrapper');
  forklaunchRouter('/', schemaValidator, ci.resolve(tokens.OtelCollector), {
    auth: surfaceFromTestClaims
  }).use(router as never);
}

/**
 * Local SDK calls skip middlewares unless asked. This wraps an SDK call so it
 * runs them (parsing and authentication included), as a request through the
 * server would: `withMiddlewares(route.sdk.getUser)({ params, headers })`.
 */
export const withMiddlewares =
  <Request extends object, Response>(
    call: (request: Request) => Promise<Response>
  ) =>
  (request: Request): Promise<Response> =>
    call({ ...request, executeMiddlewares: true });

export async function clearDatabase(options?: {
  orm?: TestSetupResult['orm'];
  redis?: TestSetupResult['redis'];
}): Promise<void> {
  // clearTestDatabase deletes per entity; the many-to-many join tables are not
  // entities, and without migrations (a freshly generated app) their rows
  // outlive the roles and permissions, so the next setupTestData collides.
  if (options?.orm && getEnvVar('DATABASE_TYPE') !== 'mongodb') {
    const connection = options.orm.em.getConnection();
    for (const table of ['user_roles', 'role_permissions']) {
      await connection.execute(`delete from "${table}"`);
    }
  }
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
    providerFields: null
  });

  // Create test permission
  const createdPermission = em.create(Permission, {
    id: '123e4567-e89b-12d3-a456-426614174002',
    slug: 'read:users'
  });

  // Create test role
  const createdRole = em.create(Role, {
    id: '123e4567-e89b-12d3-a456-426614174000',
    name: 'admin',
    permissions: [createdPermission]
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
    providerFields: []
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
  slug: 'write:organizations'
};

export const mockRoleData = {
  name: 'editor',
  permissionIds: ['123e4567-e89b-12d3-a456-426614174002']
};

export const mockUpdateRoleData = {
  id: '123e4567-e89b-12d3-a456-426614174000',
  name: 'super-admin',
  permissionIds: ['123e4567-e89b-12d3-a456-426614174002']
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
