---
title: Framework - Auto Documentation and OpenAPI
category: References
description: Reference for Auto Documentation and OpenAPI generation in ForkLaunch.
---

## Overview

ForkLaunch automatically generates comprehensive API documentation from your contract definitions. This means:

- No manual OpenAPI/Swagger maintenance
- Documentation stays in sync with your code
- Type-safe contracts that generate accurate API specs
- Interactive API testing through built-in documentation UI

When running a single service, the generated documentation is available at `http://localhost:8000/docs` by default.

### Documentation UI Options
- **Scalar** (default): Modern, feature-rich API explorer with enhanced testing capabilities. See [HTTP Framework > Application](/docs/framework/http.md#Application) for applying configuration.
- **Swagger UI**: Traditional OpenAPI interface, familiar to most developers
- **None**: Disable UI, serve only OpenAPI JSON for consumption by other tools

### Configuration

Configure documentation through environment variables:
```bash
VERSION=v1        # OpenAPI documentation title
DOCS_PATH=/docs   # Documentation endpoint path
```

## OpenAPI Generation

Contract details are automatically converted to OpenAPI specifications. Here's an example:

```typescript
// Your Contract Details
const apiContract = {
  name: "Create Message API",
  summary: "Create a new message with nested data",
  
  // Request Headers
  headers: {
    "x-header-name": string
  },
  
  // URL Parameters
  params: {
    id: number
  },
  
  // Request Body
  body: {
    message: string,
    nestedObject: {
      anotherNestedObject: {
        sweet: boolean
      },
      justAString: string,
      justANumber: number,
    }
  },
  
  // Response Types
  responses: {
    200: {
      returnMessage: string,
      metadata: {
        items: array(union(string, number, boolean)),
        timestamp: date
      }
    },
    301: union(string, number)
  },
};
```

This automatically generates:

```json
{
  "parameters": [
    {
      "name": "id",
      "in": "path",
      "required": true,
      "schema": {
        "type": "number"
      }
    },
    {
      "name": "x-header-name",
      "in": "header",
      "required": true,
      "schema": {
        "type": "string"
      }
    }
  ],
  "requestBody": {
    "required": true,
    "content": {
      "application/json": {
        "schema": {
          "type": "object",
          "properties": {
            "message": {
              "type": "string"
            },
            "nestedObject": {
              "type": "object",
              "properties": {
                "anotherNestedObject": {
                  "type": "object",
                  "properties": {
                    "sweet": {
                      "type": "boolean"
                    }
                  }
                },
                "justAString": {
                  "type": "string"
                },
                "justANumber": {
                  "type": "number"
                }
              }
            }
          }
        }
      }
    }
  },
  "responses": {
    "200": {
      "description": "Successful response",
      "content": {
        "application/json": {
          "schema": {
            "type": "object",
            "properties": {
              "returnMessage": {
                "type": "string"
              },
              "metadata": {
                "type": "object",
                "properties": {
                  "items": {
                    "type": "array",
                    "items": {
                      "oneOf": [
                        { "type": "string" },
                        { "type": "number" },
                        { "type": "boolean" }
                      ]
                    }
                  },
                  "timestamp": {
                    "type": "string",
                    "format": "date-time"
                  }
                }
              }
            }
          }
        }
      }
    },
    "301": {
      "description": "Redirect response",
      "content": {
        "application/json": {
          "schema": {
            "oneOf": [
              { "type": "string" },
              { "type": "number" }
            ]
          }
        }
      }
    }
  }
}
```

## Best Practices

1. Provide clear names and summaries for each endpoint
2. Use descriptive parameter names
3. Document all possible response codes
4. Include example values where helpful

## Coming Soon

- Enhanced schema generation capabilities