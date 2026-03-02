---
title: MCP Autogeneration
category: Framework
description: Automatic Model Context Protocol (MCP) generation for ForkLaunch applications.
status: In Progress
---

# MCP Autogeneration

> ⚠️ **Development Status**: MCP is under active development. APIs and defaults may change before a stable release.

## Overview

ForkLaunch can expose your HTTP endpoints as **MCP tools** by spinning up a **FastMCP** server (Streamable HTTP / `httpStream` transport) and generating tools from your existing ForkLaunch route contracts.

At a high level:

- A FastMCP server acts as an MCP **control plane**.
- Each tool invocation proxies to the underlying HTTP route on your application host.

**Prerequisites:**
- Requires `zod` validator choice (see [Validation](/docs/framework/validation.md))
- Built on [HTTP Framework](/docs/framework/http.md) contract definitions
- Integrates with [Authorization](/docs/framework/authorization.md) at both the MCP layer and the underlying route layer

## Architecture

1. **Tool registration**: ForkLaunch walks the router tree and registers one MCP tool per route (and per version, if versioned).
2. **Schema integration**: Tool input schemas are derived from contract schemas (params/body/query/requestHeaders).
3. **Execution**: Tool calls proxy to the real HTTP endpoint and normalize the response into MCP content.
4. **Security**: You can gate access at the MCP transport layer and/or at the HTTP route layer.

```
┌─────────────────┐    MCP (httpStream)    ┌──────────────────────────┐
│   MCP Client    │ ─────────────────────► │ FastMCP Server (control)  │
│ (AI Assistant)  │                        │   POST/STREAM /mcp         │
└─────────────────┘                        └───────────┬──────────────┘
                                                        │ proxies
                                                        ▼
                                           ┌──────────────────────────┐
                                           │   Your HTTP API Host      │
                                           │   GET /users/:id          │
                                           │   POST /users             │
                                           └──────────────────────────┘
```

## Configuration (actual)

MCP is configured via the application `mcp` options:

- `mcp: false` disables MCP entirely.
- `mcp: { ... }` configures the FastMCP server (port/path/version/auth/etc).

### Disable MCP

```typescript
import { forklaunchExpress } from '@forklaunch/express';

const app = forklaunchExpress(schemaValidator, telemetryCollector, {
  mcp: false
});
```

### Typical setup (with auth hook)

```typescript
import { forklaunchExpress } from '@forklaunch/express';

const app = forklaunchExpress(schemaValidator, telemetryCollector, {
  mcp: {
    // Defaults:
    // - port: (appPort + 2000)
    // - path: '/mcp'
    // - version: '1.0.0'
    path: '/mcp',
    version: '1.0.0',

    /**
     * Optional: authenticate each MCP request (auth-framework-agnostic).
     * Return any object to represent the authenticated context, or undefined to deny.
     */
    authenticate: async (request) => {
      const token = request.headers.authorization?.replace('Bearer ', '');
      if (!token) return undefined;

      // Validate locally (JWT/HMAC/etc) OR via IAM (recommended for centralized OAuth/session management)
      return { subject: 'user-id-or-email' };
    }
  }
});
```

### Options table

| Option | Type | Description | Default |
| :----- | :--- | :---------- | :------ |
| `mcp` | `false \| object` | Disable or configure MCP | `object` (defaults applied when omitted) |
| `mcp.port` | `number` | Port for the MCP server | `appPort + 2000` |
| `mcp.path` | `` `/${string}` `` | MCP endpoint path on the MCP server | `'/mcp'` |
| `mcp.version` | `` `${number}.${number}.${number}` `` | MCP server version | `'1.0.0'` |
| `mcp.options` | `FastMCP options` | Passed through to FastMCP constructor | `{}` |
| `mcp.authenticate` | `(request) => Promise<object \| undefined>` | MCP request auth hook (framework-agnostic) | `undefined` |
| `mcp.additionalTools` | `(mcpServer) => void` | Register extra tools beyond HTTP routes | `undefined` |
| `mcp.contentTypeMapping` | `Record<string, string>` | Map contract response content-types to MCP content handling | `undefined` |

## Tool exposure (endpoint/router control)

### Per-endpoint opt-in/opt-out

Route contracts can opt out of MCP tool generation via `contractDetails.options.mcp`:

```typescript
const contractDetails = {
  name: 'AdminOperation',
  summary: 'Admin-only operation',
  options: {
    mcp: false
  },
  // ... params/body/query/responses/auth/etc
};
```

### Router-level control

Routers can also disable MCP tool generation via router options (`routerOptions.mcp`).

## Authentication and authorization

ForkLaunch supports authorization at **two layers**:

- **MCP transport auth** via `mcp.authenticate(request)` (gates access to the MCP control plane)
- **HTTP endpoint auth** via `contractDetails.auth` (JWT/HMAC/Basic + roles/permissions/scopes/features)

These are intentionally separate. Even if MCP transport auth succeeds, the proxied HTTP call may still fail contract-level authorization. See [Authorization](/docs/framework/authorization.md) for details.

## Calling the MCP endpoint (debug)

Assuming your app is on `:3000`, the default MCP port will be `:5000` and the default path is `/mcp`.

List available tools:

```bash
curl http://localhost:5000/mcp -X POST \
  -H "Content-Type: application/json" \
  -d '{"method":"tools/list"}'
```

Call a tool (example shape):

```bash
curl http://localhost:5000/mcp -X POST \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{"method":"tools/call","params":{"name":"GetUser","arguments":{"params":{"id":"123"}}}}'
```

## Notes and gotchas

- **Zod-only (currently)**: MCP generation requires the Zod schema validator.
- **Port separation**: The MCP server listens on its own port and proxies to your main HTTP server.
- **Tool names**: Tool names come from `contractDetails.name`. If a route has multiple versions, the tool name may be suffixed with a version label.

## Related documentation

- **[HTTP Frameworks](/docs/framework/http.md)** - Contracts that drive tool generation
- **[Validation](/docs/framework/validation.md)** - Zod requirement
- **[Authorization](/docs/framework/authorization.md)** - Endpoint auth and MCP auth hook design
- **[Error Handling](/docs/framework/error-handling.md)** - Standardized error responses
