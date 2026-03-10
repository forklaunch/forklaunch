---
title: Framework - Validation
category: References
description: Reference for using Validation and Coercion in ForkLaunch.
---

## Overview

`@forklaunch/validator` wraps common validation libraries and simplifies validation and coercion for ForkLaunch applications. Validation schemas are used throughout the framework for:

- **HTTP request/response validation** in [ContractDetails](/docs/framework/http.md)
- **Automatic OpenAPI generation** for [documentation](/docs/framework/documentation.md)
- **MCP tool parameter validation** for [MCP autogeneration](/docs/framework/mcp.md)
- **Type-safe API contracts** with the [Universal SDK](/docs/framework/universal-sdk.md)

Available validators include:

- [Zod](https://zod.dev/) - Recommended for full framework compatibility
- [TypeBox](https://github.com/sinclairzx81/typebox) - Lightweight alternative

> **Note**: MCP autogeneration currently requires `zod` validator choice.

### Schema Types

#### Primitives

- `string`: validates input is a string and coerces to string
- `number`: validates input is a number and coerces to number
- `boolean`: validates input is a boolean value and coerces to boolean
- `bigint`: validates input conforms to BigInt requirements and coerces to BigInt
- `date`: validates input is a valid date string/timestamp and coerces to Date object
- `symbol`: validates input is a symbol and coerces to Symbol

#### String Formats

- `uuid`: validates input conforms to uuid format and coerces to string
- `email`: validates input conforms to email format and coerces to string
- `uri`: validates input conforms to uri format and coerces to string

#### Special Types

- `nullish`: validates input is null or undefined
- `any`: accepts any input value without validation
- `unknown`: accepts any input value but requires type checking before use
- `never`: validates that no value can satisfy this type (useful for exhaustive checks)

#### Schema Functions

- `optional(schema)`: marks schema as optional
- `array(schema)`: creates array of given schema
- `union(schemas)`: creates union of multiple schemas
- `literal(value)`: creates literal type schema
- `enum_(enumValue)`: creates enum schema

### Complex Objects

Objects can be defined using natural TypeScript syntax:

```typescript
enum SomeEnum {
  enumValue1,
  enumValue2
}

const complexSchema = {
  a: string,
  b: {
    u: union([boolean, number]),
    l: literal('l')
  },
  e: enum_(SomeEnum)
};
```

### Operations

- `validate(schema, value)`: Returns boolean indicating if value matches schema
- `parse(schema, value)`: Validates and coerces value to schema type
- `openapi(schema)`: Generates OpenAPI definition from schema

```typescript
import { string, parse } from '@forklaunch/validator/<validatorType>';

const schema = string;
const result = parse(schema, '123');

if (!result.ok) {
  throw new Error('Parse failed');
} else {
  console.log(result.value);
}
```

### Best Practices

1. **Use the most specific schema type available** - Prefer `email` over `string` for email fields
2. **Prefer `parse` over `validate`** - Ensures type coercion and safer data handling
3. **Use `union` types when multiple types are valid** - Better than `any` for flexibility
4. **Use `literal` types for exact value matching** - Ensures precise validation
5. **Define schemas once, reuse everywhere** - Same schema for HTTP, docs, and MCP

## Integration Examples

### HTTP Framework Integration

```typescript
import { string, number } from '@forklaunch/validator/zod';

app.post(
  '/users',
  {
    name: 'Create User',
    body: {
      name: string,
      age: number
    },
    responses: {
      201: { id: number, name: string }
    }
  },
  handler
);
```

### Universal SDK Usage

```typescript
// Same schemas provide type safety in SDK
const user = await sdk.post('/users', {
  body: { name: 'John', age: 30 } // Fully typed
});
```

## Related Documentation

- **[HTTP Frameworks](/docs/framework/http.md)** - Using schemas in ContractDetails
- **[Documentation](/docs/framework/documentation.md)** - Automatic OpenAPI generation from schemas
- **[Universal SDK](/docs/framework/universal-sdk.md)** - Type-safe client requests
- **[MCP Autogeneration](/docs/framework/mcp.md)** - Tool parameter validation (zod only)
