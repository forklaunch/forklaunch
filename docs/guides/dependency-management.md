---
title: Dependency Injection
category: Guides
description: Managing dependencies with ForkLaunch's type-safe dependency injection container using lifetime patterns and factory functions.
---

## Overview

ForkLaunch uses a **type-safe dependency injection (DI) container** built on the `configInjector` system. Dependencies are explicitly registered in `registrations.ts` files with factory functions and lifetime management, providing compile-time type safety and runtime injection.

## Dependency Injection Pattern

### Core Concepts

1. **Explicit Registration** - Dependencies are registered in `registrations.ts`
2. **Lifetime Management** - Singleton, Scoped, or Transient lifetimes
3. **Factory Functions** - Dependencies constructed via factories
4. **Type Safety** - Full TypeScript type inference
5. **Dependency Resolution** - Automatic resolution of dependency chains

### registrations.ts Structure

Every service has a `registrations.ts` file that defines its dependencies:

```typescript
// src/registrations.ts
import { createConfigInjector, Lifetime, getEnvVar } from '@forklaunch/core/services';
import { schemaValidator } from '@forklaunch/validator';
import { MikroORM, EntityManager } from '@mikro-orm/core';
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import mikroOrmOptionsConfig from './mikro-orm.config';

// Create base config injector
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: { name: string, version: string },
    value: {
      name: 'my-service',
      version: '1.0.0'
    }
  }
});

// Environment configuration (Singleton)
const environmentConfig = configInjector.chain({
  DB_HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('DB_HOST')
  },
  REDIS_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('REDIS_URL')
  }
});

// Runtime dependencies (Singleton/Scoped)
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions)
  },
  TtlCache: {
    lifetime: Lifetime.Singleton,
    type: RedisTtlCache,
    factory: ({ REDIS_URL, OpenTelemetryCollector }) =>
      new RedisTtlCache(
        60 * 60 * 1000,  // 1 hour TTL
        OpenTelemetryCollector,
        { url: REDIS_URL },
        { enabled: true, level: 'info' }
      )
  }
});

// Service dependencies (Scoped)
const serviceDependencies = runtimeDependencies.chain({
  UserService: {
    lifetime: Lifetime.Scoped,
    type: BaseUserService,
    factory: ({ EntityManager, OpenTelemetryCollector }) =>
      new BaseUserService(EntityManager, OpenTelemetryCollector)
  }
});

// Export container
export const createDependencyContainer = (envFilePath: string) => ({
  ci: serviceDependencies.validateConfigSingletons(envFilePath),
  tokens: serviceDependencies.tokens()
});
```

## Lifetime Patterns

ForkLaunch supports three dependency lifetimes:

### 1. Singleton

Created **once** per application, shared across all requests:

```typescript
{
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  TtlCache: {
    lifetime: Lifetime.Singleton,
    type: RedisTtlCache,
    factory: ({ REDIS_URL }) => new RedisTtlCache(...)
  },
  S3ObjectStore: {
    lifetime: Lifetime.Singleton,
    type: S3ObjectStore,
    factory: ({ S3_BUCKET, S3_REGION }) => new S3ObjectStore(...)
  }
}
```

**Use Cases:**
- Database connections
- Cache clients
- Object storage clients
- Configuration objects
- Shared infrastructure clients

### 2. Scoped

Created **once per request/scope**, isolated from other requests:

```typescript
{
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions)
  },
  UserService: {
    lifetime: Lifetime.Scoped,
    type: BaseUserService,
    factory: ({ EntityManager, OpenTelemetryCollector }) =>
      new BaseUserService(EntityManager, OpenTelemetryCollector)
  }
}
```

**Use Cases:**
- Request-specific database sessions (EntityManager)
- Request-scoped services
- Transaction contexts
- Per-request state

### 3. Transient

Created **every time** it's requested, never cached:

```typescript
{
  RequestId: {
    lifetime: Lifetime.Transient,
    type: string,
    factory: () => crypto.randomUUID()
  },
  Timestamp: {
    lifetime: Lifetime.Transient,
    type: Date,
    factory: () => new Date()
  }
}
```

**Use Cases:**
- Unique identifiers
- Timestamps
- Temporary objects
- Non-cacheable computations

## Factory Functions

Dependencies are created using factory functions with automatic dependency injection:

### Factory Signature

```typescript
factory: (dependencies, resolve, context) => Instance
```

**Parameters:**
- `dependencies` - Object containing all declared dependencies
- `resolve` - Function to resolve additional dependencies
- `context` - Request/scope-specific context (for Scoped lifetime)

### Basic Factory

```typescript
{
  UserService: {
    lifetime: Lifetime.Scoped,
    type: BaseUserService,
    factory: ({ EntityManager }) => new BaseUserService(EntityManager)
  }
}
```

### Factory with Multiple Dependencies

```typescript
{
  PaymentService: {
    lifetime: Lifetime.Scoped,
    type: BasePaymentService,
    factory: ({
      EntityManager,
      TtlCache,
      S3ObjectStore,
      OpenTelemetryCollector
    }) =>
      new BasePaymentService(
        EntityManager,
        TtlCache,
        S3ObjectStore,
        OpenTelemetryCollector
      )
  }
}
```

### Factory with Context (Scoped)

```typescript
{
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions)
  }
}
```

## Chaining Dependencies

Use `.chain()` to compose dependency containers:

```typescript
// Base config
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: { /* ... */ }
});

// Chain: Environment variables
const environmentConfig = configInjector.chain({
  DB_HOST: { lifetime: Lifetime.Singleton, type: string, value: getEnvVar('DB_HOST') },
  DB_PORT: { lifetime: Lifetime.Singleton, type: number, value: Number(getEnvVar('DB_PORT')) }
});

// Chain: Infrastructure
const runtimeDependencies = environmentConfig.chain({
  MikroORM: { /* ... */ },
  TtlCache: { /* ... */ },
  S3ObjectStore: { /* ... */ }
});

// Chain: Services
const serviceDependencies = runtimeDependencies.chain({
  UserService: { /* ... */ },
  PaymentService: { /* ... */ }
});
```

**Benefits:**
- Clear separation of concerns
- Dependencies available to subsequent chains
- Logical grouping (env → runtime → services)

## Resolving Dependencies

### In Routes/Controllers

```typescript
// api/controllers/user.controller.ts
import { handlers } from '@forklaunch/framework';

export const userPost = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'User Post',
    body: UserRequestSchema,
    responses: { 200: UserResponseSchema }
  },
  async (req, res) => {
    // Resolve from request scope
    const { UserService } = req.scope.cradle;

    const user = await UserService.userPost(req.body);
    res.json(user);
  }
);
```

### In Services

```typescript
// services/user.service.ts
import { EntityManager } from '@mikro-orm/core';

export class BaseUserService {
  constructor(
    private em: EntityManager,
    private telemetry: OpenTelemetryCollector
  ) {}

  async userPost(dto: UserRequestDto): Promise<UserResponseDto> {
    // EntityManager is scoped to this request
    const entity = this.em.create(User, { ...dto });
    await this.em.flush();

    this.telemetry.info('User created', { id: entity.id });

    return UserResponseMapper.toDto(entity);
  }
}
```

### Direct Resolution

```typescript
// bootstrapper.ts
import { createDependencyContainer } from './registrations';

const { ci, tokens } = createDependencyContainer('.env.local');

// Resolve singleton
const openTelemetry = ci.resolve(tokens.OpenTelemetryCollector);

// Resolve with scope
const scope = ci.createScope();
const userService = scope.resolve(tokens.UserService);
```

## Real-World Example

Complete dependency injection setup for a service:

```typescript
// src/registrations.ts
import {
  createConfigInjector,
  Lifetime,
  getEnvVar,
  type
} from '@forklaunch/core/services';
import { schemaValidator } from '@forklaunch/validator';
import { MikroORM, EntityManager, ForkOptions } from '@mikro-orm/core';
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import { S3ObjectStore } from '@forklaunch/infrastructure-s3';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import mikroOrmOptionsConfig from './mikro-orm.config';
import { BaseUserService } from './services/user.service';
import { BasePaymentService } from './services/payment.service';
import { metrics } from './metrics';

// Service metadata
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: { name: string, version: string },
    value: {
      name: 'payment-service',
      version: '1.0.0'
    }
  }
});

// Environment configuration
const environmentConfig = configInjector.chain({
  // Database
  DB_HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('DB_HOST')
  },
  DB_PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('DB_PORT'))
  },
  DB_USER: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('DB_USER')
  },
  DB_PASSWORD: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('DB_PASSWORD')
  },
  DB_NAME: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('DB_NAME')
  },

  // Cache
  REDIS_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('REDIS_URL')
  },

  // Storage
  S3_REGION: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('S3_REGION')
  },
  S3_BUCKET: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('S3_BUCKET')
  },
  S3_ACCESS_KEY_ID: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('S3_ACCESS_KEY_ID')
  },
  S3_SECRET_ACCESS_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('S3_SECRET_ACCESS_KEY')
  },

  // Telemetry
  OTEL_SERVICE_NAME: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('OTEL_SERVICE_NAME')
  },
  OTEL_LEVEL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('OTEL_LEVEL')
  }
});

// Runtime dependencies (infrastructure)
const runtimeDependencies = environmentConfig.chain({
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },

  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },

  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  },

  TtlCache: {
    lifetime: Lifetime.Singleton,
    type: RedisTtlCache,
    factory: ({ REDIS_URL, OpenTelemetryCollector }) =>
      new RedisTtlCache(
        60 * 60 * 1000,  // 1 hour default TTL
        OpenTelemetryCollector,
        { url: REDIS_URL },
        { enabled: true, level: 'info' }
      )
  },

  S3ObjectStore: {
    lifetime: Lifetime.Singleton,
    type: S3ObjectStore,
    factory: ({
      S3_REGION,
      S3_BUCKET,
      S3_ACCESS_KEY_ID,
      S3_SECRET_ACCESS_KEY,
      OpenTelemetryCollector,
      OTEL_LEVEL
    }) =>
      new S3ObjectStore(
        OpenTelemetryCollector,
        {
          bucket: S3_BUCKET,
          clientConfig: {
            region: S3_REGION,
            credentials: {
              accessKeyId: S3_ACCESS_KEY_ID,
              secretAccessKey: S3_SECRET_ACCESS_KEY
            }
          }
        },
        {
          enabled: true,
          level: OTEL_LEVEL || 'info'
        }
      )
  }
});

// Service dependencies (business logic)
const serviceDependencies = runtimeDependencies.chain({
  UserService: {
    lifetime: Lifetime.Scoped,
    type: BaseUserService,
    factory: ({ EntityManager, OpenTelemetryCollector }) =>
      new BaseUserService(EntityManager, OpenTelemetryCollector)
  },

  PaymentService: {
    lifetime: Lifetime.Scoped,
    type: BasePaymentService,
    factory: ({
      EntityManager,
      TtlCache,
      S3ObjectStore,
      OpenTelemetryCollector
    }) =>
      new BasePaymentService(
        EntityManager,
        TtlCache,
        S3ObjectStore,
        OpenTelemetryCollector
      )
  }
});

// Export container
export const createDependencyContainer = (envFilePath: string) => ({
  ci: serviceDependencies.validateConfigSingletons(envFilePath),
  tokens: serviceDependencies.tokens()
});
```

## Circular Dependency Detection

ForkLaunch automatically detects circular dependencies at startup:

```typescript
// ❌ Circular dependency
const dependencies = configInjector.chain({
  ServiceA: {
    lifetime: Lifetime.Scoped,
    factory: ({ ServiceB }) => new ServiceA(ServiceB)
  },
  ServiceB: {
    lifetime: Lifetime.Scoped,
    factory: ({ ServiceA }) => new ServiceB(ServiceA)  // Circular!
  }
});

// Error: Circular dependency detected: ServiceA → ServiceB → ServiceA
```

**Solutions:**
1. Use events/messaging for loose coupling
2. Extract shared logic to a separate service
3. Refactor to remove bidirectional dependency

## Type Safety

Full TypeScript type inference throughout the DI system:

```typescript
// Define dependencies with types
const deps = configInjector.chain({
  UserService: {
    lifetime: Lifetime.Scoped,
    type: BaseUserService,  // TypeScript class type
    factory: ({ EntityManager }) => new BaseUserService(EntityManager)
  }
});

// Resolve with full type safety
const { ci, tokens } = createDependencyContainer('.env');

// tokens.UserService is typed as BaseUserService
const userService = ci.resolve(tokens.UserService);
// userService is BaseUserService (not 'any')

// In routes
const { UserService } = req.scope.cradle;
// UserService is BaseUserService (typed from registrations)
```

## Best Practices

### 1. Use Appropriate Lifetimes

```typescript
// ✅ Good: Singleton for shared infrastructure
{
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(config)
  }
}

// ✅ Good: Scoped for request-specific instances
{
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }) => MikroORM.em.fork()
  }
}

// ❌ Bad: Singleton for request-specific data
{
  EntityManager: {
    lifetime: Lifetime.Singleton,  // ❌ Shared across requests!
    type: EntityManager,
    factory: ({ MikroORM }) => MikroORM.em.fork()
  }
}
```

### 2. Organize with .chain()

```typescript
// ✅ Good: Logical grouping
const env = configInjector.chain({ /* env vars */ });
const runtime = env.chain({ /* infrastructure */ });
const services = runtime.chain({ /* business logic */ });

// ❌ Bad: Everything in one chain
const everything = configInjector.chain({
  DB_HOST: /* ... */,
  MikroORM: /* ... */,
  UserService: /* ... */
});
```

### 3. Explicit Dependencies

```typescript
// ✅ Good: Explicit dependencies in factory
{
  PaymentService: {
    lifetime: Lifetime.Scoped,
    factory: ({ EntityManager, TtlCache, S3ObjectStore }) =>
      new PaymentService(EntityManager, TtlCache, S3ObjectStore)
  }
}

// ❌ Bad: Resolving from global scope
{
  PaymentService: {
    lifetime: Lifetime.Scoped,
    factory: () => {
      const em = someGlobalContainer.get('EntityManager');  // ❌
      return new PaymentService(em);
    }
  }
}
```

### 4. Test with Mocks

```typescript
// __tests__/services/user.test.ts
import { createConfigInjector, Lifetime } from '@forklaunch/core/services';

describe('UserService', () => {
  test('creates user', async () => {
    // Mock dependencies
    const mockEm = {
      create: jest.fn(),
      flush: jest.fn()
    };

    const mockTelemetry = {
      info: jest.fn()
    };

    // Create test container
    const testDeps = createConfigInjector(schemaValidator, {}).chain({
      EntityManager: {
        lifetime: Lifetime.Scoped,
        type: EntityManager,
        value: mockEm as any
      },
      OpenTelemetryCollector: {
        lifetime: Lifetime.Singleton,
        type: OpenTelemetryCollector,
        value: mockTelemetry as any
      },
      UserService: {
        lifetime: Lifetime.Scoped,
        type: BaseUserService,
        factory: ({ EntityManager, OpenTelemetryCollector }) =>
          new BaseUserService(EntityManager, OpenTelemetryCollector)
      }
    });

    const { ci, tokens } = {
      ci: testDeps.validateConfigSingletons('.env.test'),
      tokens: testDeps.tokens()
    };

    // Test service
    const scope = ci.createScope();
    const userService = scope.resolve(tokens.UserService);

    await userService.userPost({ name: 'Test', email: 'test@example.com' });

    expect(mockEm.create).toHaveBeenCalled();
    expect(mockEm.flush).toHaveBeenCalled();
  });
});
```

### 5. Document Dependencies

```typescript
/**
 * Payment Service
 *
 * Dependencies:
 * - EntityManager: Database access (Scoped)
 * - TtlCache: Payment session caching (Singleton)
 * - S3ObjectStore: Receipt storage (Singleton)
 * - OpenTelemetryCollector: Logging/metrics (Singleton)
 */
export class BasePaymentService {
  constructor(
    private em: EntityManager,
    private cache: TtlCache,
    private s3: S3ObjectStore,
    private telemetry: OpenTelemetryCollector
  ) {}
}
```

## Related Documentation

- [Contract-First Development](/docs/guides/contract-first-development.md)
- [Infrastructure Overview](/docs/infrastructure/overview.md)
- [Databases](/docs/infrastructure/databases.md)
- [Caches](/docs/infrastructure/caches.md)
- [Storage](/docs/infrastructure/storage.md)
