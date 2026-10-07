import type {
  BillingCacheService,
  SubscriptionCacheData
} from '@forklaunch/blueprint-core';
import { generateHmacAuthHeaders } from '@forklaunch/core/http';
import { universalSdk } from '@forklaunch/universal-sdk';
import type { JWTPayload } from 'jose';
import type { BillingSdkClient } from './sdk';

const sdkCache = new Map<string, BillingSdkClient>();

async function getBillingSdk(billingUrl: string): Promise<BillingSdkClient> {
  let sdk = sdkCache.get(billingUrl);
  if (!sdk) {
    sdk = await universalSdk<BillingSdkClient>({
      host: billingUrl,
      registryOptions: { path: 'api/v1/openapi' }
    });
    sdkCache.set(billingUrl, sdk);
  }
  return sdk;
}

/**
 * Create a surfaceSubscription function that fetches organization subscription data
 * from billing cache (populated by webhook events) or via SDK.
 */
export async function createSurfaceSubscription(params: {
  billingCacheService: BillingCacheService;
  billingUrl: string;
  hmacSecretKey: string;
}): Promise<
  (
    payload: JWTPayload & { organizationId?: string }
  ) => Promise<SubscriptionCacheData | null>
> {
  const { billingCacheService, billingUrl, hmacSecretKey } = params;
  const billingSdk = await getBillingSdk(billingUrl);

  return async (payload: JWTPayload & { organizationId?: string }) => {
    if (!payload.organizationId) {
      throw new Error('organizationId is required in JWT payload');
    }

    const cached = await billingCacheService.getCachedSubscription(
      payload.organizationId
    );
    if (cached) {
      return cached;
    }

    try {
      const headers = generateHmacAuthHeaders({
        secretKey: hmacSecretKey,
        method: 'GET',
        path: `/${payload.organizationId}/subscription`
      });

      const response =
        await billingSdk.subscription.getOrganizationSubscription({
          params: { id: payload.organizationId },
          headers
        });

      if (response.code !== 200 || !response.response) {
        return null;
      }

      const sub = response.response;

      // Fetch the plan to get features
      const planHeaders = generateHmacAuthHeaders({
        secretKey: hmacSecretKey,
        method: 'GET',
        path: `/${sub.productId}/plan`
      });
      const planResponse = await billingSdk.plan.getPlan({
        params: { id: sub.productId },
        headers: planHeaders
      });

      const plan =
        planResponse.code === 200 ? planResponse.response : undefined;

      const subscription: SubscriptionCacheData = {
        subscriptionId: sub.id,
        planId: sub.productId,
        planName: plan?.name ?? sub.description ?? '',
        status: sub.status,
        currentPeriodEnd: sub.endDate ?? sub.startDate,
        features: plan?.features ?? []
      };

      await billingCacheService.setCachedSubscription(
        payload.organizationId,
        subscription
      );
      return subscription;
    } catch (error) {
      console.error(
        '[surfaceSubscription] Error surfacing subscription:',
        error
      );
      return null;
    }
  };
}

/**
 * Create a surfaceFeatures function that fetches organization feature flags
 * from billing cache or via SDK.
 */
export async function createSurfaceFeatures(params: {
  billingCacheService: BillingCacheService;
  billingUrl: string;
  hmacSecretKey: string;
}): Promise<
  (payload: JWTPayload & { organizationId?: string }) => Promise<Set<string>>
> {
  const { billingCacheService, billingUrl, hmacSecretKey } = params;
  const billingSdk = await getBillingSdk(billingUrl);

  return async (payload: JWTPayload & { organizationId?: string }) => {
    if (!payload.organizationId) {
      throw new Error('organizationId is required in JWT payload');
    }

    const cached = await billingCacheService.getCachedFeatures(
      payload.organizationId
    );
    if (cached) {
      return cached;
    }

    try {
      const subHeaders = generateHmacAuthHeaders({
        secretKey: hmacSecretKey,
        method: 'GET',
        path: `/${payload.organizationId}/subscription`
      });

      const response =
        await billingSdk.subscription.getOrganizationSubscription({
          params: { id: payload.organizationId },
          headers: subHeaders
        });

      if (response.code !== 200 || !response.response) {
        return new Set<string>();
      }

      // Fetch the plan to get features
      const planHeaders = generateHmacAuthHeaders({
        secretKey: hmacSecretKey,
        method: 'GET',
        path: `/${response.response.productId}/plan`
      });
      const planResponse = await billingSdk.plan.getPlan({
        params: { id: response.response.productId },
        headers: planHeaders
      });

      const features = new Set<string>(
        planResponse.code === 200 ? (planResponse.response?.features ?? []) : []
      );

      await billingCacheService.setCachedFeatures(
        payload.organizationId,
        features
      );
      return features;
    } catch (error) {
      console.error('[surfaceFeatures] Error surfacing features:', error);
      return new Set<string>();
    }
  };
}

/**
 * Create a surfaceSubscription function that fetches subscription data
 * from local database via subscription service and caches results.
 */
export function createSurfaceSubscriptionLocally(params: {
  billingCacheService: BillingCacheService;
  subscriptionService: {
    getActiveSubscription: (params: {
      organizationId: string;
    }) => Promise<SubscriptionCacheData | null>;
  };
}): (payload: {
  organizationId?: string;
}) => Promise<SubscriptionCacheData | null> {
  const { billingCacheService, subscriptionService } = params;

  return async (payload: { organizationId?: string }) => {
    if (!payload.organizationId) {
      return null;
    }

    const cached = await billingCacheService.getCachedSubscription(
      payload.organizationId
    );
    if (cached) {
      return cached;
    }

    try {
      const subscription = await subscriptionService.getActiveSubscription({
        organizationId: payload.organizationId
      });

      if (subscription) {
        await billingCacheService.setCachedSubscription(
          payload.organizationId,
          subscription
        );
      }

      return subscription;
    } catch (error) {
      console.error('Failed to surface subscription locally:', error);
      return null;
    }
  };
}

/**
 * Create a surfaceFeatures function that fetches feature flags
 * from local database via subscription service and caches results.
 */
export function createSurfaceFeaturesLocally(params: {
  billingCacheService: BillingCacheService;
  subscriptionService: {
    getFeatures: (params: { organizationId: string }) => Promise<string[]>;
  };
}): (payload: { organizationId?: string }) => Promise<Set<string>> {
  const { billingCacheService, subscriptionService } = params;

  return async (payload: { organizationId?: string }) => {
    if (!payload.organizationId) {
      return new Set<string>();
    }

    const cached = await billingCacheService.getCachedFeatures(
      payload.organizationId
    );
    if (cached) {
      return cached;
    }

    try {
      const featuresArray = await subscriptionService.getFeatures({
        organizationId: payload.organizationId
      });
      const features = new Set<string>(featuresArray);
      await billingCacheService.setCachedFeatures(
        payload.organizationId,
        features
      );
      return features;
    } catch (error) {
      console.error('Failed to surface features locally:', error);
      return new Set<string>();
    }
  };
}

/**
 * Validates if organization has all required features.
 */
export function validateRequiredFeatures(
  requiredFeatures: string[],
  availableFeatures: Set<string>
): { allowed: boolean; missingFeatures: string[] } {
  const missingFeatures = requiredFeatures.filter(
    (feature) => !availableFeatures.has(feature)
  );

  return {
    allowed: missingFeatures.length === 0,
    missingFeatures
  };
}

/**
 * Validates if organization has an active subscription.
 */
export function validateActiveSubscription(
  subscription: SubscriptionCacheData | null
): {
  allowed: boolean;
  reason?: 'NO_SUBSCRIPTION' | 'INACTIVE';
} {
  if (!subscription) {
    return { allowed: false, reason: 'NO_SUBSCRIPTION' };
  }

  if (subscription.status !== 'active' && subscription.status !== 'trialing') {
    return { allowed: false, reason: 'INACTIVE' };
  }

  return { allowed: true };
}
