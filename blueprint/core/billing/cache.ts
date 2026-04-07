/**
 * Billing Cache Service
 * Redis-based caching for billing data (subscriptions, features, plans)
 */

import type { ResourceLimits } from './feature-flags';

import type { TtlCache } from '@forklaunch/core/cache';

// Billing cache key prefixes
const SUBSCRIPTION_CACHE_PREFIX = 'billing:subscription:';
const FEATURES_CACHE_PREFIX = 'billing:features:';
const PLAN_CACHE_PREFIX = 'billing:plan:';

export type SubscriptionCacheData = {
  subscriptionId: string;
  planId: string;
  planName: string;
  status: string;
  currentPeriodEnd: Date;
  features: string[];
};

export type PlanCacheData = {
  id: string;
  name: string;
  features: string[];
};

export type EntitlementCacheData = {
  features: string[];
  limits: ResourceLimits;
  syncedAt: Date;
  source: 'db' | 'external';
};

export type BillingCacheLike = TtlCache;

export type BillingCacheService = {
  getCachedSubscription: (
    organizationId: string
  ) => Promise<SubscriptionCacheData | null>;
  setCachedSubscription: (
    organizationId: string,
    data: SubscriptionCacheData
  ) => Promise<void>;
  deleteCachedSubscription: (organizationId: string) => Promise<void>;
  getCachedPlan: (
    organizationId: string,
    planId: string
  ) => Promise<PlanCacheData | null>;
  setCachedPlan: (
    organizationId: string,
    planId: string,
    data: PlanCacheData
  ) => Promise<void>;
  getCachedFeatures: (organizationId: string) => Promise<Set<string> | null>;
  setCachedFeatures: (
    organizationId: string,
    features: Set<string>
  ) => Promise<void>;
  getCachedEntitlements: (
    partyKey: string
  ) => Promise<EntitlementCacheData | null>;
  setCachedEntitlements: (
    partyKey: string,
    data: EntitlementCacheData
  ) => Promise<void>;
  deleteCachedEntitlements: (partyKey: string) => Promise<void>;
};

export function createBillingCacheService(
  cache: BillingCacheLike
): BillingCacheService {
  const TTL = 5 * 60 * 1000; // 5 minutes
  const PLAN_TTL = 15 * 60 * 1000; // 15 minutes

  return {
    async getCachedSubscription(organizationId: string) {
      try {
        const result = await cache.readRecord<SubscriptionCacheData>(
          `${SUBSCRIPTION_CACHE_PREFIX}${organizationId}`,
          { tenantId: organizationId }
        );
        if (!result.value || typeof result.value !== 'object') {
          return null;
        }
        const subscription = result.value;
        if (typeof subscription.currentPeriodEnd === 'string') {
          subscription.currentPeriodEnd = new Date(
            subscription.currentPeriodEnd
          );
        }
        return subscription;
      } catch {
        return null;
      }
    },
    async setCachedSubscription(
      organizationId: string,
      data: SubscriptionCacheData
    ) {
      try {
        await cache.putRecord(
          {
            key: `${SUBSCRIPTION_CACHE_PREFIX}${organizationId}`,
            value: data,
            ttlMilliseconds: TTL
          },
          { tenantId: organizationId }
        );
      } catch {
        // Silently fail - cache is not critical
      }
    },
    async deleteCachedSubscription(organizationId: string) {
      try {
        await cache.deleteRecord(
          `${SUBSCRIPTION_CACHE_PREFIX}${organizationId}`
        );
      } catch {
        // Silently fail
      }
    },
    async getCachedPlan(organizationId: string, planId: string) {
      try {
        const result = await cache.readRecord<PlanCacheData>(
          `${PLAN_CACHE_PREFIX}${planId}`,
          { tenantId: organizationId }
        );
        return result.value;
      } catch {
        return null;
      }
    },
    async setCachedPlan(
      organizationId: string,
      planId: string,
      data: PlanCacheData
    ) {
      try {
        await cache.putRecord(
          {
            key: `${PLAN_CACHE_PREFIX}${planId}`,
            value: data,
            ttlMilliseconds: PLAN_TTL
          },
          { tenantId: organizationId }
        );
      } catch {
        // Silently fail
      }
    },
    async getCachedFeatures(organizationId: string) {
      try {
        const result = await cache.readRecord<string[]>(
          `${FEATURES_CACHE_PREFIX}${organizationId}`,
          { tenantId: organizationId }
        );
        if (Array.isArray(result.value)) {
          return new Set<string>(result.value);
        }
        return null;
      } catch {
        return null;
      }
    },
    async setCachedFeatures(organizationId: string, features: Set<string>) {
      try {
        await cache.putRecord(
          {
            key: `${FEATURES_CACHE_PREFIX}${organizationId}`,
            value: Array.from(features),
            ttlMilliseconds: TTL
          },
          { tenantId: organizationId }
        );
      } catch {
        // Silently fail
      }
    },
    async getCachedEntitlements(partyKey: string) {
      try {
        const result = await cache.readRecord<EntitlementCacheData>(
          `${ENTITLEMENT_PREFIX}${partyKey}`,
          { tenantId: partyKey }
        );
        const entitlement = result.value;
        if (typeof entitlement.syncedAt === 'string') {
          entitlement.syncedAt = new Date(entitlement.syncedAt);
        }
        return entitlement;
      } catch {
        return null;
      }
    },
    async setCachedEntitlements(partyKey: string, data: EntitlementCacheData) {
      try {
        await cache.putRecord(
          {
            key: `${ENTITLEMENT_PREFIX}${partyKey}`,
            value: data,
            ttlMilliseconds: TTL
          },
          { tenantId: partyKey }
        );
      } catch {
        // Silently fail
      }
    },
    async deleteCachedEntitlements(partyKey: string) {
      try {
        await cache.deleteRecord(`${ENTITLEMENT_PREFIX}${partyKey}`);
      } catch {
        // Silently fail
      }
    }
  };
}

const ENTITLEMENT_PREFIX = 'billing:entitlement:';

export const BILLING_CACHE_KEYS = {
  SUBSCRIPTION_PREFIX: SUBSCRIPTION_CACHE_PREFIX,
  FEATURES_PREFIX: FEATURES_CACHE_PREFIX,
  PLAN_PREFIX: PLAN_CACHE_PREFIX,
  ENTITLEMENT_PREFIX
};
