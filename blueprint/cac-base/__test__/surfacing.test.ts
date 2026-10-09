import type { AuthCacheService } from '@forklaunch/blueprint-core';
import { createSurfacePermissions, createSurfaceRoles } from '../surfacing';

// universalSdk resolves the IAM SDK from a live OpenAPI spec over the
// network — mocked here so cache-hit vs. cache-miss behavior can be
// asserted without a running IAM instance, the actual thing under test.
const surfacePermissions = vi.fn();
const surfaceRoles = vi.fn();
vi.mock('@forklaunch/universal-sdk', () => ({
  universalSdk: vi.fn(async () => ({
    user: { surfacePermissions, surfaceRoles }
  }))
}));

function fakeAuthCacheService(
  overrides: Partial<AuthCacheService> = {}
): AuthCacheService {
  return {
    getCachedRoles: vi.fn(async () => null),
    setCachedRoles: vi.fn(async () => {}),
    getCachedPermissions: vi.fn(async () => null),
    setCachedPermissions: vi.fn(async () => {}),
    deleteCachedRoles: vi.fn(async () => {}),
    deleteCachedPermissions: vi.fn(async () => {}),
    deleteAllCachedData: vi.fn(async () => {}),
    deleteByPrefix: vi.fn(async () => {}),
    getCachedOrganizationRoles: vi.fn(async () => null),
    setCachedOrganizationRoles: vi.fn(async () => {}),
    deleteCachedOrganizationRoles: vi.fn(async () => {}),
    ...overrides
  };
}

const params = { iamUrl: 'http://iam.local', hmacSecretKey: 'secret' };

describe('createSurfacePermissions', () => {
  beforeEach(() => {
    surfacePermissions.mockReset();
    surfaceRoles.mockReset();
  });

  it('returns an empty set and never touches the cache when sub is missing', async () => {
    const authCacheService = fakeAuthCacheService();
    const surface = createSurfacePermissions({ authCacheService, ...params });

    const result = await surface({});

    expect(result).toEqual(new Set());
    expect(authCacheService.getCachedPermissions).not.toHaveBeenCalled();
  });

  it('returns cached permissions without calling IAM on a cache hit', async () => {
    const cached = new Set(['biller:view_denials']);
    const authCacheService = fakeAuthCacheService({
      getCachedPermissions: vi.fn(async () => cached)
    });
    const surface = createSurfacePermissions({ authCacheService, ...params });

    const result = await surface({ sub: 'user-1' });

    expect(result).toBe(cached);
    expect(surfacePermissions).not.toHaveBeenCalled();
  });

  it('calls IAM and populates the cache on a cache miss', async () => {
    surfacePermissions.mockResolvedValue({
      code: 200,
      response: [{ slug: 'biller:view_denials' }]
    });
    const authCacheService = fakeAuthCacheService();
    const surface = createSurfacePermissions({ authCacheService, ...params });

    const result = await surface({ sub: 'user-1' });

    expect(result).toEqual(new Set(['biller:view_denials']));
    expect(surfacePermissions).toHaveBeenCalledTimes(1);
    expect(authCacheService.setCachedPermissions).toHaveBeenCalledWith(
      'user-1',
      new Set(['biller:view_denials'])
    );
  });

  it('does not write to the cache when the IAM call fails', async () => {
    surfacePermissions.mockRejectedValue(new Error('iam unavailable'));
    const authCacheService = fakeAuthCacheService();
    const surface = createSurfacePermissions({ authCacheService, ...params });

    const result = await surface({ sub: 'user-1' });

    expect(result).toEqual(new Set());
    expect(authCacheService.setCachedPermissions).not.toHaveBeenCalled();
  });
});

describe('createSurfaceRoles', () => {
  beforeEach(() => {
    surfacePermissions.mockReset();
    surfaceRoles.mockReset();
  });

  it('returns cached roles without calling IAM on a cache hit', async () => {
    const cached = new Set(['coder']);
    const authCacheService = fakeAuthCacheService({
      getCachedRoles: vi.fn(async () => cached)
    });
    const surface = createSurfaceRoles({ authCacheService, ...params });

    const result = await surface({ sub: 'user-1' });

    expect(result).toBe(cached);
    expect(surfaceRoles).not.toHaveBeenCalled();
  });

  it('calls IAM and populates the cache on a cache miss', async () => {
    surfaceRoles.mockResolvedValue({
      code: 200,
      response: [{ name: 'coder' }]
    });
    const authCacheService = fakeAuthCacheService();
    const surface = createSurfaceRoles({ authCacheService, ...params });

    const result = await surface({ sub: 'user-1' });

    expect(result).toEqual(new Set(['coder']));
    expect(authCacheService.setCachedRoles).toHaveBeenCalledWith(
      'user-1',
      new Set(['coder'])
    );
  });
});
