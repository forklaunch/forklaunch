/**
 * Billing Feature Flags
 *
 * Define your application's billing-based feature flags here.
 * Feature flags control access to functionality based on subscription plans.
 *
 * This file provides the structure and utilities for managing feature entitlements.
 * Customize the FEATURE_FLAGS and PLAN_FEATURES objects for your application.
 */

/**
 * Feature Flags
 * Define all available feature flags for your application.
 *
 * Example:
 * ```typescript
 * export const FEATURE_FLAGS = {
 *   PREMIUM_SUPPORT: 'premium_support',
 *   ADVANCED_ANALYTICS: 'advanced_analytics',
 *   CUSTOM_BRANDING: 'custom_branding',
 *   API_ACCESS: 'api_access',
 *   UNLIMITED_USERS: 'unlimited_users',
 * } as const;
 * ```
 */
export const FEATURE_FLAGS = {
  // Add your billing-based feature flags here
  // Each feature should have a unique slug (lowercase with underscores)
} as const;

export type FeatureFlag = (typeof FEATURE_FLAGS)[keyof typeof FEATURE_FLAGS];

/**
 * Plan-to-Feature Mapping
 * Define which features are available in each billing plan tier.
 *
 * Example:
 * ```typescript
 * export const PLAN_FEATURES: Record<string, FeatureFlag[]> = {
 *   free: [],
 *   starter: [FEATURE_FLAGS.API_ACCESS],
 *   pro: [FEATURE_FLAGS.API_ACCESS, FEATURE_FLAGS.ADVANCED_ANALYTICS],
 *   enterprise: Object.values(FEATURE_FLAGS),
 * };
 * ```
 */
export const PLAN_FEATURES: Record<string, FeatureFlag[]> = {
  free: [],
  pro: [],
  enterprise: []
};

/**
 * Resource Limits by Plan
 * Define hard resource limits for each billing plan tier.
 * Use -1 to indicate unlimited.
 *
 * Customize the ResourceLimits type for your application's needs.
 */
export type ResourceLimits = {
  maxEnvironments?: number;
  maxServices?: number;
  maxWorkers?: number;
  maxMonthlyDeployments?: number;
};

export const PLAN_LIMITS: Record<string, ResourceLimits> = {
  free: {},
  pro: {},
  enterprise: {}
};

/**
 * Feature Sets for Common Use Cases
 * Group related features for easier authorization checks.
 *
 * Example:
 * ```typescript
 * export const PREMIUM_FEATURES = new Set([
 *   FEATURE_FLAGS.PREMIUM_SUPPORT,
 *   FEATURE_FLAGS.ADVANCED_ANALYTICS,
 * ]);
 * ```
 */

/**
 * Helper Functions
 */

/**
 * Get features for a specific plan
 * @param planName - Name of the plan (e.g., 'free', 'pro', 'enterprise')
 * @returns Array of feature flag slugs available in the plan
 */
export function getFeaturesForPlan(planName: string): FeatureFlag[] {
  const normalizedPlanName = planName.toLowerCase();
  return PLAN_FEATURES[normalizedPlanName] || PLAN_FEATURES.free || [];
}

/**
 * Get resource limits for a specific plan
 * @param planName - Name of the plan (e.g., 'free', 'pro', 'enterprise')
 * @returns Resource limit configuration for the plan
 */
export function getLimitsForPlan(planName: string): ResourceLimits {
  const normalizedPlanName = planName.toLowerCase();
  return PLAN_LIMITS[normalizedPlanName] || PLAN_LIMITS.free || {};
}

/**
 * Check if a feature is available in a specific plan
 * @param planName - Name of the plan
 * @param featureSlug - Feature flag slug to check
 * @returns True if the feature is available in the plan
 */
export function isPlanFeatureAvailable(
  planName: string,
  featureSlug: string
): boolean {
  const features = getFeaturesForPlan(planName);
  return features.includes(featureSlug as FeatureFlag);
}

/**
 * Convert feature set to array for auth config
 * @param features - Set of feature flags
 * @returns Array of feature flag slugs
 */
export function featureSetToArray(features: Set<string>): string[] {
  return Array.from(features);
}

/**
 * Check if required features are available
 * @param requiredFeatures - Array of required feature slugs
 * @param availableFeatures - Set of available feature slugs
 * @returns True if all required features are available
 */
export function hasRequiredFeatures(
  requiredFeatures: string[],
  availableFeatures: Set<string>
): boolean {
  return requiredFeatures.every((feature) => availableFeatures.has(feature));
}

/**
 * Get missing features
 * @param requiredFeatures - Array of required feature slugs
 * @param availableFeatures - Set of available feature slugs
 * @returns Array of missing feature slugs
 */
export function getMissingFeatures(
  requiredFeatures: string[],
  availableFeatures: Set<string>
): string[] {
  return requiredFeatures.filter((feature) => !availableFeatures.has(feature));
}
