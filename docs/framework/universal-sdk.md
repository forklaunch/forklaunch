---
title: Framework - Universal SDK
category: References
description: Reference for using the ForkLaunch Universal SDK for type-safe service-to-service communication.
---

## Overview

The Universal SDK provides an auto-generated, type-safe client for communicating with ForkLaunch services. It:
- Automatically generates typed methods from OpenAPI specifications
- Refreshes API definitions on-the-fly
- Provides full TypeScript autocomplete for all API endpoints
- Handles request/response serialization
- Maps SDK method calls to actual HTTP requests

## Basic Usage

```typescript
import { universalSdk } from '@forklaunch/universal-sdk';
import type { UsersSdkClient } from '@ecommerce-api/users';

// Initialize SDK with host and OpenAPI registry path
const usersSdk = await universalSdk<UsersSdkClient>({
  host: 'http://localhost:3000',
  registryOptions: { path: 'api/v1/openapi' }
});

// Make type-safe requests using generated methods
const user = await usersSdk.getUser({
  params: { id: '123' }
});
```

## Dependency Injection Setup

The recommended way to use Universal SDK is through dependency injection in your `registrations.ts`:

```typescript
import { universalSdk } from '@forklaunch/universal-sdk';
import type { ProductsSdkClient } from '@ecommerce-api/products';
import { Lifetime, type } from '@forklaunch/core/services';

const runtimeDependencies = environmentConfig.chain({
  ProductsSdk: {
    lifetime: Lifetime.Singleton,
    type: type<Promise<ProductsSdkClient>>(),
    factory: ({ PRODUCTS_URL }) =>
      universalSdk<ProductsSdkClient>({
        host: PRODUCTS_URL,
        registryOptions: { path: 'api/v1/openapi' }
      })
  }
});

// Resolve in your application
const productsSdk = await ci.resolve(tokens.ProductsSdk);
```

## How It Works

1. **OpenAPI Registry**: The SDK fetches the OpenAPI specification from the service's registry endpoint
2. **Path Mapping**: Maps OpenAPI operation IDs to SDK method names (e.g., `getUser`, `createProduct`)
3. **Proxy Pattern**: Uses JavaScript proxies to intercept method calls and route them to the correct endpoints
4. **Auto-refresh**: Periodically checks for OpenAPI spec updates and regenerates the SDK

## SDK Method Calls

Methods are automatically generated based on the `name` field in your route definitions:

```typescript
// Service defines route
app.get('/users/:id', {
  name: 'GetUser',  // Becomes: usersSdk.getUser()
  params: { id: string },
  responses: { 200: userSchema }
}, handler);

// Client calls
const user = await usersSdk.getUser({
  params: { id: '123' }
});
```

## Request Options

```typescript
type RequestType = {
  params?: Record<string, string | number>;  // URL path parameters
  query?: Record<string, string | number>;   // Query string parameters
  body?: Record<string, unknown>;            // Request body (for POST/PUT/PATCH)
  headers?: Record<string, string>;          // Custom headers
};
```

### Example with All Options

```typescript
await productsSdk.updateProduct({
  params: { id: 'prod-123' },
  query: { notify: 'true' },
  body: { name: 'New Product Name', price: 29.99 },
  headers: { 'x-idempotency-key': 'unique-key' }
});
```

## Response Format

```typescript
interface ResponseType {
  code: number;        // HTTP status code
  content: unknown;    // Parsed response body
  headers: Headers;    // Response headers
}
```

## Content Type Handling

Custom content type parsers can be provided:

```typescript
const sdk = await universalSdk<ApiClient>({
  host: 'http://localhost:3000',
  registryOptions: { path: 'api/v1/openapi' },
  contentTypeParserMap: {
    'application/xml': (text) => parseXML(text),
    'text/csv': (text) => parseCSV(text)
  }
});
```

## Error Handling

```typescript
try {
  const user = await usersSdk.getUser({ params: { id: '123' } });

  if (user.code >= 400) {
    console.error('API Error:', user.content);
  }
} catch (error) {
  console.error('Network or parsing error:', error);
}
```

## Advanced: Direct Fetch

For endpoints not in the OpenAPI spec, use the `fetch` method:

```typescript
const response = await sdk.fetch('/custom/path', {
  method: 'post',
  body: { data: 'value' },
  headers: { 'x-custom': 'header' }
});
```

## Best Practices

1. **Use Dependency Injection**: Register SDKs as singletons in your DI container
2. **Type Safety**: Always provide the typed SDK client as a generic parameter
3. **Environment Configuration**: Store service URLs in environment variables
4. **Error Handling**: Check response codes and handle errors appropriately
5. **OpenAPI Registry**: Ensure your services expose OpenAPI specs at the configured path

