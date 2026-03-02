---
title: Framework - Authorization
category: References
description: Reference for Authorization in ForkLaunch.
---

## Overview

ForkLaunch provides comprehensive authorization through the `@forklaunch/core` package's authentication system, built into the `ContractDetails` auth property. The system supports multiple authentication methods with flexible access control strategies.


## Authorization layers: HTTP endpoints vs MCP transport

ForkLaunch authorization can apply at **two different layers**, and it’s common to use both:

- **HTTP endpoint authorization (ContractDetails `auth`)**: Enforced on your normal REST/HTTP routes using `contractDetails.auth` (JWT/HMAC/Basic + roles/permissions/scopes + optional session typing).
- **MCP transport authorization (FastMCP `authenticate`)**: Enforced on the `/mcp` control plane itself using an `authenticate(request)` callback. This is intentionally **auth-framework-agnostic**: the framework doesn’t implement OAuth; it only calls your callback so you can validate tokens however you want (JWT, Better Auth session, external IAM call, etc.).

### MCP transport auth: `mcp.authenticate(request)`

When MCP is enabled for an app, you can provide an `authenticate` callback in the app’s MCP options. It is invoked for each MCP request and should return a user/session/context object, or `undefined` to deny access.

Example pattern:
```ts
const app = forklaunchExpress(schemaValidator, telemetryCollector, {
  mcp: {
    authenticate: async (request) => {
    const token = request.headers.authorization?.replace('Bearer ', '');
    if (!token) return undefined;
      // Option A: validate locally (JWT/JWKS/etc)
      // Option B: validate via IAM (recommended for centralized OAuth/session management)
      return { subject: 'user-id-or-email' };
    }
  }
});
```
**Important**: MCP transport auth is separate from HTTP route auth. Even if MCP authentication passes, the proxied HTTP route can still require `contractDetails.auth` (and should, for defense-in-depth).

### Centralized OAuth/IAM pattern (recommended)
A common architecture is:
- IAM service owns OAuth providers / sessions (e.g. Better Auth).
- Other services implement `mcp.authenticate` by calling IAM to validate the bearer token and returning the session/user object.
- HTTP routes still use contract-level auth for roles/permissions/scopes.


## Authentication Methods

The authorization system supports three main authentication methods:

### JWT Authentication

```typescript
const jwtAuth = {
  jwt: {
    // Option 1: Direct JWK key
    jwksPublicKey: jwkKey,
    
    // Option 2: JWKS URL for key discovery
    jwksPublicKeyUrl: 'https://auth.example.com/.well-known/jwks.json',
    
    // Option 3: Signature key for simple JWT validation
    signatureKey: 'your-secret-key'
  }
}
```

### Basic Authentication

```typescript
const basicAuth = {
  basic: {
    login: (username: string, password: string) => boolean
  }
}
```

### HMAC Authentication

```typescript
const hmacAuth = {
  hmac: {
    secretKeys: {
      'key-id-1': 'secret-key-1',
      'key-id-2': 'secret-key-2'
    }
  }
}
```

### Token Options

All authentication methods support optional token configuration:

```typescript
const authWithTokenOptions = {
  jwt: { signatureKey: 'secret' },
  tokenPrefix: 'Bearer',        // Default: 'Bearer'
  headerName: 'Authorization'   // Default: 'Authorization'
}
```

## Access Control Strategies

The authorization system supports permission-based and role-based access control, with optional scope-based authorization:

### Permission-Based Access Control

```typescript
const permissionControl = {
  // Allow specific permissions
  allowedPermissions: new Set(['read:users', 'write:users']),
  
  // OR forbid specific permissions
  forbiddenPermissions: new Set(['delete:users']),
  
  // Dynamic permission resolution
  surfacePermissions: async (resourceId: string, req: Request) => {
    // Return user's permissions for this resource
    return new Set(['read:users']);
  }
}
```

## Token decoding + session enrichment (`decodeResource`)

Sometimes authorization needs extra context that isn’t directly in the JWT (example: `organizationId`). In that case, use `decodeResource` to transform the raw token into the payload/session shape your routes depend on.

Typical use:
- Verify token (often via JWKS)
- Lookup additional context (e.g. user’s organization)
- Return an augmented payload that becomes available as typed session context (via `sessionSchema`)

Example pattern:
```ts
const contractDetails = {
  auth: {
    sessionSchema: {
      organizationId: string
    },
    jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
    decodeResource: async (token: string) => {
      const payload = await verifyAndDecodeJwt(token);
      const organizationId = await lookupOrganizationId(payload.sub);
      return { ...payload, organizationId };
    },
    allowedPermissions: new Set(['read:users'])
  }
};
```

### Role-Based Access Control

```typescript
const roleControl = {
  // Allow specific roles
  allowedRoles: new Set(['admin', 'editor']),
  
  // OR forbid specific roles
  forbiddenRoles: new Set(['blocked']),
  
  // Dynamic role resolution
  surfaceRoles: async (resourceId: string, req: Request) => {
    // Return user's roles for this resource
    return new Set(['editor']);
  }
}
```

### Scope-Based Access Control

```typescript
const scopeControl = {
  // Required scope for this endpoint
  requiredScope: 'users:read',
  
  // Scope hierarchy (more specific scopes include less specific ones)
  scopeHeirarchy: ['admin', 'users:write', 'users:read'],
  
  // Dynamic scope resolution
  surfaceScopes: async (resourceId: string, req: Request) => {
    // Return user's scopes for this resource
    return ['users:read'];
  }
}
```

### Session Schema

Define the structure of user session data:

```typescript
const sessionSchema = {
  userId: z.string(),
  email: z.string().email(),
  roles: z.array(z.string()),
  permissions: z.array(z.string())
}
```

## Implementation Examples

### JWT + Permissions
```typescript
const contractDetails = {
  auth: {
    jwt: { signatureKey: process.env.JWT_SECRET },
    allowedPermissions: new Set(['read:users']),
    surfacePermissions: async (resourceId: string, req: Request) => {
      // Extract permissions from JWT payload or database
      return new Set(['read:users']);
    },
    sessionSchema: {
      userId: z.string(),
      email: z.string().email(),
      permissions: z.array(z.string())
    }
  }
}
```

### JWT + Roles
```typescript
const contractDetails = {
  auth: {
    jwt: { jwksPublicKeyUrl: 'https://auth.example.com/.well-known/jwks.json' },
    allowedRoles: new Set(['admin']),
    surfaceRoles: async (resourceId: string, req: Request) => {
      // Extract roles from JWT payload or database
      return new Set(['admin']);
    }
  }
}
```

### Basic Auth + Permissions
```typescript
const contractDetails = {
  auth: {
    basic: {
      login: async (username: string, password: string) => {
        // Validate credentials against database
        const user = await getUserByUsername(username);
        return user && await verifyPassword(password, user.hashedPassword);
      }
    },
    allowedPermissions: new Set(['read:users']),
    surfacePermissions: async (resourceId: string, req: Request) => {
      // Get user permissions from database
      const user = await getUserByUsername(req.basicAuth?.username);
      return new Set(user?.permissions || []);
    }
  }
}
```

### HMAC Authentication
```typescript
const contractDetails = {
  auth: {
    hmac: {
      secretKeys: {
        'api-key-1': 'secret-key-1',
        'api-key-2': 'secret-key-2'
      }
    },
    allowedPermissions: new Set(['read:users']),
    surfacePermissions: async (resourceId: string, req: Request) => {
      // Get permissions based on API key
      return new Set(['read:users']);
    }
  }
}
```

### Scope-Based Authorization
```typescript
const contractDetails = {
  auth: {
    jwt: { signatureKey: process.env.JWT_SECRET },
    requiredScope: 'users:read',
    scopeHeirarchy: ['admin', 'users:write', 'users:read'],
    surfaceScopes: async (resourceId: string, req: Request) => {
      // Extract scopes from JWT or determine based on user context
      return ['users:read'];
    }
  }
}
```

### Combined Authorization
```typescript
const contractDetails = {
  auth: {
    jwt: { signatureKey: process.env.JWT_SECRET },
    allowedRoles: new Set(['admin', 'editor']),
    allowedPermissions: new Set(['read:users', 'write:users']),
    surfaceRoles: async (resourceId: string, req: Request) => {
      return new Set(['editor']);
    },
    surfacePermissions: async (resourceId: string, req: Request) => {
      return new Set(['read:users']);
    },
    sessionSchema: {
      userId: z.string(),
      email: z.string().email(),
      roles: z.array(z.string()),
      permissions: z.array(z.string())
    }
  }
}
```

## Best Practices

1. **Choose Authentication Base**
   - JWT for modern web applications
   - Basic Auth for simple scenarios
   - Custom Auth for special requirements

2. **Choose Access Control Strategy**
   - Permissions for granular control
   - Roles for simpler management
   - Can combine both for complex scenarios

3. **Security Considerations**
   - Always use HTTPS in production
   - Implement proper token validation
   - Keep permissions/roles granular
   - Document access requirements

## Error Responses

| Status | Message | Cause |
|--------|---------|-------|
| 401 | No Authorization token provided | Missing auth header |
| 401 | Invalid Authorization token format | Wrong token format |
| 403 | Invalid Authorization subject | JWT missing subject |
| 403 | Invalid Authorization permissions | Permission check failed |
| 403 | Invalid Authorization roles | Role check failed |
| 403 | Invalid Authorization token | Token validation failed |
| 403 | Invalid Authorization login | Basic auth failed |
| 500 | No permission/role surfacing function provided | Missing required mapper |

## Type Safety

The auth configuration is fully typed:
```typescript
type AuthMethods = {
  method: 'jwt' | 'basic' | 'other';
  allowedPermissions?: Set<string>;
  forbiddenPermissions?: Set<string>;
  allowedRoles?: Set<string>;
  forbiddenRoles?: Set<string>;
  surfacePermissions?: (resourceId: string, req: Request) => Promise<Set<string>>;
  surfaceRoles?: (resourceId: string, req: Request) => Promise<Set<string>>;
}
```

## Telemetry

Authorization events are automatically:
- Logged with appropriate severity
- Tagged with correlation IDs
- Tracked for metrics
- Traced for debugging

