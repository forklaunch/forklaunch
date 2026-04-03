---
title: Framework - HTTP Frameworks
category: References
description: Reference for using HTTP Frameworks in ForkLaunch.
---

## Overview

`ForkLaunch` is built directly on top of the `express` syntax, allowing for a near drop-in replacement for your express applications. The net new difference is the requirement of a `ContractDetails` object, which enables:
- Automatic schema validation and type coercion
- Automatic API documentation generation
- Built-in telemetry collection
- Standardized error handling

By default, `ForkLaunch` ships two flavors of HTTP frameworks:

- `@forklaunch/express`: Compatible with `express.js` 4.0. As close to the standard framework in `node` development. Compatible with `bun` and `node` 18+.
- `@forklaunch/hyper-express`: A `uwebsockets`-based framework that is faster and more efficient than `express.js` but still maintains a similar API.

There are plans to support `express 5.0` as well as part of a separate package.

## Application

`forklaunchExpress` virtually acts as a drop-in replacement for `express`, with additional initialization arguments:

### Express Framework (`@forklaunch/express`)

```typescript
import { forklaunchExpress } from '@forklaunch/express';

const app = forklaunchExpress(
  schemaValidator,
  openTelemetryCollector,
  options?: {
    docs?: DocsConfiguration;
    busboy?: BusboyConfig;
    text?: OptionsText;
    json?: OptionsJson;
    urlencoded?: OptionsUrlencoded;
    cors?: CorsOptions;
    mcp?: McpConfiguration; // Future: MCP autogeneration
  }
);
```

#### Configuration Options

| Option | Type | Description | Default |
| :----- | :--- | :---------- | :------ |
| `docs` | `DocsConfiguration` | API documentation generation settings | Auto-configured |
| `busboy` | `BusboyConfig` | File upload configuration via busboy | Default limits |
| `text` | `OptionsText` | Text body parser options | Default settings |
| `json` | `OptionsJson` | JSON body parser options | Default settings |
| `urlencoded` | `OptionsUrlencoded` | URL-encoded body parser options | Default settings |
| `cors` | `CorsOptions` | Cross-Origin Resource Sharing configuration | Disabled |
| `mcp` | `McpConfiguration` | Model Context Protocol settings (planned) | Disabled |

#### Body Parser Configuration

**JSON Parser Options (`OptionsJson`)**
```typescript
{
  json: {
    limit: '100kb',           // Maximum request body size
    strict: true,             // Only parse objects and arrays
    type: 'application/json', // Content-Type to parse as JSON
    verify: undefined,        // Function to verify body
  }
}
```

**Text Parser Options (`OptionsText`)**
```typescript
{
  text: {
    limit: '100kb',          // Maximum request body size
    type: 'text/plain',      // Content-Type to parse as text
    defaultCharset: 'utf-8', // Default charset when not specified
  }
}
```

**URL-Encoded Parser Options (`OptionsUrlencoded`)**
```typescript
{
  urlencoded: {
    limit: '100kb',          // Maximum request body size
    extended: true,          // Use qs library (true) or querystring (false)
    parameterLimit: 1000,    // Maximum number of parameters
  }
}
```

**File Upload Options (`BusboyConfig`)**
```typescript
{
  busboy: {
    limits: {
      fileSize: 10 * 1024 * 1024, // 10MB file size limit
      files: 5,                    // Maximum number of files
      fields: 10,                  // Maximum number of fields
    },
    preservePath: false,           // Preserve file path
  }
}
```

**CORS Configuration (`CorsOptions`)**
```typescript
{
  cors: {
    origin: true,                  // Allow all origins
    methods: ['GET', 'POST', 'PUT', 'DELETE'],
    allowedHeaders: ['Content-Type', 'Authorization'],
    credentials: true,             // Allow credentials
    maxAge: 86400,                 // Preflight cache duration
  }
}
```

### HyperExpress Framework (`@forklaunch/hyper-express`)

```typescript
import { forklaunchExpress } from '@forklaunch/hyper-express';

const app = forklaunchExpress(
  schemaValidator,
  openTelemetryCollector,
  options?: {
    docs?: DocsConfiguration;
    busboy?: BusboyConfig;
    server?: ServerConstructorOptions;
    cors?: CorsOptions;
    mcp?: McpConfiguration; // Future: MCP autogeneration
  }
);
```

#### Configuration Options

| Option | Type | Description | Default |
| :----- | :--- | :---------- | :------ |
| `docs` | `DocsConfiguration` | API documentation generation settings | Auto-configured |
| `busboy` | `BusboyConfig` | File upload configuration via busboy | Default limits |
| `server` | `ServerConstructorOptions` | HyperExpress server configuration | Default settings |
| `cors` | `CorsOptions` | Cross-Origin Resource Sharing configuration | Disabled |
| `mcp` | `McpConfiguration` | Model Context Protocol settings (planned) | Disabled |

#### HyperExpress Server Options (`ServerConstructorOptions`)

```typescript
{
  server: {
    key_file_name: undefined,        // SSL private key file
    cert_file_name: undefined,       // SSL certificate file
    passphrase: undefined,           // SSL passphrase
    dh_params_file_name: undefined,  // DH parameters file
    ssl_prefer_low_memory_usage: false, // Optimize for memory vs speed
    compression: uWS.SHARED_COMPRESSOR, // Response compression
    max_compression_size: 64 * 1024,    // Maximum compression size
    max_backpressure: 64 * 1024,        // Maximum backpressure
    close_on_backpressure_limit: false, // Close on backpressure
    reset_idle_timeout_on_send: true,   // Reset timeout on send
    send_pings_automatically: true,     // Automatic ping frames
  }
}
```

#### Performance Tuning

**High Performance Configuration**
```typescript
const app = forklaunchExpress(schemaValidator, telemetryCollector, {
  server: {
    compression: uWS.DEDICATED_COMPRESSOR,
    max_compression_size: 128 * 1024,
    max_backpressure: 128 * 1024,
    ssl_prefer_low_memory_usage: false,
  },
  busboy: {
    limits: {
      fileSize: 50 * 1024 * 1024, // 50MB for large uploads
      files: 10,
    },
  },
});
```

**Memory Optimized Configuration**
```typescript
const app = forklaunchExpress(schemaValidator, telemetryCollector, {
  server: {
    ssl_prefer_low_memory_usage: true,
    max_compression_size: 32 * 1024,
    max_backpressure: 32 * 1024,
  },
  busboy: {
    limits: {
      fileSize: 5 * 1024 * 1024, // 5MB limit
      files: 3,
    },
  },
});
```

## Router

`forklaunchRouter` virtually acts as a drop-in replacement for `express.Router`, with additional initialization arguments:

### Express Router

```typescript
import { forklaunchRouter } from '@forklaunch/express';

const router = forklaunchRouter(
  basePath,
  schemaValidator,
  openTelemetryCollector,
  options?: {
    busboy?: BusboyConfig;
    text?: OptionsText;
    json?: OptionsJson;
    urlencoded?: OptionsUrlencoded;
  }
);
```

### HyperExpress Router

```typescript
import { forklaunchRouter } from '@forklaunch/hyper-express';

const router = forklaunchRouter(
  basePath,
  schemaValidator,
  openTelemetryCollector,
  options?: {
    busboy?: BusboyConfig;
  }
);
```

## Example Configurations

### Production Express Setup
```typescript
import { forklaunchExpress } from '@forklaunch/express';

const app = forklaunchExpress(schemaValidator, telemetryCollector, {
  json: {
    limit: '10mb',
    strict: true,
  },
  urlencoded: {
    limit: '10mb',
    extended: true,
    parameterLimit: 1000,
  },
  cors: {
    origin: process.env.ALLOWED_ORIGINS?.split(',') || false,
    credentials: true,
    methods: ['GET', 'POST', 'PUT', 'DELETE', 'PATCH'],
    allowedHeaders: ['Content-Type', 'Authorization', 'X-API-Key'],
  },
  busboy: {
    limits: {
      fileSize: 25 * 1024 * 1024, // 25MB
      files: 5,
      fields: 20,
    },
  },
  docs: {
    enabled: process.env.NODE_ENV !== 'production',
    title: 'My API Documentation',
    version: '1.0.0',
  },
});
```

### Development HyperExpress Setup
```typescript
import { forklaunchExpress } from '@forklaunch/hyper-express';

const app = forklaunchExpress(schemaValidator, telemetryCollector, {
  server: {
    compression: uWS.DEDICATED_COMPRESSOR,
    max_compression_size: 64 * 1024,
  },
  cors: {
    origin: true, // Allow all origins in development
    credentials: true,
  },
  busboy: {
    limits: {
      fileSize: 100 * 1024 * 1024, // 100MB for development
      files: 10,
    },
  },
  docs: {
    enabled: true,
    title: 'Development API',
    description: 'API documentation for development',
  },
});
```

## Handlers

Handlers virtually act as drop-in replacements for `express` handlers, with one or two additional arguments. This means you will be able to use existing handlers and middleware.

### Contract Details

With any handler registration, there now is a required `ContractDetails` type, which captures schema metadata that will automatically validate and coerce before executing handler logic:

```typescript
// These type definitions are simplified for demonstration purposes
type ContractDetails = HttpContractDetails | PathParamContractDetails | MiddlewareContractDetails

// This is simplified for documentation purposes
type PathParamContractDetails = {
  /** Name of the contract */
  readonly name: string;
  /** Summary of the contract */
  readonly summary: string;
  /** Response schemas for the contract */
  readonly responses: ResponseSchemas;
  /** Optional path params for the contract */
  readonly params?: ParamSchemas;
  /** Optional request headers for the contract */
  readonly requestHeaders?: ReqHeadersSchema;
  /** Optional response headers for the contract */
  readonly responseHeaders?: ResHeadersSchema;
  /** Optional query schemas for the contract */
  readonly query?: QuerySchema;
  /** Optional authentication details for the contract */
  readonly auth?: SchemaAuthMethods
  /** Endpoint level options */
  readonly options?: {
    readonly requestValidation: 'error' | 'warning' | 'none';
    readonly responseValidation: 'error' | 'warning' | 'none';
  };
}

type HttpContractDetails = PathParamContractDetails & {
  /** Body Schemas */
  readonly body: BodySchemas; 
}

type MiddlewareContractDetails = Partial<HttpContractDetails>
```

### Registering Handlers

There are two main ways to instantiate a route on an application or router, resembling common `express` patterns:

- Controller pattern: directly instantiate handlers inline when registering a route with a router/application

```typescript
interface Router {
    // Simplified definition
    function get(path: `/${string}`, contractDetails: PathParamContractDetails, ...handlers: ExpressLikeHandler[]) {
        ...
    };
    function post(path: `/${string}`, contractDetails: HttpContractDetails, ...handlers: ExpressLikeHandler[]) {
        ...
    };
    function use(path: `/${string}`, contractDetails: MiddlewareContractDetails, ...handlers: ExpressLikeHandler[]) {
        ...
    }
    ...
}

// example instantiation
router.get('/get-path', {
  name: 'Get Request',
  summary: 'a simple get request'
  responses: {
    200: {
      success: boolean
    }
  }
}, (req, res) => {
  res.status(200).json({
    success: true
  })
})
```

- Controller/Route pattern

```typescript
function get(schemaValidator: SchemaValidator, path?: `/${string}`, contractDetails: PathParamContractDetails, handler: ExpressLikeHandler[]) {
    ...
};
function post(schemaValidator: SchemaValidator, path?: `/${string}`, contractDetails: HttpContractDetails, handlers: ExpressLikeHandler[]) {
    ...
};
function use(schemaValidator: SchemaValidator, path?: `/${string}`, contractDetails: MiddlewareContractDetails, handlers: ExpressLikeHandler[]) {
    ...
}

// example instantiation
const standaloneController = get(SchemaValidator(), '/get-path', {
  name: 'Get Request',
  summary: 'a simple get request'
  responses: {
    200: {
      success: boolean
    }
  }
}, (req, res) => {
  res.status(200).json({
    success: true
  })
})
router.get('/get-path')
```

### Testing and Local Invocation

When registering a route, the return type is a live-typed function. This allows for testing or local invocation. It will run only the handlers specified on the route and return a result.

```typescript
// example instantiations
const liveFunction = router.post('/post-path', {
    name: 'post example',
    summary: 'A random post example',
    responses: {
        200: {
            someParam: string
        },
        418: literal("I'm a teapot"),
        500: number,
    },
    body: {
        someBodyParam: string,
        someDeeplyNestedBodyParam: {
            layerOne: {
                layerTwo: {
                    layerThree: union(string, boolean, number)
                }
            }
        }
    }
}, (req, res) => {
    // fully typed req and res objects based on contract detail definitions
    const a = req.body.someBodyParam.substring(0, 3);

    res.status(200).json({
      someParam: a
    })
});

// The live function can be invoked locally for testing purposes, using a fetch like syntax
const response = liveFunction('/post-path', {
  body: {
    someBodyParam: 'some string',
    someDeeplyNestedBodyParam: {
      layerOne: {
        layerTwo: {
          layerThree: true
        }
      }
    }
  }
});

// all valid typings and results
switch (response.statusCode) {
    case 200:
        // string return type
        console.log(res.content.someParam);
        break;
    case 418:
        // I'm a teapot
        console.log(res.content);
    case 500:
        // number return type
        console.log(res.content * 5)
}
```

### Best Practices

1. Define one API per request type
2. Use proper HTTP methods for operations
3. Keep handlers focused and simple
4. Use live functions for testing

### Performance Considerations

1. Choose the appropriate framework (`express` vs `hyper-express`) based on your needs
2. Configure validation levels in ContractDetails options when needed