/**
 * Auth Cache Service
 * Redis-based caching for authentication data (roles, permissions)
 */

import type { CacheLike } from '../billing/cache';

export type { CacheLike };

// Cache key prefixes
const ROLES_CACHE_PREFIX = 'auth:roles:';
const PERMISSIONS_CACHE_PREFIX = 'auth:permissions:';
const ORG_ROLES_CACHE_PREFIX = 'auth:org-roles:';

export interface AuthCacheService {
  getCachedRoles(userId: string): Promise<Set<string> | null>;
  setCachedRoles(userId: string, roles: Set<string>): Promise<void>;
  getCachedPermissions(userId: string): Promise<Set<string> | null>;
  setCachedPermissions(userId: string, permissions: Set<string>): Promise<void>;
  deleteCachedRoles(userId: string): Promise<void>;
  deleteCachedPermissions(userId: string): Promise<void>;
  deleteAllCachedData(userId: string): Promise<void>;
  deleteByPrefix(prefix: string): Promise<void>;
  getCachedOrganizationRoles(organizationId: string): Promise<string[] | null>;
  setCachedOrganizationRoles(
    organizationId: string,
    roles: string[]
  ): Promise<void>;
  deleteCachedOrganizationRoles(organizationId: string): Promise<void>;
}

export function createAuthCacheService(cache: CacheLike): AuthCacheService {
  const TTL = 5 * 60 * 1000; // 5 minutes

  return {
    async getCachedRoles(userId: string) {
      try {
        const result = await cache.readRecord<string[]>(
          `${ROLES_CACHE_PREFIX}${userId}`
        );
        return new Set(result.value);
      } catch {
        return null;
      }
    },

    async setCachedRoles(userId: string, roles: Set<string>) {
      try {
        await cache.putRecord({
          key: `${ROLES_CACHE_PREFIX}${userId}`,
          value: Array.from(roles),
          ttlMilliseconds: TTL
        });
      } catch {
        // Silently fail - cache is not critical
      }
    },

    async getCachedPermissions(userId: string) {
      try {
        const result = await cache.readRecord<string[]>(
          `${PERMISSIONS_CACHE_PREFIX}${userId}`
        );
        return new Set(result.value);
      } catch {
        return null;
      }
    },

    async setCachedPermissions(userId: string, permissions: Set<string>) {
      try {
        await cache.putRecord({
          key: `${PERMISSIONS_CACHE_PREFIX}${userId}`,
          value: Array.from(permissions),
          ttlMilliseconds: TTL
        });
      } catch {
        // Silently fail - cache is not critical
      }
    },

    async deleteCachedRoles(userId: string) {
      try {
        await cache.deleteRecord(`${ROLES_CACHE_PREFIX}${userId}`);
      } catch {
        // Silently fail
      }
    },

    async deleteCachedPermissions(userId: string) {
      try {
        await cache.deleteRecord(`${PERMISSIONS_CACHE_PREFIX}${userId}`);
      } catch {
        // Silently fail
      }
    },

    async deleteAllCachedData(userId: string) {
      await this.deleteCachedRoles(userId);
      await this.deleteCachedPermissions(userId);
    },

    async deleteByPrefix(prefix: string) {
      try {
        const keys = await cache.listKeys(prefix);
        if (keys.length > 0) {
          await cache.deleteBatchRecords(keys);
        }
      } catch {
        // Silently fail
      }
    },

    async getCachedOrganizationRoles(organizationId: string) {
      try {
        const result = await cache.readRecord<string[]>(
          `${ORG_ROLES_CACHE_PREFIX}${organizationId}`
        );
        return result.value;
      } catch {
        return null;
      }
    },

    async setCachedOrganizationRoles(organizationId: string, roles: string[]) {
      try {
        await cache.putRecord({
          key: `${ORG_ROLES_CACHE_PREFIX}${organizationId}`,
          value: roles,
          ttlMilliseconds: TTL
        });
      } catch {
        // Silently fail
      }
    },

    async deleteCachedOrganizationRoles(organizationId: string) {
      try {
        await cache.deleteRecord(`${ORG_ROLES_CACHE_PREFIX}${organizationId}`);
      } catch {
        // Silently fail
      }
    }
  };
}

export const AUTH_CACHE_KEYS = {
  ROLES_PREFIX: ROLES_CACHE_PREFIX,
  PERMISSIONS_PREFIX: PERMISSIONS_CACHE_PREFIX,
  ORG_ROLES_PREFIX: ORG_ROLES_CACHE_PREFIX
};
