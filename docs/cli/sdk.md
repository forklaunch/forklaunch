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

Configure SDK generation mode.

```bash
forklaunch sdk mode [options]
```

**Options:**
- `-t, --type <mode>` - SDK mode type: `generated` or `live`
- `-p, --path <path>` - Path to application root (optional)
- `-n, --dryrun` - Show what would be changed without making changes

If `--type` is not specified, the CLI will prompt for mode selection interactively.

## SDK Generation Modes

### Generated Mode

The `generated` mode creates pre-built SDK clients from your service definitions:

```bash
forklaunch sdk mode --type generated
```

**Generated SDK Usage:**
```typescript
import { paymentsService } from '@/sdk';

// Full type safety
const result = await paymentsService.createCharge({
  amount: 1000,
  currency: 'usd',
  customerId: 'cus_123'
});
```

### Live Mode

The `live` mode generates SDK clients that call services directly at runtime:

```bash
forklaunch sdk mode --type live
```

**Live SDK Usage:**
```typescript
import { paymentsService } from '@/sdk';

// Same type-safe interface, live calls
const result = await paymentsService.createCharge({
  amount: 1000,
  currency: 'usd',
  customerId: 'cus_123'
});
```

## SDK Features

### Type Safety

All SDK modes provide complete TypeScript type safety:

```typescript
// TypeScript knows the exact shape
const charge = await paymentsService.createCharge({
  amount: 1000,        // number required
  currency: 'usd',     // string required
  customerId: 'cus_123' // string required
});

// TypeScript knows response type
const customerId = charge.customerId; // string
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
    // Generated implementation
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
// Error: Cannot find module '@/sdk'
import { paymentsService } from '@/sdk';
```

**Solution**: Run sync to generate SDKs:
```bash
forklaunch sync all
```

### Type Mismatches

```typescript
// Error: Property 'customerId' is missing
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

1. **Start with Generated**: Use `generated` mode as a starting point
2. **Regenerate Regularly**: Run sync after pulling code changes
3. **Type Everything**: Leverage full TypeScript type safety
4. **Handle Errors**: Always catch and handle SDK errors appropriately

## Related Commands

- [`forklaunch sync`](/docs/cli/sync.md) - Regenerate SDKs
- [`forklaunch init service`](/docs/cli/init.md) - Create new service with SDK
- [`forklaunch openapi export`](/docs/cli/openapi.md) - Export API specifications

## See Also

- [Service Communication](/docs/guides/service-communication.md)
- [Universal SDK Framework](/docs/framework/universal-sdk.md)
