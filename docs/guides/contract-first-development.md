---
title: Contract-First Development
category: Guides
description: Define API contracts before implementation using ForkLaunch's type-safe schema validation and OpenAPI generation.
---

## Overview

Contract-first development means defining your API contracts before implementing them. ForkLaunch enforces contracts through **typed route handlers** with built-in validation, ensuring type safety across all services and automatically generating OpenAPI documentation.

## Why Contract-First?

### Benefits

- **Type Safety** - Catch errors at compile time with TypeScript
- **Automatic Validation** - Requests/responses validated against schemas
- **Living Documentation** - OpenAPI specs generated from contracts
- **Breaking Change Detection** - Schema changes surface type errors in callers
- **Team Coordination** - Frontend and backend work in parallel with shared contracts

### Traditional Approach (Code-First)

```typescript
// Implementation first, no contract
app.post('/users', async (req, res) => {
  const user = await db.users.create({
    name: req.body.name,      // What if name is missing?
    email: req.body.email,    // What if email is invalid?
    age: req.body.age         // What if age is a string?
  });
  res.json(user);
});
```

Problems:
- No validation
- No type safety
- No documentation
- Breaking changes are silent

### Contract-First with ForkLaunch

```typescript
import { handlers, schemaValidator, string, optional } from '@forklaunch-platform/core';

// 1. Define contract inline with handler
export const userPost = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'User Post',
    summary: 'Create a new user',
    body: {
      name: string,
      email: string,
      age: optional(string)
    },
    responses: {
      200: {
        id: string,
        name: string,
        email: string,
        createdAt: string
      }
    }
  },
  async (req, res) => {
    // req.body is typed automatically
    const user = await db.users.create(req.body);
    res.json(user);
  }
);
```

Benefits:
- Automatic request/response validation
- Full TypeScript type inference
- OpenAPI generation from contracts
- Breaking changes caught at compile time

## Defining Contracts

### Contract Details in Routes

ForkLaunch routes use contract details to define their API contract:

```typescript
import {
  handlers,
  schemaValidator,
  string,
  boolean,
  optional,
  array
} from '@forklaunch-platform/core';

export const userPost = handlers.post(
  schemaValidator,        // Schema validator instance
  '/',                    // Route path
  {
    // Contract Details
    name: 'User Post',
    summary: 'Creates a new user',

    // Request body schema - plain object with Zod types
    body: {
      name: string,
      email: string,
      age: optional(string),
      preferences: optional({
        newsletter: boolean,
        notifications: boolean
      })
    },

    // Response schemas by status code
    responses: {
      200: {
        id: string,
        name: string,
        email: string,
        createdAt: string
      },
      400: {
        error: string,
        details: optional(array(string))
      }
    }
  },
  async (req, res) => {
    // Handler implementation
    const user = await userService.create(req.body);
    res.json(user);
  }
);
```

### Schema Types

ForkLaunch uses Zod for schema validation. Import schema types from `@forklaunch-platform/core`:

```typescript
import {
  string,
  number,
  boolean,
  array,
  union,
  literal,
  optional,
  type
} from '@forklaunch-platform/core';

// Primitive types
const nameSchema = string;
const ageSchema = number;
const activeSchema = boolean;

// Optional fields
const phoneSchema = optional(string);

// Objects - plain JavaScript objects with Zod types
const addressSchema = {
  street: string,
  city: string,
  state: string,
  zip: string
};

// Arrays
const tagsSchema = array(string);

// Unions
const roleSchema = union([
  literal('user'),
  literal('admin'),
  literal('moderator')
]);

// Type inference works with Zod schemas
// The ForkLaunch framework automatically infers TypeScript types from schemas
```

### Complete Route Example

```typescript
// api/controllers/user.controller.ts
import {
  handlers,
  schemaValidator,
  string,
  boolean,
  array,
  union,
  literal,
  optional
} from '@forklaunch-platform/core';

// Request schema - plain object with Zod types
const UserRequestSchema = {
  name: string,
  email: string,
  age: optional(string),
  role: union([literal('user'), literal('admin')]),
  preferences: optional({
    newsletter: boolean,
    notifications: boolean
  })
};

// Response schema - plain object with Zod types
const UserResponseSchema = {
  id: string,
  name: string,
  email: string,
  age: optional(string),
  role: string,
  createdAt: string,
  updatedAt: string
};

// Create user route
export const userPost = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'User Post',
    summary: 'Create a new user',
    body: UserRequestSchema,
    responses: {
      200: UserResponseSchema,
      400: { error: string }
    }
  },
  async (req, res) => {
    const user = await userService.userPost(req.body);
    res.json(user);
  }
);

// Get user route
export const userGet = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'User Get',
    summary: 'Get user by ID',
    params: {
      id: string
    },
    query: {
      include: optional(array(string))
    },
    responses: {
      200: UserResponseSchema,
      404: { error: string }
    }
  },
  async (req, res) => {
    const user = await userService.userGet(
      req.params.id,
      req.query.include
    );
    if (!user) {
      res.status(404).json({ error: 'User not found' });
      return;
    }
    res.json(user);
  }
);

// Update user route
export const userPatch = handlers.patch(
  schemaValidator,
  '/:id',
  {
    name: 'User Patch',
    summary: 'Update user',
    params: {
      id: string
    },
    body: {
      name: optional(string),
      email: optional(string),
      preferences: optional({
        newsletter: boolean,
        notifications: boolean
      })
    },
    responses: {
      200: UserResponseSchema,
      404: { error: string }
    }
  },
  async (req, res) => {
    const user = await userService.userPatch(req.params.id, req.body);
    res.json(user);
  }
);

// Delete user route
export const userDelete = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'User Delete',
    summary: 'Delete user',
    params: {
      id: string
    },
    responses: {
      200: string,
      404: { error: string }
    }
  },
  async (req, res) => {
    await userService.userDelete(req.params.id);
    res.status(200).send(`Deleted user ${req.params.id}`);
  }
);
```

## Validation Modes

ForkLaunch supports validation configuration at the route level:

```typescript
import { handlers, schemaValidator } from '@forklaunch-platform/core';

export const userPost = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'User Post',
    summary: 'Create a new user',
    options: {
      requestValidation: 'error',   // 'error' | 'warning' | 'none'
      responseValidation: 'warning' // 'error' | 'warning' | 'none'
    },
    body: { /* ... */ },
    responses: { /* ... */ }
  },
  async (req, res) => {
    // Handler implementation
  }
);
```

### Validation Modes:

- **`'error'`** (Production) - Invalid requests/responses throw errors (400/500)
- **`'warning'`** (Development) - Invalid data logged as warnings, allows through
- **`'none'`** (Testing) - No validation performed

## OpenAPI Generation

ForkLaunch automatically generates OpenAPI 3.1.0 specifications from your route contracts.

### OpenAPI Generator

```typescript
import { generateOpenApiV3 } from '@forklaunch/core/http';
import { userPost, userGet, userPatch, userDelete } from './controllers/user.controller';

const openApiSpec = generateOpenApiV3({
  info: {
    title: 'Users API',
    version: '1.0.0',
    description: 'User management API'
  },
  servers: [
    { url: 'https://api.example.com', description: 'Production' },
    { url: 'http://localhost:3000', description: 'Development' }
  ],
  routes: [
    userPost,
    userGet,
    userPatch,
    userDelete
  ]
});

// Export as JSON or YAML
import fs from 'fs';
fs.writeFileSync('openapi.json', JSON.stringify(openApiSpec, null, 2));
```

### Generated OpenAPI Example

```yaml
openapi: 3.1.0
info:
  title: Users API
  version: 1.0.0
  description: User management API

servers:
  - url: https://api.example.com
    description: Production
  - url: http://localhost:3000
    description: Development

paths:
  /users:
    post:
      operationId: User Post
      summary: Create a new user
      tags:
        - Users
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                name:
                  type: string
                email:
                  type: string
                age:
                  type: number
                role:
                  type: string
                  enum: [user, admin]
              required:
                - name
                - email
                - role
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
                  name:
                    type: string
                  email:
                    type: string
                  createdAt:
                    type: string
                    format: date-time

  /users/{id}:
    get:
      operationId: User Get
      summary: Get user by ID
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
        - name: include
          in: query
          required: false
          schema:
            type: array
            items:
              type: string
      responses:
        '200':
          description: User found
        '404':
          description: User not found
```

## Type Inference

TypeScript automatically infers types from Zod schemas in ForkLaunch handlers:

```typescript
import { handlers, schemaValidator, string, optional } from '@forklaunch-platform/core';

const UserSchema = {
  name: string,
  email: string,
  age: optional(string)
};

// Use in handler
export const userPost = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'User Post',
    body: UserSchema,
    responses: { 200: UserSchema }
  },
  async (req, res) => {
    // req.body is automatically typed based on the schema
    const name: string = req.body.name;           // ✓
    const age: string | undefined = req.body.age; // ✓
    const invalid = req.body.invalid;             // ✗ Type error
  }
);
```

## Versioning Contracts

### Non-Breaking Changes (Add Optional Fields)

```typescript
import { string, optional } from '@forklaunch-platform/core';

// v1 (existing)
const UserSchemaV1 = {
  name: string,
  email: string
};

// v2 (backwards compatible - add optional field)
const UserSchemaV2 = {
  name: string,
  email: string,
  phone: optional(string)  // New optional field
};

// Old clients still work
const v1User = { name: 'John', email: 'john@example.com' };
// ✓ Valid for both V1 and V2
```

### Breaking Changes (Separate Endpoints)

```typescript
// V1 endpoint
export const userPostV1 = handlers.post(
  schemaValidator,
  '/v1/users',
  {
    name: 'User Post V1',
    body: UserSchemaV1,
    responses: { 200: UserResponseV1 }
  },
  handlerV1
);

// V2 endpoint (breaking change)
export const userPostV2 = handlers.post(
  schemaValidator,
  '/v2/users',
  {
    name: 'User Post V2',
    body: UserSchemaV2,  // Different schema
    responses: { 200: UserResponseV2 }
  },
  handlerV2
);
```

## Query and Path Parameters

```typescript
import {
  handlers,
  schemaValidator,
  string,
  array,
  union,
  literal,
  optional
} from '@forklaunch-platform/core';

export const userSearch = handlers.get(
  schemaValidator,
  '/search',
  {
    name: 'User Search',
    summary: 'Search users',
    query: {
      q: string,                       // Required query param
      limit: optional(string),         // Optional query param
      offset: optional(string),
      sort: optional(union([
        literal('name'),
        literal('email'),
        literal('createdAt')
      ]))
    },
    responses: {
      200: {
        users: array(UserResponseSchema),
        total: string,
        limit: string,
        offset: string
      }
    }
  },
  async (req, res) => {
    // req.query is typed
    const { q, limit = '10', offset = '0', sort } = req.query;

    const results = await userService.search({
      query: q,
      limit: parseInt(limit),
      offset: parseInt(offset),
      sort
    });

    res.json(results);
  }
);

export const userGetById = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'User Get By ID',
    params: {
      id: string  // Path parameter
    },
    responses: {
      200: UserResponseSchema,
      404: { error: string }
    }
  },
  async (req, res) => {
    // req.params.id is typed as string
    const user = await userService.getById(req.params.id);
    if (!user) {
      res.status(404).json({ error: 'User not found' });
      return;
    }
    res.json(user);
  }
);
```

## Headers and Authentication

ForkLaunch provides built-in authentication support with JWT, HMAC, and basic auth:

```typescript
import {
  handlers,
  schemaValidator,
  string,
  array,
  optional,
  PLATFORM_VIEWER_ROLES
} from '@forklaunch-platform/core';

// JWT Authentication Example
export const protectedRoute = handlers.get(
  schemaValidator,
  '/protected',
  {
    name: 'Protected Route',
    summary: 'Requires JWT authentication',
    auth: {
      jwt: {
        jwksPublicKeyUrl: process.env.JWKS_PUBLIC_KEY_URL
      },
      allowedRoles: PLATFORM_VIEWER_ROLES,
      sessionSchema: {
        sub: string,
        roles: optional(array(string))
      }
    },
    responses: {
      200: { data: string },
      401: { error: string }
    }
  },
  async (req, res) => {
    // req.session is typed based on sessionSchema
    const userId = req.session.sub;
    res.json({ data: `Protected data for user ${userId}` });
  }
);

// HMAC Authentication Example
export const hmacRoute = handlers.post(
  schemaValidator,
  '/webhook',
  {
    name: 'Webhook Handler',
    summary: 'HMAC authenticated webhook',
    auth: {
      hmac: {
        secretKeys: {
          default: process.env.HMAC_SECRET_KEY
        }
      }
    },
    body: { event: string, data: string },
    responses: {
      200: string
    }
  },
  async (req, res) => {
    // HMAC signature already verified by framework
    res.status(200).send('Webhook received');
  }
);
```

## Error Handling

Define error response schemas for different status codes:

```typescript
import {
  handlers,
  schemaValidator,
  string,
  array,
  optional
} from '@forklaunch-platform/core';

const ErrorSchema = {
  error: string,
  code: string,
  details: optional(array({
    field: string,
    message: string
  }))
};

export const userPost = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'User Post',
    body: UserRequestSchema,
    responses: {
      200: UserResponseSchema,
      400: ErrorSchema,  // Validation error
      409: ErrorSchema,  // Conflict error
      500: ErrorSchema   // Server error
    }
  },
  async (req, res) => {
    try {
      const user = await userService.create(req.body);
      res.json(user);
    } catch (error) {
      if ((error as any).code === 'DUPLICATE_EMAIL') {
        res.status(409).json({
          error: 'Email already exists',
          code: 'DUPLICATE_EMAIL'
        });
      } else {
        res.status(500).json({
          error: 'Internal server error',
          code: 'INTERNAL_ERROR'
        });
      }
    }
  }
);
```

## Testing Contracts

### Contract Validation Tests

```typescript
import { schemaValidator } from '@forklaunch-platform/core';
import { UserRequestSchema } from '@/api/controllers/user.controller';

describe('User Contract', () => {
  test('valid request passes validation', () => {
    const validRequest = {
      name: 'John Doe',
      email: 'john@example.com',
      role: 'user'
    };

    const result = schemaValidator.validate(UserRequestSchema, validRequest);
    expect(result.success).toBe(true);
  });

  test('invalid email fails validation', () => {
    const invalidRequest = {
      name: 'John Doe',
      email: 'not-an-email',
      role: 'user'
    };

    const result = schemaValidator.validate(UserRequestSchema, invalidRequest);
    expect(result.success).toBe(false);
  });

  test('missing required field fails validation', () => {
    const invalidRequest = {
      email: 'john@example.com',
      role: 'user'
      // Missing 'name'
    };

    const result = schemaValidator.validate(UserRequestSchema, invalidRequest);
    expect(result.success).toBe(false);
  });
});
```

### Integration Tests

```typescript
import request from 'supertest';
import { app } from '@/app';

describe('POST /users', () => {
  test('creates user with valid data', async () => {
    const response = await request(app)
      .post('/users')
      .send({
        name: 'John Doe',
        email: 'john@example.com',
        role: 'user'
      });

    expect(response.status).toBe(200);
    expect(response.body).toMatchObject({
      name: 'John Doe',
      email: 'john@example.com'
    });
  });

  test('rejects invalid email', async () => {
    const response = await request(app)
      .post('/users')
      .send({
        name: 'John Doe',
        email: 'invalid-email',
        role: 'user'
      });

    expect(response.status).toBe(400);
    expect(response.body).toHaveProperty('error');
  });
});
```

## Best Practices

### 1. Define Contracts Inline with Routes

```typescript
// ✅ Good: Contract is part of route definition
export const userPost = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'User Post',
    body: UserRequestSchema,
    responses: { 200: UserResponseSchema }
  },
  handler
);

// ❌ Bad: No contract definition
export const userPost = handlers.post(
  schemaValidator,
  '/',
  {},  // Empty contract
  handler
);
```

### 2. Use Descriptive Names

```typescript
// ✅ Good: Clear operation names
{
  name: 'User Post',
  summary: 'Create a new user',
  description: 'Creates a new user account with email verification'
}

// ❌ Bad: Generic names
{
  name: 'Post',
  summary: 'Post endpoint'
}
```

### 3. Document Response Status Codes

```typescript
// ✅ Good: All possible responses documented
{
  responses: {
    200: SuccessSchema,
    400: ValidationErrorSchema,
    401: UnauthorizedSchema,
    404: NotFoundSchema,
    500: ServerErrorSchema
  }
}

// ❌ Bad: Only success case
{
  responses: {
    200: SuccessSchema
  }
}
```

### 4. Keep Schemas Reusable

```typescript
import { string } from '@forklaunch-platform/core';

// ✅ Good: Define schemas separately for reuse
const UserResponseSchema = {
  id: string,
  name: string,
  email: string
};

export const userGet = handlers.get(/* ... uses UserResponseSchema */);
export const userPost = handlers.post(/* ... uses UserResponseSchema */);

// ❌ Bad: Duplicate schema definitions
export const userGet = handlers.get(
  schemaValidator,
  '/',
  {
    responses: {
      200: { id: string, name: string, email: string }  // Duplicated
    }
  }
);
```

### 5. Use Type Inference

```typescript
import { string } from '@forklaunch-platform/core';

// ✅ Good: Let TypeScript and ForkLaunch infer types from schemas
const UserSchema = {
  name: string,
  email: string
};
// Type is automatically inferred in handler request/response types

// ❌ Bad: Manually defining types alongside schemas
const UserSchema = { name: string, email: string };
interface User {  // Duplicates schema
  name: string;
  email: string;
}
```

## Real-World Example: Billing Module

Here's a complete example from the ForkLaunch platform's billing module showing real patterns in production:

```typescript
import {
  handlers,
  schemaValidator,
  string,
  array,
  IdSchema,
  IdsSchema
} from '@forklaunch-platform/core';

// Using mappers for schema validation
import {
  CreatePlanMapper,
  PlanMapper,
  UpdatePlanMapper
} from '../../domain/mappers/plan.mappers';
import { PlanSchemas } from '../../domain/schemas';

// HMAC-authenticated create endpoint
export const createPlan = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Plan',
    summary: 'Create a plan',
    auth: {
      hmac: {
        secretKeys: {
          default: process.env.HMAC_SECRET_KEY
        }
      }
    },
    body: CreatePlanMapper.schema,
    responses: {
      200: PlanMapper.schema
    }
  },
  async (req, res) => {
    res.status(200).json(await planService.createPlan(req.body));
  }
);

// Path parameter example
export const getPlan = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Plan',
    summary: 'Get a plan',
    auth: {
      hmac: {
        secretKeys: {
          default: process.env.HMAC_SECRET_KEY
        }
      }
    },
    params: IdSchema,  // Reusable schema: { id: string }
    responses: {
      200: PlanSchemas.PlanSchema
    }
  },
  async (req, res) => {
    res.status(200).json(await planService.getPlan(req.params));
  }
);

// Query parameter example
export const listPlans = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'List Plans',
    summary: 'List plans',
    query: IdsSchema,  // Reusable schema: { ids: array(string) }
    auth: {
      hmac: {
        secretKeys: {
          default: process.env.HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: array(PlanSchemas.PlanSchema)
    }
  },
  async (req, res) => {
    res.status(200).json(await planService.listPlans(req.query));
  }
);

// Delete endpoint returning string
export const deletePlan = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Delete Plan',
    summary: 'Delete a plan',
    params: IdSchema,
    auth: {
      hmac: {
        secretKeys: {
          default: process.env.HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: string
    }
  },
  async (req, res) => {
    await planService.deletePlan(req.params);
    res.status(200).send(`Deleted plan ${req.params.id}`);
  }
);
```

### Key Patterns in This Example:

1. **Reusable Schemas**: `IdSchema` and `IdsSchema` are defined once and reused across controllers
2. **Mapper Schemas**: Complex validation logic is encapsulated in mappers with `.schema` property
3. **Authentication**: HMAC auth configured at the route level with secret keys
4. **Type Safety**: All request/response types are inferred from schemas automatically
5. **OpenAPI Ready**: These routes automatically generate OpenAPI documentation

## Related Documentation

- [AsyncAPI Generation](/docs/guides/asyncapi.md)
- [Dependency Management](/docs/guides/dependency-management.md)
- [Infrastructure Overview](/docs/infrastructure/overview.md)
