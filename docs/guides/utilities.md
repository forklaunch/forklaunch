---
title: Framework - Utilities
category: References
description: Reference for utility packages and helper functions in ForkLaunch.
---

## Overview

ForkLaunch provides a comprehensive set of utility packages for common programming tasks. These utilities are organized into several modules:

- **Common Package** (`@forklaunch/common`) - String manipulation, object utilities, type guards, and helpers
- **Mappers Module** (`@forklaunch/core/mappers`) - Data transformation between DTOs and entities
- **Persistence Module** (`@forklaunch/core/persistence`) - Base entity class with convenient CRUD methods
- **Environment Module** (`@forklaunch/core/environment`) - Cascading environment variable loading

## Common Package

### String Manipulation

#### `toCamelCaseIdentifier(str: string)`

Converts a string to a valid TypeScript identifier in camelCase. Handles various separators (dash, underscore, dot, slash, colon, space) and removes leading numbers.

```typescript
import { toCamelCaseIdentifier } from '@forklaunch/common';

toCamelCaseIdentifier("hello-world");        // "helloWorld"
toCamelCaseIdentifier("my_var_name");        // "myVarName"
toCamelCaseIdentifier("some.property.name"); // "somePropertyName"
toCamelCaseIdentifier("123invalid");         // "invalid"
toCamelCaseIdentifier("/organization/base"); // "organizationBase"
toCamelCaseIdentifier("API-Key");            // "aPIKey"
```

**Type Signature:**
```typescript
function toCamelCaseIdentifier<T extends string>(str: T): CamelCaseIdentifier<T>
```

#### `toPrettyCamelCase(str: string)`

Similar to `toCamelCaseIdentifier` but handles abbreviations more intuitively by lowercasing all parts first.

```typescript
import { toPrettyCamelCase } from '@forklaunch/common';

toPrettyCamelCase("API-Key");           // "apiKey"
toPrettyCamelCase("HTTP-Response");     // "httpResponse"
toPrettyCamelCase("user-ID");           // "userId"
toPrettyCamelCase("get-user-by-id");    // "getUserById"
```

**Type Signature:**
```typescript
function toPrettyCamelCase<T extends string>(str: T): PrettyCamelCase<T>
```

#### `capitalize(str: string)`

Capitalizes the first letter of a string.

```typescript
import { capitalize } from '@forklaunch/common';

capitalize("hello"); // "Hello"
capitalize("world"); // "World"
```

**Type Signature:**
```typescript
function capitalize(str: string): string
```

#### `uncapitalize(str: string)`

Converts the first letter to lowercase.

```typescript
import { uncapitalize } from '@forklaunch/common';

uncapitalize("Hello"); // "hello"
uncapitalize("World"); // "world"
```

**Type Signature:**
```typescript
function uncapitalize(str: string): string
```

#### `isValidIdentifier(str: string)`

Checks if a string is a valid JavaScript/TypeScript identifier.

```typescript
import { isValidIdentifier } from '@forklaunch/common';

isValidIdentifier("myVar");     // true
isValidIdentifier("123invalid"); // false
isValidIdentifier("my-var");    // false
isValidIdentifier("_private");  // true
```

**Type Signature:**
```typescript
function isValidIdentifier(str: string): boolean
```

#### `openApiCompliantPath(path: string)`

Converts Express-style path parameters (`:paramName`) to OpenAPI format (`{paramName}`).

```typescript
import { openApiCompliantPath } from '@forklaunch/common';

openApiCompliantPath('/users/:id/posts/:postId');
// Returns: '/users/{id}/posts/{postId}'

openApiCompliantPath('/products/:productId/reviews/:reviewId');
// Returns: '/products/{productId}/reviews/{reviewId}'
```

**Type Signature:**
```typescript
function openApiCompliantPath(path: string): string
```

### Path Manipulation

#### `removeTrailingSlash(path: string)`

Removes a trailing slash from a path string if present.

```typescript
import { removeTrailingSlash } from '@forklaunch/common';

removeTrailingSlash('/users/'); // '/users'
removeTrailingSlash('/users');  // '/users'
```

**Type Signature:**
```typescript
function removeTrailingSlash(path: string): string
```

#### `removeDoubleLeadingSlash(path: string)`

Removes a double leading slash from a path string if present.

```typescript
import { removeDoubleLeadingSlash } from '@forklaunch/common';

removeDoubleLeadingSlash('//users'); // '/users'
removeDoubleLeadingSlash('/users');  // '/users'
```

**Type Signature:**
```typescript
function removeDoubleLeadingSlash(path: string): string
```

#### `sanitizePathSlashes(path: string)`

Sanitizes a path string by removing both trailing slashes and double leading slashes.

```typescript
import { sanitizePathSlashes } from '@forklaunch/common';

sanitizePathSlashes('//users/'); // '/users'
sanitizePathSlashes('/users');   // '/users'
```

**Type Signature:**
```typescript
function sanitizePathSlashes(path: string): string
```

### Object Utilities

#### `stripUndefinedProperties<T>(obj: T)`

Removes all properties with `undefined` values from an object. Note: Does NOT strip `null` values.

```typescript
import { stripUndefinedProperties } from '@forklaunch/common';

const obj = { a: 1, b: undefined, c: null, d: 'hello' };
const result = stripUndefinedProperties(obj);
// Result: { a: 1, c: null, d: 'hello' }
```

**Type Signature:**
```typescript
function stripUndefinedProperties<T extends Record<string, unknown>>(
  obj: T
): Partial<T>
```

#### `deepCloneWithoutUndefined<T>(obj: T)`

Deep clones an object while removing all `undefined` values. Preserves methods, getters/setters, non-enumerable properties, symbols, and prototype chains. Handles circular references safely.

```typescript
import { deepCloneWithoutUndefined } from '@forklaunch/common';

const original = {
  name: 'John',
  age: undefined,
  address: {
    city: 'NYC',
    zip: undefined
  }
};

const cloned = deepCloneWithoutUndefined(original);
// Result: { name: 'John', address: { city: 'NYC' } }
```

Supports:
- Built-in types: `Date`, `RegExp`, `Map`, `Set`
- Arrays with undefined elements removed
- Circular references (tracked via WeakMap)
- Class instances with prototype chains
- Non-enumerable properties and symbols

**Type Signature:**
```typescript
function deepCloneWithoutUndefined<T>(obj: T, seen?: WeakMap): T
```

#### `sortObjectKeys<T>(obj: T)`

Recursively sorts the keys of an object and its nested objects alphabetically. Useful for consistent object serialization and comparison.

```typescript
import { sortObjectKeys } from '@forklaunch/common';

const obj = { b: 2, a: 1, c: { f: 6, e: 5 } };
const sorted = sortObjectKeys(obj);
// Result: { a: 1, b: 2, c: { e: 5, f: 6 } }
```

**Type Signature:**
```typescript
function sortObjectKeys<T extends Record<string, unknown>>(obj: T): T
```

#### `toRecord<T>(obj: T)`

Converts an object to a record type.

```typescript
import { toRecord } from '@forklaunch/common';

const obj = { a: 1, b: 'hello' };
const record = toRecord(obj);
// Type: Record<string, unknown>
```

**Type Signature:**
```typescript
function toRecord<T>(obj: T): Record<string, unknown>
```

#### `emptyObject`

An empty object literal constant, commonly used as a default value or placeholder.

```typescript
import { emptyObject } from '@forklaunch/common';

const defaultConfig = emptyObject;
const merged = { ...emptyObject, ...userConfig };
```

**Type Signature:**
```typescript
const emptyObject: {}
```

### Hashing and Encoding

#### `hashString(str: string)`

Computes a simple, non-cryptographic hash code for a given string. Suitable for basic hashing needs such as object uniqueness checks, but should NOT be used for cryptographic or security-sensitive purposes.

```typescript
import { hashString } from '@forklaunch/common';

const hash = hashString("hello");
console.log(hash); // e.g., 99162322
```

**Type Signature:**
```typescript
function hashString(str: string): number
```

### JSON Operations

#### `safeParse<T>(input: unknown)`

Safely parses a JSON string. Returns the original input if parsing fails.

```typescript
import { safeParse } from '@forklaunch/common';

const result1 = safeParse<{ name: string }>('{"name":"John"}');
// Result: { name: "John" }

const result2 = safeParse("invalid json");
// Result: "invalid json"
```

**Type Signature:**
```typescript
function safeParse<T>(input: unknown): T
```

#### `safeStringify(arg: unknown)`

Safely stringifies any JavaScript value, handling special cases like:
- Error objects
- BigInt
- Functions
- Symbols
- Special number values (NaN, Infinity)
- Built-in objects (Date, RegExp)
- Collections (Map, Set)
- TypedArrays and ArrayBuffers

```typescript
import { safeStringify } from '@forklaunch/common';

// Handle Error objects
safeStringify(new Error("test"));
// '{"name":"Error","message":"test","stack":"..."}'

// Handle special types
safeStringify(BigInt(123));          // "123n"
safeStringify(Symbol("test"));       // "Symbol(test)"
safeStringify(() => {});             // "[Function: anonymous]"
safeStringify(new Map([["key", "value"]]));
// '{"__type":"Map","value":[["key","value"]]}'
```

**Type Signature:**
```typescript
function safeStringify(arg: unknown): string
```

### Environment Variables

#### `getEnvVar(name: string)`

Gets an environment variable and casts it to a string. The value will be validated in the bootstrap process.

```typescript
import { getEnvVar } from '@forklaunch/common';

const dbUrl = getEnvVar('DATABASE_URL');
const port = getEnvVar('PORT');
```

**Type Signature:**
```typescript
function getEnvVar(name: string): string
```

### Function Utilities

#### `extractArgumentNames(func: Function)`

Extracts the names of arguments from a function's string representation. Useful for reflection and debugging.

```typescript
import { extractArgumentNames } from '@forklaunch/common';

function example(a, b, { c, d }) {}
const names = extractArgumentNames(example);
// Result: ['a', 'b', '{c,d}']
```

**Type Signature:**
```typescript
function extractArgumentNames(func: { toString(): string }): string[]
```

#### `noop(...args: unknown[])`

A no-operation function that does nothing when called. Commonly used as a default or placeholder function.

```typescript
import { noop } from '@forklaunch/common';

function withCallback(callback = noop) {
  // ... do something
  callback();
}
```

**Type Signature:**
```typescript
function noop(..._args: unknown[]): void
```

### Stream Utilities

#### `readableStreamToAsyncIterable<T>(stream: ReadableStream<T>)`

Converts a ReadableStream to an AsyncIterable for use with async iteration.

```typescript
import { readableStreamToAsyncIterable } from '@forklaunch/common';

const stream = response.body;
for await (const chunk of readableStreamToAsyncIterable(stream)) {
  console.log(chunk);
}
```

**Type Signature:**
```typescript
async function* readableStreamToAsyncIterable<T>(
  stream: ReadableStream<T>
): AsyncIterable<T>
```

#### `InMemoryBlob`

A Blob implementation that stores content in memory.

```typescript
import { InMemoryBlob } from '@forklaunch/common';

const buffer = Buffer.from('Hello World');
const blob = new InMemoryBlob(buffer);
```

**Type Signature:**
```typescript
class InMemoryBlob extends Blob {
  constructor(public content: Buffer<ArrayBuffer>)
}
```

### Type Guards

Type guards help narrow types at runtime with TypeScript type inference support.

#### `isRecord(obj: unknown)`

Checks if the given value is a record (plain object).

```typescript
import { isRecord } from '@forklaunch/common';

isRecord({ a: 1 });      // true
isRecord([1, 2, 3]);     // false
isRecord(null);          // false
```

**Type Signature:**
```typescript
function isRecord(obj: unknown): obj is Record<string, unknown>
```

#### `isAsyncGenerator<T>(value: unknown)`

Checks if a value is an AsyncGenerator.

```typescript
import { isAsyncGenerator } from '@forklaunch/common';

async function* gen() {
  yield 1;
  yield 2;
}

isAsyncGenerator(gen());  // true
isAsyncGenerator([1, 2]); // false
```

**Type Signature:**
```typescript
function isAsyncGenerator<T>(value: unknown): value is AsyncGenerator<T>
```

#### `isNodeJsWriteableStream(value: unknown)`

Checks if a value is a Node.js WritableStream.

```typescript
import { isNodeJsWriteableStream } from '@forklaunch/common';
import { createWriteStream } from 'fs';

const stream = createWriteStream('./file.txt');
isNodeJsWriteableStream(stream); // true
```

**Type Signature:**
```typescript
function isNodeJsWriteableStream(value: unknown): value is NodeJS.WritableStream
```

#### `isTrue(value: true)`

Type guard that checks if a value is exactly `true`. Useful for narrowing boolean types to the literal `true` value.

```typescript
import { isTrue } from '@forklaunch/common';

const value: boolean = true;
if (isTrue(value)) {
  // value is now typed as true (not just boolean)
}
```

**Type Signature:**
```typescript
function isTrue(value: true): boolean
```

#### `isNever(value: never)`

Type guard for the `never` type. Always returns true since this is used for exhaustive checks.

```typescript
import { isNever } from '@forklaunch/common';

function exhaustiveCheck(value: never): never {
  if (isNever(value)) {
    throw new Error(`Unhandled case: ${value}`);
  }
}
```

**Type Signature:**
```typescript
function isNever(value: never): value is never
```

## Type Utilities

ForkLaunch provides advanced TypeScript utility types for better type safety and developer experience.

### `Prettify<T>`

Simplifies the displayed type information in development tools by re-mapping an object type's keys and values.

```typescript
import { Prettify } from '@forklaunch/common';

type Complex = { a: string } & { b: number };
type Simple = Prettify<Complex>;
// Displays as: { a: string; b: number; }
```

### `IdDto`

Type representing a DTO with an `id` field.

```typescript
import { IdDto } from '@forklaunch/common';

const dto: IdDto = { id: '123' };
```

### `IdsDto`

Type representing a DTO with an array of `ids`.

```typescript
import { IdsDto } from '@forklaunch/common';

const dto: IdsDto = { ids: ['123', '456', '789'] };
```

### `RecordTimingDto`

Type representing a DTO with timing information (created and updated timestamps).

```typescript
import { RecordTimingDto } from '@forklaunch/common';

const dto: RecordTimingDto = {
  createdAt: new Date(),
  updatedAt: new Date()
};
```

### `ReturnTypeRecord<T>`

Creates a record of return types from a record of functions.

```typescript
import { ReturnTypeRecord } from '@forklaunch/common';

type Functions = {
  getName: () => string;
  getAge: () => number;
};

type ReturnTypes = ReturnTypeRecord<Functions>;
// Result: { getName: string; getAge: number; }
```

### `InstanceTypeRecord<T>`

Creates a record of instance types from a record of constructors.

```typescript
import { InstanceTypeRecord } from '@forklaunch/common';

type Constructors = {
  User: new () => User;
  Post: new () => Post;
};

type InstanceTypes = InstanceTypeRecord<Constructors>;
// Result: { User: User; Post: Post; }
```

### `FlattenValues<T>`

Produces a union of an object type's value types.

```typescript
import { FlattenValues } from '@forklaunch/common';

type Obj = { a: string; b: number; c: boolean };
type Values = FlattenValues<Obj>;
// Result: string | number | boolean
```

### `FlattenKeys<T>`

Produces a union of an object type's key types.

```typescript
import { FlattenKeys } from '@forklaunch/common';

type Obj = { a: string; b: number; c: boolean };
type Keys = FlattenKeys<Obj>;
// Result: "a" | "b" | "c"
```

### `Flatten<T>`

Recursively flattens an object type.

```typescript
import { Flatten } from '@forklaunch/common';

type Nested = {
  a: {
    b: {
      c: string;
    };
  };
};

type Flat = Flatten<Nested>;
// Recursively flattens nested structure
```

### `ExclusiveRecord<T, U>`

Creates a type where properties in `T` that exist in `U` are set to `never`, making them mutually exclusive.

```typescript
import { ExclusiveRecord } from '@forklaunch/common';

type A = { a: string; b: number };
type B = { b: boolean; c: string };
type Exclusive = ExclusiveRecord<A, B>;
// Result: { a: string; b: never }
```

## Mappers Module

The mappers module provides utilities for data transformation between DTOs (Data Transfer Objects) and domain entities with built-in validation.

### `requestMapper`

Creates a request mapper that transforms and validates DTOs before converting them to entities.

```typescript
import { requestMapper } from '@forklaunch/core/mappers';
import { string, number } from '@forklaunch/validator/zod';

class User {
  constructor(
    public name: string,
    public age: number
  ) {}
}

const userRequestMapper = requestMapper({
  schemaValidator: zodValidator,
  schema: {
    name: string,
    age: number
  },
  entity: User,
  mapperDefinition: {
    toEntity: async (dto) => {
      return new User(dto.name, dto.age);
    }
  }
});

// Usage
const dto = { name: 'John', age: 30 };
const user = await userRequestMapper.toEntity(dto);
// Returns validated User instance
```

**Type Signature:**
```typescript
function requestMapper<
  SV extends AnySchemaValidator,
  DomainSchema extends IdiomaticSchema<SV>,
  Entity,
  AdditionalArgs extends unknown[] = []
>({
  schemaValidator,
  schema,
  entity,
  mapperDefinition
}: {
  schemaValidator: SV;
  schema: DomainSchema;
  entity: Constructor<Entity>;
  mapperDefinition: {
    toEntity: (
      dto: Schema<DomainSchema, SV>,
      ...args: AdditionalArgs
    ) => Promise<Entity>;
  };
}): {
  schema: DomainSchema;
} & typeof mapperDefinition
```

### `responseMapper`

Creates a response mapper that converts entities to validated DTOs.

```typescript
import { responseMapper } from '@forklaunch/core/mappers';
import { string, number } from '@forklaunch/validator/zod';

const userResponseMapper = responseMapper({
  schemaValidator: zodValidator,
  schema: {
    id: string,
    name: string,
    age: number
  },
  entity: User,
  mapperDefinition: {
    toDto: async (entity) => {
      return {
        id: entity.id,
        name: entity.name,
        age: entity.age
      };
    }
  }
});

// Usage
const user = await em.findOne(User, { id: '123' });
const dto = await userResponseMapper.toDto(user);
// Returns validated DTO
```

**Type Signature:**
```typescript
function responseMapper<
  SV extends AnySchemaValidator,
  DomainSchema extends IdiomaticSchema<SV>,
  Entity,
  AdditionalArgs extends unknown[] = []
>({
  schemaValidator,
  schema,
  entity,
  mapperDefinition
}: {
  schemaValidator: SV;
  schema: DomainSchema;
  entity: Constructor<Entity>;
  mapperDefinition: {
    toDto: (
      entity: Entity,
      ...args: AdditionalArgs
    ) => Promise<Schema<DomainSchema, SV>>;
  };
}): Prettify<{
  schema: DomainSchema;
} & typeof mapperDefinition>
```

### `mapServiceSchemas`

Maps a set of service schema factories or pre-instantiated schemas to their resolved schemas using provided arguments.

```typescript
import { mapServiceSchemas } from '@forklaunch/core/mappers';

const schemas = {
  UserSchemas: (opts) => createUserSchemas(opts),
  ProductSchemas: (opts) => createProductSchemas(opts),
  AlreadyInstantiated: someSchemaObject
};

const mapped = mapServiceSchemas(schemas, {
  validator: myValidator,
  uuidId: true
});

// mapped.UserSchemas and mapped.ProductSchemas are instantiated
// mapped.AlreadyInstantiated is passed through as-is
```

**Type Signature:**
```typescript
function mapServiceSchemas<
  SV extends AnySchemaValidator,
  T extends Record<string, SchemaResolutionFunction<Args> | IdiomaticSchema<SV>>,
  Args extends Record<string, unknown>
>(
  schemas: T,
  args: Args
): {
  [K in keyof T]: T[K] extends SchemaResolutionFunction<Args>
    ? ReturnType<T[K]>
    : T[K];
}
```

## Persistence Module

The persistence module provides a `BaseEntity` class with convenient static methods for entity creation, updating, and reading.

### `BaseEntity`

Extends MikroORM's BaseEntity to provide convenience methods for CRUD operations.

```typescript
import { BaseEntity } from '@forklaunch/core/persistence';
import { Entity, PrimaryKey, Property } from '@mikro-orm/core';

@Entity()
class User extends BaseEntity {
  @PrimaryKey()
  id!: string;

  @Property()
  name!: string;

  @Property()
  email!: string;

  @Property()
  createdAt = new Date();

  @Property({ onUpdate: () => new Date() })
  updatedAt = new Date();
}
```

#### `BaseEntity.create(data, em?, ...constructorArgs)`

Creates a new entity instance with the given data. If an EntityManager is provided, it calls `em.create()` for proper ORM initialization.

```typescript
// Without EntityManager (simple instantiation)
const user = await User.create({
  id: '123',
  name: 'John Doe',
  email: 'john@example.com'
});

// With EntityManager (ORM-managed)
const user = await User.create(
  {
    id: '123',
    name: 'John Doe',
    email: 'john@example.com'
  },
  em
);
```

**Type Signature:**
```typescript
static async create<T extends BaseEntity>(
  this: Constructor<T>,
  data: RequiredEntityData<T>,
  em?: EntityManager,
  ...constructorArgs: ConstructorParameters<Constructor<T>>
): Promise<T>
```

#### `BaseEntity.update(data, em?, ...constructorArgs)`

Updates an existing entity instance with the given data. Must include an `id` field to identify the entity.

```typescript
// Without EntityManager
const user = await User.update({
  id: '123',
  name: 'Jane Doe'
});

// With EntityManager (fetches and updates existing entity)
const user = await User.update(
  {
    id: '123',
    name: 'Jane Doe'
  },
  em
);
await em.flush(); // Persist changes
```

**Type Signature:**
```typescript
static async update<T extends BaseEntity>(
  this: Constructor<T>,
  data: EntityData<T> & { id: unknown },
  em?: EntityManager,
  ...constructorArgs: ConstructorParameters<Constructor<T>>
): Promise<T>
```

#### `entity.read(em?)`

Reads the entity, initializing it if necessary, and returns its DTO (plain object) representation.

```typescript
const user = await em.findOne(User, { id: '123' });
const userDto = await user.read(em);
// Returns: { id: '123', name: 'John Doe', email: 'john@example.com', ... }
```

**Type Signature:**
```typescript
async read(em?: EntityManager): Promise<EntityDTO<this>>
```

## Environment Module

The environment module provides cascading environment variable loading, supporting multiple `.env.local` files from the application root down to the project directory.

### `loadCascadingEnv(projectEnvPath?, projectRoot?)`

Loads environment variables with cascading precedence: all `.env.local` files from root to project, then the project-specific env file.

```typescript
import { loadCascadingEnv } from '@forklaunch/core/environment';

const result = loadCascadingEnv('.env.development', process.cwd());

console.log(result);
// {
//   rootEnvLoaded: true,
//   projectEnvLoaded: true,
//   rootEnvPath: '/path/to/root/.env.local',
//   projectEnvFilePath: '/path/to/project/.env.development',
//   envFilesLoaded: ['/path/to/root/.env.local', '/path/to/project/.env.local', '/path/to/project/.env.development'],
//   totalEnvFilesLoaded: 3
// }
```

**How it works:**

1. Finds the application root by looking for `.forklaunch/manifest.toml`
2. Collects all `.env.local` files from root to project directory
3. Loads env files in order (root to project) with later values overriding earlier ones
4. Finally loads the project-specific env file if specified

**Type Signature:**
```typescript
function loadCascadingEnv(
  projectEnvPath: string | undefined,
  projectRoot?: string
): {
  rootEnvLoaded: boolean;
  projectEnvLoaded: boolean;
  rootEnvPath?: string;
  projectEnvFilePath?: string;
  envFilesLoaded: string[];
  totalEnvFilesLoaded: number;
}
```

### `getCascadingEnvPaths(projectEnvPath?, projectRoot?)`

Gets cascading environment file paths without loading them. Useful for inspection or custom loading logic.

```typescript
import { getCascadingEnvPaths } from '@forklaunch/core/environment';

const paths = getCascadingEnvPaths('.env.development', process.cwd());

console.log(paths);
// {
//   rootEnvExists: true,
//   projectEnvExists: true,
//   rootEnvPath: '/path/to/root/.env.local',
//   projectEnvFilePath: '/path/to/project/.env.development',
//   loadOrder: ['/path/to/root/.env.local', '/path/to/project/.env.local', '/path/to/project/.env.development']
// }
```

**Type Signature:**
```typescript
function getCascadingEnvPaths(
  projectEnvPath: string | undefined,
  projectRoot?: string
): {
  rootEnvExists: boolean;
  projectEnvExists: boolean;
  rootEnvPath?: string;
  projectEnvFilePath?: string;
  loadOrder: string[];
}
```

## Best Practices

### String Manipulation

1. **Use `toPrettyCamelCase` for user-facing identifiers** - Handles abbreviations more intuitively
2. **Use `toCamelCaseIdentifier` for code generation** - Preserves exact casing of parts
3. **Always validate with `isValidIdentifier`** - Ensure generated identifiers are valid before use

### Object Utilities

1. **Use `deepCloneWithoutUndefined` for deep cloning** - Handles circular references and preserves prototypes
2. **Use `stripUndefinedProperties` for shallow cleanup** - Faster than deep clone when you only need shallow cleanup
3. **Use `sortObjectKeys` for consistent serialization** - Ensures deterministic output for comparison and hashing

### Type Guards

1. **Always use type guards for runtime validation** - Leverage TypeScript's type narrowing
2. **Combine guards with validation** - Use guards first for quick checks, then validate with schemas
3. **Use `isRecord` before accessing properties** - Prevent runtime errors with unknown types

### Mappers

1. **Always define schemas once and reuse** - Use the same schema for request/response mappers
2. **Validate at boundaries** - Use mappers at HTTP endpoints, not internal logic
3. **Keep mappers simple** - Complex transformations should be in separate services

### Persistence

1. **Always use EntityManager when possible** - Ensures proper ORM tracking and transactions
2. **Use `read()` for safe serialization** - Returns plain objects without ORM metadata
3. **Extend BaseEntity for all entities** - Provides consistent CRUD interface

### Environment

1. **Use cascading env loading in monorepos** - Share common config at root, override in projects
2. **Keep sensitive data in .env.local** - Never commit `.env.local` files to version control
3. **Validate environment variables** - Use `getEnvVar` and validate in bootstrap

## Performance Considerations

### String Manipulation
- `toCamelCaseIdentifier` and `toPrettyCamelCase` use regex and string operations - cache results for repeated conversions
- `isValidIdentifier` is fast - use for validation without performance concerns

### Object Utilities
- `deepCloneWithoutUndefined` is expensive for large objects - consider shallow clones when possible
- `sortObjectKeys` is recursive - cache results for large objects used in comparisons

### Type Guards
- All type guards are constant time operations (O(1)) - safe to use in hot paths
- `isRecord` is the fastest check for object types

### Mappers
- Validation adds overhead - consider caching validated results when safe
- `mapServiceSchemas` is called once at startup - no runtime performance impact

### Environment
- `loadCascadingEnv` reads from filesystem - call once at startup
- Use `getCascadingEnvPaths` for inspection without loading
