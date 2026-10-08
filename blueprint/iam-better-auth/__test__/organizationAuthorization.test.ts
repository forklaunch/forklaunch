import { beforeEach, expect, it, vi } from 'vitest';
const { APPLICATION_ADMIN_ROLES, PLATFORM_ADMIN_ROLES, ROLES } = await import(
  new URL('../../core/auth/rbac.ts', import.meta.url).href
);
vi.mock('../persistence/entities/member.entity', () => ({ Member: 'Member' }));
vi.mock('../persistence/entities/organizationRole.entity', () => ({
  OrganizationRole: 'OrganizationRole'
}));
const mocks = vi.hoisted(() => ({
  role: vi.fn(),
  permissions: vi.fn(),
  hmac: vi.fn((value: { path: string }) => ({ 'x-fixture-path': value.path }))
}));
vi.mock('@forklaunch/core/http', () => ({
  generateHmacAuthHeaders: mocks.hmac
}));
vi.mock('@forklaunch/universal-sdk', () => ({
  universalSdk: async () => ({
    user: { surfaceRoles: mocks.role, surfacePermissions: mocks.permissions }
  })
}));
import { SurfacingService } from '../domain/services/surfacing.service';
import { createSurfaceRoles, createSurfacePermissions } from '../surfacing';
beforeEach(() => vi.clearAllMocks());
it('allows application owners/admins without granting platform or system authority', () => {
  expect(APPLICATION_ADMIN_ROLES.has(ROLES.OWNER)).toBe(true);
  expect(APPLICATION_ADMIN_ROLES.has(ROLES.ADMIN)).toBe(true);
  expect(APPLICATION_ADMIN_ROLES.has(ROLES.VIEWER as never)).toBe(false);
  expect(APPLICATION_ADMIN_ROLES.has(ROLES.SYSTEM as never)).toBe(false);
  expect(PLATFORM_ADMIN_ROLES.has(ROLES.OWNER as never)).toBe(false);
});
it('looks up exact membership and never derives authority from another active session', async () => {
  const findOne = vi.fn(
    async (
      entity: unknown,
      query: { userId: string; organizationId: string }
    ) => {
      expect(entity).toBe('Member');
      return query.userId === 'user' && query.organizationId === 'org-a'
        ? { role: 'viewer' }
        : query.userId === 'user' && query.organizationId === 'org-b'
          ? { role: 'owner' }
          : null;
    }
  );
  const find = vi.fn(
    async (
      _entity: unknown,
      query: { organizationId: string; role: string }
    ) => [{ permission: query.organizationId + ':' + query.role }]
  );
  const service = new SurfacingService({ findOne, find } as never);
  expect(await service.surfaceRole('user', 'org-a')).toBe('viewer');
  expect(await service.surfaceRole('user', 'org-b')).toBe('owner');
  expect(await service.surfaceRole('other', 'org-b')).toBeNull();
  expect(await service.surfaceRole('user', '')).toBeNull();
  expect(await service.surfacePermissions('user', 'org-a')).toEqual([
    'org-a:viewer'
  ]);
  expect(findOne).not.toHaveBeenCalledWith('Session', expect.anything());
});
it('binds user and token organization into signed HMAC path and refuses the old user-only cache', async () => {
  const cache = {
    getCachedRoles: vi.fn(async () => new Set(['admin'])),
    setCachedRoles: vi.fn(),
    getCachedPermissions: vi.fn(async () => new Set(['write'])),
    setCachedPermissions: vi.fn()
  };
  mocks.role.mockImplementation(async ({ params }) => ({
    code: 200,
    response: [{ name: params.organizationId === 'org-a' ? 'viewer' : 'owner' }]
  }));
  mocks.permissions.mockImplementation(async ({ params }) => ({
    code: 200,
    response: [{ slug: params.organizationId + ':read' }]
  }));
  const options = {
    authCacheService: cache as never,
    iamUrl: 'http://fixture.invalid',
    hmacSecretKey: 'synthetic'
  };
  const roles = await createSurfaceRoles(options);
  const permissions = await createSurfacePermissions(options);
  expect(await roles({ sub: 'user', activeOrganizationId: 'org-b' })).toEqual(
    new Set(['owner'])
  );
  expect(await roles({ sub: 'user', activeOrganizationId: 'org-a' })).toEqual(
    new Set(['viewer'])
  );
  expect(mocks.role).toHaveBeenLastCalledWith({
    params: { id: 'user', organizationId: 'org-a' },
    headers: { 'x-fixture-path': '/user/organizations/org-a/surface-roles' }
  });
  expect(
    await permissions({ sub: 'user', activeOrganizationId: 'org-a' })
  ).toEqual(new Set(['org-a:read']));
  expect(await roles({ sub: 'user' })).toEqual(new Set());
  expect(await permissions({ sub: 'user' })).toEqual(new Set());
  expect(mocks.hmac).toHaveBeenLastCalledWith({
    secretKey: 'synthetic',
    method: 'GET',
    path: '/user/organizations/org-a/surface-permissions'
  });
  for (const method of Object.values(cache))
    expect(method).not.toHaveBeenCalled();
});
it('rechecks revoked membership instead of retaining a previous authorization cache entry', async () => {
  const roles = await createSurfaceRoles({
    authCacheService: {} as never,
    iamUrl: 'http://fixture.invalid',
    hmacSecretKey: 'synthetic'
  });
  mocks.role
    .mockResolvedValueOnce({ code: 200, response: [{ name: 'owner' }] })
    .mockResolvedValueOnce({ code: 200, response: [] });
  const token = { sub: 'user', activeOrganizationId: 'org-a' };
  expect(await roles(token)).toEqual(new Set(['owner']));
  expect(await roles(token)).toEqual(new Set());
});
it('fails closed on membership outages without logging signed request details', async () => {
  const log = vi.spyOn(console, 'error').mockImplementation(() => {});
  const roles = await createSurfaceRoles({
    authCacheService: {} as never,
    iamUrl: 'http://fixture.invalid',
    hmacSecretKey: 'synthetic'
  });
  mocks.role.mockRejectedValueOnce(new Error('private-signature-sentinel'));
  expect(await roles({ sub: 'user', activeOrganizationId: 'org-a' })).toEqual(
    new Set()
  );
  expect(JSON.stringify(log.mock.calls)).not.toContain(
    'private-signature-sentinel'
  );
  log.mockRestore();
});
