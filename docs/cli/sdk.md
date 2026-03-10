---
title: SDK Command
category: CLI Reference
description: Manage SDK generation modes and configuration.
---

## Overview

The `sdk` command controls how ForkLaunch generates internal SDKs for service-to-service communication. SDKs provide type-safe, idiomatic ways for services and workers to call each other.

## Usage

```bash
forklaunch sdk <subcommand> [options]
```

## Subcommands

### mode

Configure SDK generation mode for services.

```bash
forklaunch sdk mode <mode-type> [options]
```

**Mode Types:**
- `default` - Standard SDK generation with all features
- `minimal` - Lightweight SDKs with essential features only
- `custom` - Custom SDK configuration (advanced)

**Options:**
- `-p, --path <path>` - Path to application root (optional)
- `--service <name>` - Apply mode to specific service (optional)
- `--global` - Apply mode globally to all services

## SDK Generation Modes

### Default Mode

The default mode generates full-featured SDKs with:
- Complete type safety
- Automatic retry logic
- Built-in error handling
- Request/response logging
- OpenTelemetry tracing
- Automatic authentication

**Example:**
```bash
forklaunch sdk mode default --global
```

**Generated SDK Usage:**
```typescript
import { paymentsService } from '@/sdk';

// Full type safety and features
const result = await paymentsService.createCharge({
  amount: 1000,
  currency: 'usd',
  customerId: 'cus_123'
});
```

### Minimal Mode

Minimal mode generates lightweight SDKs optimized for:
- Reduced bundle size
- Lower memory footprint
- Faster cold starts
- Minimal dependencies

**Example:**
```bash
forklaunch sdk mode minimal --global
```

**Generated SDK Usage:**
```typescript
import { paymentsService } from '@/sdk';

// Same type safety, fewer features
const result = await paymentsService.createCharge({
  amount: 1000,
  currency: 'usd',
  customerId: 'cus_123'
});
```

### Custom Mode

Custom mode allows fine-grained control over SDK features.

**Example:**
```bash
forklaunch sdk mode custom --global
```

Then edit `.forklaunch/sdk.config.json`:

```json
{
  "mode": "custom",
  "features": {
    "retry": true,
    "tracing": true,
    "logging": false,
    "authentication": true
  },
  "bundleSize": "optimized",
  "typescript": {
    "strict": true
  }
}
```

## Per-Service Configuration

Configure SDK mode for specific services:

```bash
# Minimal mode for high-volume service
forklaunch sdk mode minimal --service analytics

# Default mode for critical service
forklaunch sdk mode default --service payments

# Check configuration
cat .forklaunch/sdk.config.json
```

**Example Configuration:**
```json
{
  "globalMode": "default",
  "services": {
    "analytics": {
      "mode": "minimal"
    },
    "payments": {
      "mode": "default"
    }
  }
}
```

## SDK Features Explained

### Type Safety

All SDK modes provide complete TypeScript type safety:

```typescript
// TypeScript knows the exact shape
const charge = await paymentsService.createCharge({
  amount: 1000,        // ✓ number required
  currency: 'usd',     // ✓ string required
  customerId: 'cus_123' // ✓ string required
});

// TypeScript error - missing field
const charge = await paymentsService.createCharge({
  amount: 1000
  // ❌ Error: currency required
});

// TypeScript knows response type
const customerId = charge.customerId; // ✓ string
const invalid = charge.nonExistent;   // ❌ Error: property doesn't exist
```

### Automatic Retry Logic (Default Mode)

```typescript
// Automatically retries on failure
try {
  const result = await paymentsService.createCharge({ ... });
} catch (error) {
  // Only throws after 3 retry attempts
  console.error('All retries exhausted', error);
}
```

### Built-in Tracing (Default Mode)

```typescript
// Automatically traces all SDK calls
const result = await paymentsService.createCharge({ ... });
// Appears in OpenTelemetry/Grafana with full context
```

### Error Handling

All modes provide structured error handling:

```typescript
import { ServiceError } from '@forklaunch/core';

try {
  const result = await paymentsService.createCharge({ ... });
} catch (error) {
  if (error instanceof ServiceError) {
    console.error('Service error:', error.statusCode, error.message);
  }
}
```

## Common Use Cases

### Optimize for Performance

For high-throughput services:

```bash
# Use minimal mode for event-processing service
forklaunch sdk mode minimal --service event-processor

# Rebuild to apply changes
pnpm build
```

### Optimize for Developer Experience

For complex business logic services:

```bash
# Use default mode for business services
forklaunch sdk mode default --service orders
forklaunch sdk mode default --service inventory

# Get full observability and debugging features
```

### Mixed Configuration

Different modes for different needs:

```bash
# Critical services get full features
forklaunch sdk mode default --service payments
forklaunch sdk mode default --service auth

# High-volume services get minimal overhead
forklaunch sdk mode minimal --service analytics
forklaunch sdk mode minimal --service logging

# Verify configuration
forklaunch sdk mode --show
```

## How SDKs Are Generated

SDKs are automatically generated from:

1. **API Schemas**: Zod/TypeBox schemas in your services
2. **Route Definitions**: HTTP route handlers
3. **Type Definitions**: TypeScript interfaces and types

**Example Service Code:**
```typescript
// src/modules/payments/api/routes/charge.routes.ts
export const createChargeRoute = forklaunch.post('/charges', {
  name: 'CreateCharge',
  body: {
    amount: z.number(),
    currency: z.string(),
    customerId: z.string()
  },
  responses: {
    200: chargeSchema
  }
}, async (req, res) => {
  // Implementation
});
```

**Generated SDK:**
```typescript
// Auto-generated: src/sdk/payments.sdk.ts
export const paymentsService = {
  createCharge: async (data: CreateChargeRequest): Promise<Charge> => {
    // Generated implementation with retries, tracing, etc.
  }
};
```

## SDK Usage Patterns

### Service-to-Service Calls

```typescript
// In orders service
import { paymentsService } from '@/sdk';

export async function createOrder(data: OrderData) {
  // Call payments service via SDK
  const charge = await paymentsService.createCharge({
    amount: data.total,
    currency: 'usd',
    customerId: data.customerId
  });

  // Create order with charge ID
  return orderRepository.create({
    ...data,
    chargeId: charge.id
  });
}
```

### Worker-to-Service Calls

```typescript
// In email worker
import { usersService } from '@/sdk';

export async function sendWelcomeEmail(userId: string) {
  // Fetch user details via SDK
  const user = await usersService.getUser(userId);

  // Send email
  await emailProvider.send({
    to: user.email,
    subject: 'Welcome!',
    body: `Hi ${user.name}!`
  });
}
```

## Regenerating SDKs

SDKs are automatically regenerated when you:

```bash
# Add or modify service routes
forklaunch sync service payments

# Sync entire workspace
forklaunch sync all

# Build for production (includes sync)
pnpm build
```

## Troubleshooting

### SDK Import Errors

```typescript
// ❌ Error: Cannot find module '@/sdk'
import { paymentsService } from '@/sdk';
```

**Solution**: Run sync to generate SDKs:
```bash
forklaunch sync all
```

### Type Mismatches

```typescript
// ❌ Error: Property 'customerId' is missing
const charge = await paymentsService.createCharge({
  amount: 1000,
  currency: 'usd'
});
```

**Solution**: Check service schema and update your call.

### SDK Not Updated

If changes to service aren't reflected in SDK:

```bash
# Force regeneration
forklaunch sync service payments

# Or sync all
forklaunch sync all
```

## Best Practices

1. **Use Global Default**: Start with default mode for all services
2. **Optimize Selectively**: Switch to minimal mode only when needed
3. **Consistent Configuration**: Keep SDK config in version control
4. **Regenerate Regularly**: Run sync after pulling code changes
5. **Type Everything**: Leverage full TypeScript type safety
6. **Handle Errors**: Always catch and handle SDK errors appropriately

## Performance Comparison

| Feature | Default Mode | Minimal Mode | Impact |
|---------|--------------|--------------|--------|
| Bundle Size | ~15KB per service | ~3KB per service | 80% reduction |
| Cold Start | ~50ms | ~10ms | 80% faster |
| Memory | ~2MB per service | ~500KB per service | 75% reduction |
| Retry Logic | ✓ Automatic | ✗ Manual | N/A |
| Tracing | ✓ Automatic | ✗ Manual | N/A |
| Logging | ✓ Automatic | ✗ Manual | N/A |

## Related Commands

- [`forklaunch sync`](/docs/cli/sync.md) - Regenerate SDKs
- [`forklaunch init service`](/docs/cli/init.md) - Create new service with SDK
- [`forklaunch openapi export`](/docs/cli/openapi.md) - Export API specifications

## See Also

- [Internal SDKs Guide](/docs/guides/internal-sdks.md)
- [Service Communication](/docs/guides/service-communication.md)
- [Universal SDK Framework](/docs/framework/universal-sdk.md)
