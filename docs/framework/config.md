---
title: Framework - Config Injector
category: References
description: Reference for using ConfigInjector in ForkLaunch.
---

## Overview

The `ConfigInjector` abstraction makes it easy, safe and convenient to inject properly constructed dependencies when necessary. It is designed around the `Inversion of Control` principle, but does minimal _magic_ where construction is hidden and hard to debug.

It follows a simple structure:
1. you define a set of dependencies and their return types
2. you register the dependencies in a definition dictionary specifying the lifetime and how to construct the dependency when resolved
3. you control the creation of scopes (with helpful utilities for doing so)

## Basic Usage

```typescript
import { createConfigInjector, Lifetime, type } from '@forklaunch/core/services';
import { SchemaValidator } from '@forklaunch/validator/typebox';

// Create the injector with dependency definitions
const configInjector = createConfigInjector(SchemaValidator(), {
  DatabaseClient: {
    lifetime: Lifetime.Transient,
    type: type<DatabaseClient>(),
    factory: ({ Logger }) => new DatabaseClient(Logger)
  },
  Logger: {
    lifetime: Lifetime.Scoped,
    type: type<LoggerService>(),
    factory: () => new LoggerService()
  },
  API_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: process.env.API_KEY
  }
});
```

To aide in construction of dependencies, the `factory` method conforms to the simplified signature:

```typescript
function factory<T extends keyof ConfigShapes>(args: SplayedArgs<keyof ConfigShapes>, resolve: (token: T) => ConfigShapes[T], context: Record<string, unknown>);
```

This solves multiple problems:
  1. allows you to instantiate other dependencies subject to their definition, 
  2. circumvent circular dependencies by creating callbacks,
  3. pass outside context to construction

## Lifetime Management

The ConfigInjector supports three types of lifetimes:

- `Singleton`: Created once and reused across all scopes
- `Scoped`: Created once per scope
- `Transient`: Created new for each resolution

```typescript
enum Lifetime {
    Singleton = 'singleton',
    Scoped = 'scoped',
    Transient = 'transient'
}
```

## Dependency Resolution

Dependencies can be resolved using the tokens object for type-safe resolution:

### Resolution
```typescript
// Create container and tokens
const { ci, tokens } = createDependencyContainer();

// Resolve dependencies
const logger = ci.resolve(tokens.Logger);
const dbClient = ci.resolve(tokens.DatabaseClient);
```

## Scoping

The ConfigInjector supports creating scopes for managing dependency lifetimes:

```typescript
// Create a new scope
const scope = configInjector.createScope();

// Resolve scoped dependencies
const scopedService = scope.resolve('someService');

// Clean up scope
scope.dispose();
```

## Validation

The ConfigInjector includes built-in validation:

```typescript
// Safe validation that returns a result
const validationResult = configInjector.safeValidateConfigSingletons();

// Validation that throws on error
const validInjector = configInjector.validateConfigSingletons('MyConfig');
```

## Circular Dependency Detection

The ConfigInjector automatically detects and prevents circular dependencies:

```typescript
// This will throw an error if serviceA depends on serviceB which depends on serviceA
const serviceA = configInjector.resolve('serviceA');
```

## Context Injection

You can pass context to dependency resolution, if during construction, you need outside information:

```typescript
const service = configInjector.resolve('service', { 
    requestId: '123',
    userId: '456' 
});
```

## Best Practices

1. Define dependencies at the highest appropriate level
2. Use singletons for stateless services
3. Use scoped lifetime for request-scoped dependencies
4. Use transient lifetime sparingly
5. Always validate your configuration during startup
6. Dispose of scopes when they're no longer needed

## Type Safety

The ConfigInjector is fully typed and will provide compile-time type checking for:

- Dependency definitions
- Resolution types
- Factory arguments
- Validation results
