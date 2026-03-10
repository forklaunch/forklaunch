---
title: IAM and Billing Utilities
category: Guides
description: Complete guide for using IAM and Billing utilities in your ForkLaunch application.
---

## Overview

ForkLaunch provides built-in authorization through the ContractDetails auth property, combining authentication methods with access control strategies. This guide covers how to use IAM and Billing utilities to implement role-based access control (RBAC) and feature-based entitlements.

## Quick Start

### 1. Import What You Need

```typescript
// All IAM utilities (RBAC, cache, surfacing)
import {
  // RBAC constants (roles, permissions)
  PLATFORM_ADMIN_ROLES,
  PERMISSIONS,
  ROLE_PERMISSIONS,
  // Cache services
  createAuthCacheService,
  // Surfacing functions
  createSurfaceRoles,
  createSurfacePermissions
} from '@{your-app-name}/iam/utils';

// All Billing utilities (cache, surfacing, feature flags)
import {
  // Cache services
  createBillingCacheService,
  // Surfacing functions
  createSurfaceSubscription,
  createSurfaceFeatures,
  // Feature flags
  FEATURE_FLAGS,
  getFeaturesForPlan
} from '@{your-app-name}/billing/utils';
```

### 2. Configure Server Auth

```typescript
import { forklaunchExpress } from '@forklaunch/express';
import {
  createSurfaceRoles,
  createSurfacePermissions,
  createAuthCacheService
} from '@{your-app-name}/iam/utils';
import {
  createSurfaceFeatures,
  createBillingCacheService
} from '@{your-app-name}/billing/utils';

// Create cache services
const authCacheService = createAuthCacheService(redisCache);
const billingCacheService = createBillingCacheService(redisCache);

// Create surfacing functions
const surfaceRoles = await createSurfaceRoles({
  authCacheService,
  iamUrl: process.env.IAM_URL,
  hmacSecretKey: process.env.HMAC_SECRET
});

const surfacePermissions = await createSurfacePermissions({
  authCacheService,
  iamUrl: process.env.IAM_URL,
  hmacSecretKey: process.env.HMAC_SECRET
});

const surfaceFeatures = await createSurfaceFeatures({
  billingCacheService,
  billingUrl: process.env.BILLING_URL
});

// Configure server with auth
const app = forklaunchExpress(schemaValidator, telemetry, {
  auth: {
    mapRoles: surfaceRoles,
    mapPermissions: surfacePermissions,
    mapFeatures: surfaceFeatures
  }
});
```

### 3. Protect Routes

```typescript
import { handlers, schemaValidator } from '@{your-app-name}/core';
import { PLATFORM_ADMIN_ROLES } from '@{your-app-name}/iam/utils';
import { FEATURE_FLAGS } from '@{your-app-name}/billing/utils';

export const protectedRoute = handlers.get(
  schemaValidator,
  '/admin/dashboard',
  {
    name: 'getAdminDashboard',
    auth: {
      method: 'jwt',
      allowedRoles: PLATFORM_ADMIN_ROLES,
      requiredFeatures: [FEATURE_FLAGS.ADVANCED_ANALYTICS]
    },
    responses: { 200: DashboardSchema }
  },
  async (req, res) => {
    // Only admins with ADVANCED_ANALYTICS feature can access
  }
);
```

## IAM Module Utilities

The IAM module exports all utilities through a single `/utils` entry point:

```typescript
import {
  // Role definitions (from core RBAC)
  ROLES,                      // { VIEWER, EDITOR, ADMIN, SYSTEM }
  PLATFORM_VIEWER_ROLES,      // Set of all roles (viewer+)
  PLATFORM_EDITOR_ROLES,      // Set of editor+ roles
  PLATFORM_ADMIN_ROLES,       // Set of admin+ roles
  PLATFORM_SYSTEM_ROLES,      // Set of system roles only

  // Permission definitions (from core RBAC)
  PERMISSIONS,                // { PLATFORM_READ, PLATFORM_WRITE }
  PLATFORM_READ_PERMISSIONS,  // Set of read permissions
  PLATFORM_WRITE_PERMISSIONS, // Set of write permissions

  // Role-permission mapping (from core RBAC)
  ROLE_PERMISSIONS,           // Record<ROLES, PERMISSIONS[]>

  // Cache service
  createAuthCacheService,
  type AuthCacheService,
  type CacheLike,

  // Surfacing functions
  createSurfaceRoles,
  createSurfaceRolesLocally,
  createSurfacePermissions,
  createSurfacePermissionsLocally,
  generateHmacAuthHeaders,

  // Auth session schema
  SHARED_SESSION_SCHEMA,
  type BetterAuthConfig,

  // Organization provisioning
  OrganizationProvisioningService,

  // Domain types and schemas
  OrganizationUserRoleEnum,
  OrganizationManagementSchemas,
  type ProjectPermission,
  type UserWithPermissions
} from '@{your-app-name}/iam/utils';
```

**Usage in Controllers:**
```typescript
// Require admin role
auth: { allowedRoles: PLATFORM_ADMIN_ROLES }

// Require write permission
auth: { allowedPermissions: PLATFORM_WRITE_PERMISSIONS }

// Check specific role
if (PLATFORM_ADMIN_ROLES.has(user.role)) { ... }
```

### Auth Cache Service

```typescript
import { createAuthCacheService, type AuthCacheService, type CacheLike } from '@{your-app-name}/iam/utils';

const authCache = createAuthCacheService(redisCache);

// Methods available:
await authCache.getCachedRoles(userId);           // Get cached roles
await authCache.setCachedRoles(userId, roles);    // Cache roles
await authCache.getCachedPermissions(userId);     // Get cached permissions
await authCache.setCachedPermissions(userId, permissions);
await authCache.deleteAllCachedData(userId);      // Clear user cache
```

### Role/Permission Surfacing

**Remote Surfacing (calls IAM service via SDK):**
```typescript
import { createSurfaceRoles, createSurfacePermissions } from '@{your-app-name}/iam/utils';

const surfaceRoles = await createSurfaceRoles({
  authCacheService,
  iamUrl: 'http://iam-service:3000',
  hmacSecretKey: process.env.HMAC_SECRET
});

// Returns function: (payload: { sub?: string }) => Promise<Set<string>>
const roles = await surfaceRoles({ sub: userId });
```

**Local Surfacing (direct database access):**
```typescript
import { createSurfaceRolesLocally, createSurfacePermissionsLocally } from '@{your-app-name}/iam/utils';

const surfaceRoles = createSurfaceRolesLocally({
  authCacheService,
  userService // Must have surfaceRoles({ id }) method
});
```

## Billing Module Utilities

The Billing module exports all utilities through a single `/utils` entry point:

```typescript
import {
  // Feature flags (from core)
  FEATURE_FLAGS,           // Define your feature flags here
  PLAN_FEATURES,           // Map of plan -> features
  PLAN_LIMITS,             // Map of plan -> resource limits
  ENTERPRISE_FEATURES,     // Set of enterprise-only features
  PRO_FEATURES,            // Set of pro-tier features
  getFeaturesForPlan,      // Get features for a plan name
  getLimitsForPlan,        // Get limits for a plan name
  isPlanFeatureAvailable,  // Check if plan has feature
  hasRequiredFeatures,     // Check if all features present
  getMissingFeatures,      // Get missing feature list
  featureSetToArray,       // Convert feature set to array
  type ResourceLimits,     // Resource limit types

  // Cache service
  BILLING_CACHE_KEYS,
  createBillingCacheService,
  type BillingCacheLike,
  type BillingCacheService,
  type PlanCacheData,
  type SubscriptionCacheData,

  // Surfacing functions
  createSurfaceFeatures,
  createSurfaceSubscription,
  validateActiveSubscription,
  validateRequiredFeatures,
  type SubscriptionData,

  // Plan enum (from core)
  BillingPlanEnum,
  type BillingPlanEnumType,

  // Billing schemas
  BillingSchemas,

  // Billing types
  type BillingInfo,
  type BillingPlan,
  type PlanDetails,
  type ProjectSummary
} from '@{your-app-name}/billing/utils';
```

**Defining Features:**
```typescript
// In feature-flags.ts
export const FEATURE_FLAGS = {
  ADVANCED_ANALYTICS: 'advanced_analytics',
  CUSTOM_BRANDING: 'custom_branding',
  API_ACCESS: 'api_access'
} as const;

export const PLAN_FEATURES = {
  free: [],
  pro: [FEATURE_FLAGS.ADVANCED_ANALYTICS, FEATURE_FLAGS.API_ACCESS],
  enterprise: Object.values(FEATURE_FLAGS)
};
```

### Billing Cache Service

```typescript
import { createBillingCacheService, type BillingCacheService } from '@{your-app-name}/billing/utils';

const billingCache = createBillingCacheService(redisCache);

// Subscription caching
await billingCache.getCachedSubscription(orgId);
await billingCache.setCachedSubscription(orgId, subscriptionData);

// Feature caching
await billingCache.getCachedFeatures(orgId);
await billingCache.setCachedFeatures(orgId, features);

// Entitlement caching
await billingCache.getCachedEntitlements(partyKey);
await billingCache.setCachedEntitlements(partyKey, entitlementData);
```

### Subscription/Feature Surfacing

**Remote Surfacing (from cache, populated by webhooks):**
```typescript
import { createSurfaceSubscription, createSurfaceFeatures } from '@{your-app-name}/billing/utils';

const surfaceSubscription = await createSurfaceSubscription({
  billingCacheService,
  billingUrl: 'http://billing-service:3000'
});

const surfaceFeatures = await createSurfaceFeatures({
  billingCacheService,
  billingUrl: 'http://billing-service:3000'
});
```

**Local Surfacing (direct database access):**
```typescript
import { createSurfaceSubscriptionLocally, createSurfaceFeaturesLocally } from '@{your-app-name}/billing/utils';

const surfaceSubscription = createSurfaceSubscriptionLocally({
  billingCacheService,
  subscriptionService // Must have getActiveSubscription({ organizationId }) method
});
```

**Validation Helpers:**
```typescript
import { validateRequiredFeatures, validateActiveSubscription } from '@{your-app-name}/billing/utils';

const featureCheck = validateRequiredFeatures(['api_access'], userFeatures);
// { allowed: boolean, missingFeatures: string[] }

const subscriptionCheck = validateActiveSubscription(subscription);
// { allowed: boolean, reason?: 'NO_SUBSCRIPTION' | 'INACTIVE' }
```

## Authorization Methods

### JWT Authentication
```typescript
auth: {
  method: 'jwt',
  allowedRoles: PLATFORM_ADMIN_ROLES
}
```

### Basic Authentication
```typescript
auth: {
  method: 'basic',
  login: (username, password) => validateCredentials(username, password)
}
```

### HMAC Authentication (Service-to-Service)
```typescript
import { generateHmacAuthHeaders } from '@{your-app-name}/iam/utils';

const headers = generateHmacAuthHeaders({
  secretKey: process.env.HMAC_SECRET,
  method: 'GET',
  path: '/users/123/roles'
});
```

## Stripe Integration

### Feature Sync from Stripe Products

Features are automatically synced from Stripe product metadata to your Plan entities:

1. **Set features in Stripe Dashboard:**
   - Go to Products → Select Product → Metadata
   - Add key: `features`
   - Add value: `"feature1,feature2,feature3"` (comma-separated)
   - Or: `'["feature1","feature2","feature3"]'` (JSON array)

2. **Webhook events that trigger sync:**
   - `product.created` / `product.updated` - Syncs features to all associated plans
   - `plan.created` / `plan.updated` - Fetches product and syncs features
   - `price.created` / `price.updated` - Same as plan events

3. **Cached subscription data includes:**
   - `planName` - Name from Stripe product
   - `status` - Subscription status (active, trialing, etc.)
   - `features` - Array of feature slugs from product metadata

## Error Responses

| Status | Message | Cause |
|--------|---------|-------|
| 401 | No Authorization token provided | Missing auth header |
| 401 | Invalid Authorization token format | Wrong token format |
| 403 | Invalid Authorization subject | JWT missing subject |
| 403 | Invalid Authorization permissions | Permission check failed |
| 403 | Invalid Authorization roles | Role check failed |
| 403 | Missing required features | Feature entitlement failed |
| 403 | No active subscription | Subscription check failed |

## Best Practices

1. **Use Role Sets, Not Individual Roles**
   ```typescript
   // ✅ Good - uses hierarchical set
   allowedRoles: PLATFORM_ADMIN_ROLES
   
   // ❌ Avoid - bypasses hierarchy
   allowedRoles: new Set(['admin'])
   ```

2. **Cache Everything**
   ```typescript
   // Surfacing functions handle caching automatically
   const roles = await surfaceRoles({ sub: userId });
   // First call: fetches from IAM service, caches result
   // Subsequent calls: returns from cache
   ```

3. **Use Local Surfacing Within Same Service**
   ```typescript
   // In IAM service itself, use local surfacing
   const surfaceRoles = createSurfaceRolesLocally({
     authCacheService,
     userService: iamUserService
   });
   
   // In other services, use remote surfacing
   const surfaceRoles = await createSurfaceRoles({
     authCacheService,
     iamUrl: process.env.IAM_URL,
     hmacSecretKey: process.env.HMAC_SECRET
   });
   ```

4. **Define Features as Constants**
   ```typescript
   // ✅ Good - type-safe, refactorable
   requiredFeatures: [FEATURE_FLAGS.ADVANCED_ANALYTICS]
   
   // ❌ Avoid - typo-prone, no type checking
   requiredFeatures: ['advnaced_analytics']
   ```

## Related Documentation

- **[HTTP Frameworks](/docs/framework/http.md)** - ContractDetails and route configuration
- **[Validation](/docs/framework/validation.md)** - Input validation and schema definitions
- **[Error Handling](/docs/framework/error-handling.md)** - Authentication and permission error handling
- **[Telemetry](/docs/framework/telemetry.md)** - Authorization event logging and tracing
