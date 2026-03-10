---
title: OpenAPI Command
category: CLI Reference
description: Export and manage OpenAPI specifications for your services.
---

## Overview

The `openapi` command exports OpenAPI 3.0 specifications from your ForkLaunch services, enabling API documentation, client SDK generation, and integration with API tools.

## Usage

```bash
forklaunch openapi <subcommand> [options]
```

## Subcommands

### export

Export OpenAPI specifications for one or more services.

```bash
forklaunch openapi export [options]
```

**Options:**
- `-p, --path <path>` - Path to application root (optional)
- `-s, --service <name>` - Export specific service (can be repeated)
- `-o, --output <dir>` - Output directory (default: `./openapi`)
- `--all` - Export all services
- `--format <format>` - Output format: `json` or `yaml` (default: both)
- `--watch` - Watch for changes and auto-regenerate

## Basic Usage

### Export All Services

```bash
$ forklaunch openapi export --all

[INFO] Exporting OpenAPI specifications...
[OK] Exported users → openapi/users.json
[OK] Exported users → openapi/users.yaml
[OK] Exported payments → openapi/payments.json
[OK] Exported payments → openapi/payments.yaml
[OK] Exported notifications → openapi/notifications.json
[OK] Exported notifications → openapi/notifications.yaml
[OK] Exported 3 services successfully
```

### Export Specific Service

```bash
$ forklaunch openapi export --service payments

[INFO] Exporting OpenAPI specification for: payments
[OK] Exported payments → openapi/payments.json
[OK] Exported payments → openapi/payments.yaml
```

### Export Multiple Services

```bash
$ forklaunch openapi export --service payments --service users

[INFO] Exporting OpenAPI specifications...
[OK] Exported payments → openapi/payments.json
[OK] Exported payments → openapi/payments.yaml
[OK] Exported users → openapi/users.json
[OK] Exported users → openapi/users.yaml
```

### Custom Output Directory

```bash
$ forklaunch openapi export --all --output ./docs/api

[INFO] Exporting OpenAPI specifications...
[OK] Exported users → docs/api/users.json
[OK] Exported users → docs/api/users.yaml
[OK] Exported payments → docs/api/payments.json
[OK] Exported payments → docs/api/payments.yaml
```

### Watch Mode

Auto-regenerate specs when service code changes:

```bash
$ forklaunch openapi export --all --watch

[INFO] Watching for changes...
[INFO] Press Ctrl+C to stop

[INFO] Change detected: src/modules/payments/api/routes/charge.routes.ts
[OK] Re-exported payments → openapi/payments.json
[OK] Re-exported payments → openapi/payments.yaml
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
forklaunch openapi export --all --format yaml

# Generate docs with Redoc
npx redoc-cli bundle openapi/payments.yaml

# Or use Swagger UI
npx swagger-ui-watcher openapi/payments.yaml
```

### Generate Client SDKs

Create type-safe client SDKs for frontend/mobile:

```bash
# Export OpenAPI specs
forklaunch openapi export --service payments

# Generate TypeScript client
npx openapi-generator-cli generate \
  -i openapi/payments.json \
  -g typescript-fetch \
  -o ./clients/payments

# Generate Python client
npx openapi-generator-cli generate \
  -i openapi/payments.json \
  -g python \
  -o ./clients/payments-python
```

### API Testing

Use specs with testing tools:

```bash
# Export specs
forklaunch openapi export --all

# Run Postman tests
newman run openapi/payments.json

# Run contract tests with Dredd
dredd openapi/payments.yaml http://localhost:3000
```

### API Gateway Integration

Configure API gateways using OpenAPI specs:

```bash
# Export specs
forklaunch openapi export --service payments --format yaml

# Import to AWS API Gateway
aws apigateway import-rest-api \
  --body file://openapi/payments.yaml

# Or Kong Gateway
deck sync --state openapi/payments.yaml
```

### CI/CD Integration

Include in your CI/CD pipeline:

```yaml
# .github/workflows/api-docs.yml
name: Generate API Docs

on: [push]

jobs:
  docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2

      - name: Setup Node
        uses: actions/setup-node@v2

      - name: Install ForkLaunch CLI
        run: npm install -g forklaunch

      - name: Export OpenAPI specs
        run: forklaunch openapi export --all --format yaml

      - name: Generate documentation
        run: npx redoc-cli bundle openapi/*.yaml

      - name: Publish to GitHub Pages
        uses: peaceiris/actions-gh-pages@v3
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: ./docs
```

## Output Formats

### JSON Format

```bash
forklaunch openapi export --service payments --format json
```

**Benefits:**
- Smaller file size
- Direct JavaScript/TypeScript import
- Faster parsing
- Better for tooling

**Use cases:**
- Client SDK generation
- API testing tools
- JavaScript/TypeScript consumers

### YAML Format

```bash
forklaunch openapi export --service payments --format yaml
```

**Benefits:**
- Human-readable
- Easy to edit
- Better for documentation
- Standard for many tools

**Use cases:**
- Documentation generation
- Manual review
- API gateway configuration
- Version control

### Both Formats (Default)

```bash
forklaunch openapi export --service payments
```

Exports both `.json` and `.yaml` files for maximum flexibility.

## Customizing Specifications

### Service-Level Configuration

Configure OpenAPI metadata in service config:

```typescript
// src/modules/payments/config.ts
export const serviceConfig = {
  name: 'payments',
  version: '1.0.0',
  openapi: {
    title: 'Payments API',
    description: 'Secure payment processing service',
    contact: {
      name: 'API Support',
      email: 'api@example.com'
    },
    license: {
      name: 'MIT'
    },
    servers: [
      {
        url: 'https://api.example.com/payments',
        description: 'Production'
      },
      {
        url: 'https://staging-api.example.com/payments',
        description: 'Staging'
      }
    ]
  }
};
```

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
forklaunch openapi export --service payments

# Validate with openapi-validator
npx @ibm/openapi-validator openapi/payments.yaml

# Or with Swagger CLI
npx swagger-cli validate openapi/payments.yaml
```

### Lint OpenAPI Specs

```bash
# Export spec
forklaunch openapi export --service payments

# Lint with Spectral
npx @stoplight/spectral-cli lint openapi/payments.yaml
```

## Best Practices

1. **Export Regularly**: Include in build process or pre-commit hooks
2. **Version Control**: Commit generated specs to track API changes
3. **Validate**: Always validate exported specs before publishing
4. **Document Thoroughly**: Add summaries and descriptions to all routes
5. **Use Tags**: Organize routes with tags for better documentation
6. **Include Examples**: Add request/response examples in route definitions
7. **Semantic Versioning**: Version your APIs following semver
8. **Automate**: Generate and publish docs automatically in CI/CD

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

- [API Documentation Guide](/docs/guides/api-documentation.md)
- [Framework HTTP Module](/docs/framework/http.md)
- [Schema Validation](/docs/framework/validation.md)
- [OpenAPI Specification](https://spec.openapis.org/oas/latest.html)
