---
title: OpenAPI Command
category: CLI Reference
description: Export OpenAPI specifications for your services.
---

## Overview

The `openapi` command exports OpenAPI 3.0 specifications from your ForkLaunch services, enabling API documentation, client SDK generation, and integration with API tools.

## Usage

```bash
forklaunch openapi <subcommand> [options]
```

## Subcommands

### export

Export OpenAPI specifications for all services.

```bash
forklaunch openapi export [options]
```

**Options:**
- `-p, --path <path>` - Path to application root (optional)
- `-o, --output <dir>` - Output directory (default: `.forklaunch/openapi`)

## Basic Usage

### Export All Services

```bash
$ forklaunch openapi export

[INFO] Exporting OpenAPI specifications...
[OK] Exported users → .forklaunch/openapi/users.json
[OK] Exported payments → .forklaunch/openapi/payments.json
[OK] Exported notifications → .forklaunch/openapi/notifications.json
[OK] Exported 3 services successfully
```

### Custom Output Directory

```bash
$ forklaunch openapi export --output ./docs/api

[INFO] Exporting OpenAPI specifications...
[OK] Exported users → docs/api/users.json
[OK] Exported payments → docs/api/payments.json
```

## OpenAPI Specification Structure

ForkLaunch automatically generates complete OpenAPI 3.0 specifications from your service code.

### Example Service Code

```typescript
// src/modules/payments/api/routes/charge.routes.ts
import { z } from 'zod';
import { forklaunch } from '@forklaunch/core';

const chargeSchema = z.object({
  id: z.string(),
  amount: z.number(),
  currency: z.string(),
  status: z.enum(['pending', 'succeeded', 'failed'])
});

export const createChargeRoute = forklaunch.post('/charges', {
  name: 'CreateCharge',
  summary: 'Create a new payment charge',
  description: 'Creates a charge for the specified amount',
  body: {
    amount: z.number().describe('Amount in cents'),
    currency: z.string().describe('Three-letter ISO currency code'),
    customerId: z.string().describe('Customer ID')
  },
  responses: {
    200: chargeSchema,
    400: { error: z.string() },
    500: { error: z.string() }
  }
}, async (req, res) => {
  // Implementation
});
```

### Generated OpenAPI Spec

```yaml
openapi: 3.0.0
info:
  title: Payments Service API
  version: 1.0.0
  description: Payment processing service

paths:
  /charges:
    post:
      operationId: CreateCharge
      summary: Create a new payment charge
      description: Creates a charge for the specified amount
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                amount:
                  type: number
                  description: Amount in cents
                currency:
                  type: string
                  description: Three-letter ISO currency code
                customerId:
                  type: string
                  description: Customer ID
              required:
                - amount
                - currency
                - customerId
      responses:
        '200':
          description: Successful response
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
                  amount:
                    type: number
                  currency:
                    type: string
                  status:
                    type: string
                    enum:
                      - pending
                      - succeeded
                      - failed
        '400':
          description: Bad request
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: string
        '500':
          description: Internal server error
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: string
```

## Common Use Cases

### Generate API Documentation

Use exported specs with documentation tools:

```bash
# Export OpenAPI specs
forklaunch openapi export

# Generate docs with Redoc
npx redoc-cli bundle .forklaunch/openapi/payments.json

# Or use Swagger UI
npx swagger-ui-watcher .forklaunch/openapi/payments.json
```

### Generate Client SDKs

Create type-safe client SDKs for frontend/mobile:

```bash
# Export OpenAPI specs
forklaunch openapi export

# Generate TypeScript client
npx openapi-generator-cli generate \
  -i .forklaunch/openapi/payments.json \
  -g typescript-fetch \
  -o ./clients/payments
```

### API Testing

Use specs with testing tools:

```bash
# Export specs
forklaunch openapi export

# Run contract tests with Dredd
dredd .forklaunch/openapi/payments.json http://localhost:3000
```

## Customizing Specifications

### Route-Level Documentation

Add detailed docs to individual routes:

```typescript
export const createChargeRoute = forklaunch.post('/charges', {
  name: 'CreateCharge',
  summary: 'Create a payment charge',
  description: `
    Creates a new charge for the specified amount.

    This endpoint:
    - Validates the customer exists
    - Checks sufficient balance
    - Creates the charge
    - Returns charge details
  `,
  tags: ['Charges', 'Payments'],
  deprecated: false,
  body: chargeRequestSchema,
  responses: {
    200: chargeSchema,
    400: errorSchema,
    401: unauthorizedSchema,
    500: serverErrorSchema
  }
}, handler);
```

## Validation

### Validate Exported Specs

```bash
# Export spec
forklaunch openapi export

# Validate with openapi-validator
npx @ibm/openapi-validator .forklaunch/openapi/payments.json

# Or with Swagger CLI
npx swagger-cli validate .forklaunch/openapi/payments.json
```

## Best Practices

1. **Export Regularly**: Include in build process or pre-commit hooks
2. **Version Control**: Commit generated specs to track API changes
3. **Validate**: Always validate exported specs before publishing
4. **Document Thoroughly**: Add summaries and descriptions to all routes
5. **Use Tags**: Organize routes with tags for better documentation
6. **Include Examples**: Add request/response examples in route definitions

## Troubleshooting

### Empty or Incomplete Spec

**Problem**: Exported spec is missing routes

**Solution**: Ensure routes are properly registered:
```typescript
// Must register routes with forklaunch
forklaunchApp.use(createChargeRoute);
```

### Type Conversion Errors

**Problem**: Zod schemas not converting correctly

**Solution**: Use supported Zod types or add custom converters

### Missing Descriptions

**Problem**: Generated spec lacks documentation

**Solution**: Add `.describe()` to Zod schemas:
```typescript
const schema = z.object({
  amount: z.number().describe('Amount in cents'),
  currency: z.string().describe('ISO currency code')
});
```

## Related Commands

- [`forklaunch sync`](/docs/cli/sync.md) - Regenerate service code
- [`forklaunch sdk mode`](/docs/cli/sdk.md) - Configure SDK generation
- [`forklaunch init service`](/docs/cli/init.md) - Create new service

## See Also

- [Framework HTTP Module](/docs/framework/http.md)
- [Schema Validation](/docs/framework/validation.md)
- [OpenAPI Specification](https://spec.openapis.org/oas/latest.html)
