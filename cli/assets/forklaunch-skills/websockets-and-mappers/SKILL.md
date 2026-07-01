---
name: websockets-and-mappers
description: "WebSockets and mappers: real-time, log streaming, requestMapper/responseMapper."
user-invokable: true
---

# ForkLaunch WebSockets & Mappers Skill

## When to Use This Skill

Use this skill when:

- Implementing real-time features with WebSockets
- Deciding whether to use mappers in services
- Converting database entities to API responses
- Building streaming log systems
- Creating real-time notifications
- Understanding data transformation patterns

## WebSocket Implementation

### Overview

Forklaunch provides `@forklaunch/ws` - a type-safe WebSocket library with automatic schema validation built on top of the standard `ws` library.

**Key Features:**

- Automatic message validation with Zod/TypeBox
- Type-safe event handling
- Buffer ↔ Object transformation
- Drop-in replacement for standard WebSocket

### EventSchema Format (CRITICAL)

The `EventSchema` type from `@forklaunch/core/ws` does NOT accept `z.object()` or `z.discriminatedUnion()`. It expects:

```typescript
type EventSchema<SV> = {
  ping?: EventSchemaEntry<SV>;
  pong?: EventSchemaEntry<SV>;
  clientMessages: Record<string, EventSchemaEntry<SV>>; // keyed record, NOT a union
  serverMessages: Record<string, EventSchemaEntry<SV>>; // keyed record, NOT a union
  errors?: Record<string, EventSchemaEntry<SV>>;
  closeReason?: Record<string, EventSchemaEntry<SV>>;
};

type EventSchemaEntry<SV> = {
  shape: IdiomaticSchema<SV>; // plain object with ForkLaunch validator primitives
};
```

**Each message type is a named key** in a `Record`, with a `shape` property containing an **idiomatic schema** (plain object with ForkLaunch primitives like `string`, `number`, `literal()`, `array()` — NOT `z.object()`).

The `@forklaunch/validator` uses `zod/v3` compatibility types internally. Zod v4's `z.object()` returns a `ZodObject` that does NOT satisfy the `IdiomaticSchema` type (missing string index signature). You MUST use the unwrapped ForkLaunch idiomatic format.

### Basic WebSocket Server

```typescript
import { ForklaunchWebSocketServer } from "@forklaunch/ws";
import {
  SchemaValidator,
  string,
  number,
  literal,
} from "@forklaunch/validator/zod";

// 1. Create validator
const validator = SchemaValidator();

// 2. Define message schemas using ForkLaunch idiomatic format
//    IMPORTANT: Use plain objects with validator primitives, NOT z.object()
const schemas = {
  // Messages from client to server — each key is a message type
  clientMessages: {
    subscribe: {
      shape: {
        type: literal("subscribe"),
        channel: string,
      },
    },
    unsubscribe: {
      shape: {
        type: literal("unsubscribe"),
        channel: string,
      },
    },
  },

  // Messages from server to client
  serverMessages: {
    notification: {
      shape: {
        type: literal("notification"),
        message: string,
        timestamp: number,
      },
    },
    error: {
      shape: {
        type: literal("error"),
        code: string,
        message: string,
      },
    },
  },

  // Ping/pong for keepalive
  ping: {
    shape: { timestamp: number },
  },
  pong: {
    shape: { timestamp: number },
  },

  // Error schema
  errors: {
    error: {
      shape: {
        code: string,
        message: string,
      },
    },
  },

  // Close reason
  closeReason: {
    close: {
      shape: {
        reason: string,
      },
    },
  },
};

// 3. Create WebSocket server
const wss = new ForklaunchWebSocketServer(validator, schemas, {
  port: 8080,
  path: "/ws",
});

// 4. Handle connections
wss.on("connection", (ws, request) => {
  console.log("Client connected");

  // Handle validated messages (automatically decoded from Buffer)
  ws.on("message", (data) => {
    // data is typed and validated!
    if (data.type === "subscribe") {
      console.log(`Subscribing to ${data.channel}`);
      // Subscribe logic
    } else if (data.type === "unsubscribe") {
      console.log(`Unsubscribing from ${data.channel}`);
      // Unsubscribe logic
    }
  });

  ws.on("close", (code, reason) => {
    console.log("Client disconnected:", code, reason);
  });

  ws.on("error", (error) => {
    console.error("WebSocket error:", error);
  });
});
```

### WebSocket Client

```typescript
import { ForklaunchWebSocket } from "@forklaunch/ws";

const ws = new ForklaunchWebSocket(
  validator,
  schemas,
  "ws://localhost:8080/ws",
);

ws.on("open", () => {
  console.log("Connected to server");

  // Send typed, validated message
  ws.send({
    type: "subscribe",
    channel: "deployment-logs",
  });
});

ws.on("message", (data) => {
  // data is typed and validated
  if (data.type === "notification") {
    console.log("Notification:", data.message);
  } else if (data.type === "error") {
    console.error("Server error:", data.message);
  }
});

ws.on("close", () => {
  console.log("Connection closed");
});
```

### How Validation Works

**Outgoing data** (server → wire, client → wire):

- `ws.send()` validates against the `clientMessages` schema (swapped on server side) before encoding to Buffer
- `ws.close()`, `ws.ping()`, `ws.pong()` validate against their respective schemas
- Invalid data throws immediately — the message is never sent

**Incoming data** (wire → listener):

- The `on('message', ...)` wrapper automatically decodes Buffer → JSON → validates against the `serverMessages` schema (swapped on server side)
- Invalid incoming data emits an `'error'` event on the WebSocket instead of reaching the message handler
- Always register an `on('error', ...)` handler to catch validation failures from malformed incoming messages

**Return type annotation required**: When a function returns a `WebSocketServer`, TypeScript may error with "inferred type cannot be named without a reference to `.pnpm/@types+ws`". Fix by adding an explicit return type (e.g., `: void` if you don't need the return value).

### Real-World Example: Deployment Log Streaming

```typescript
// websocket/deployment-logs.ws.ts
import {
  ForklaunchWebSocket,
  ForklaunchWebSocketServer,
  OPEN,
} from "@forklaunch/ws";
import {
  SchemaValidator,
  string,
  number,
  literal,
} from "@forklaunch/validator/zod";
import type { ZodSchemaValidator } from "@forklaunch/validator/zod";
import type { ServerEventSchema } from "@forklaunch/core/ws";

const validator = SchemaValidator();

const schemas = {
  clientMessages: {
    subscribe: {
      shape: {
        type: literal("subscribe"),
        deploymentId: string,
      },
    },
    unsubscribe: {
      shape: {
        type: literal("unsubscribe"),
        deploymentId: string,
      },
    },
  },
  serverMessages: {
    log: {
      shape: {
        type: literal("log"),
        deploymentId: string,
        timestamp: string,
        level: string,
        message: string,
      },
    },
    status: {
      shape: {
        type: literal("status"),
        deploymentId: string,
        status: string,
      },
    },
    error: {
      shape: {
        type: literal("error"),
        code: string,
        message: string,
      },
    },
  },
};

// Type alias for the server-side enhanced WebSocket
// On the server, clientMessages/serverMessages are swapped:
//   server sends with serverMessages schema, receives with clientMessages schema
type ServerWs = ForklaunchWebSocket<
  ZodSchemaValidator,
  ServerEventSchema<ZodSchemaValidator, typeof schemas>
>;

export function setupDeploymentLogsWebSocket(port: number, host: string) {
  const wss = new ForklaunchWebSocketServer(validator, schemas, {
    port,
    host,
    path: "/ws/deployment-logs",
  });

  // Track subscriptions with properly typed ForklaunchWebSocket instances
  const subscriptions = new Map<string, Set<ServerWs>>();

  wss.on("connection", (ws, request) => {
    const userId = getUserFromRequest(request);
    if (!userId) {
      ws.close(1008);
      return;
    }

    // Incoming messages are automatically validated and decoded
    ws.on("message", (data) => {
      // data is typed! data.type is 'subscribe' | 'unsubscribe'
      if (data.type === "subscribe") {
        if (!userCanAccessDeployment(userId, data.deploymentId)) {
          // Outgoing send() is validated against serverMessages schema
          ws.send({
            type: "error",
            code: "FORBIDDEN",
            message: "Access denied to deployment",
          });
          return;
        }

        if (!subscriptions.has(data.deploymentId)) {
          subscriptions.set(data.deploymentId, new Set());
        }
        subscriptions.get(data.deploymentId)!.add(ws);
      } else if (data.type === "unsubscribe") {
        const subs = subscriptions.get(data.deploymentId);
        if (subs) {
          subs.delete(ws);
          if (subs.size === 0) subscriptions.delete(data.deploymentId);
        }
      }
    });

    ws.on("error", (error) => {
      console.error(
        "WebSocket error (possibly invalid incoming message):",
        error,
      );
    });

    ws.on("close", () => {
      for (const [deploymentId, subs] of subscriptions.entries()) {
        subs.delete(ws);
        if (subs.size === 0) subscriptions.delete(deploymentId);
      }
    });
  });

  function broadcastLog(
    deploymentId: string,
    level: "info" | "warn" | "error",
    message: string,
  ) {
    const subs = subscriptions.get(deploymentId);
    if (!subs || subs.size === 0) return;

    // ws.send() validates against serverMessages schema automatically
    for (const ws of subs) {
      if (ws.readyState === OPEN) {
        ws.send({
          type: "log",
          deploymentId,
          timestamp: new Date().toISOString(),
          level,
          message,
        });
      }
    }
  }

  function broadcastStatus(
    deploymentId: string,
    status: "pending" | "in_progress" | "completed" | "failed",
  ) {
    const subs = subscriptions.get(deploymentId);
    if (!subs || subs.size === 0) return;

    for (const ws of subs) {
      if (ws.readyState === OPEN) {
        ws.send({ type: "status", deploymentId, status });
      }
    }
  }

  return { wss, broadcastLog, broadcastStatus };
}
```

### How to Modify server.ts for WebSocket Support

A standard ForkLaunch `server.ts` uses `app.listen()` to start the server. To add WebSocket support, you need to make **3 changes**: wrap the app in an HTTP server, attach WebSocket to it, and switch to `httpServer.listen()`.

#### Step 1: Add imports

Add `createServer` from `http` and your WebSocket setup function:

```diff
+import { createServer } from 'http';
+import { setupMyWebSocket } from './websocket/my-feature.ws';
```

#### Step 2: Wrap `app.internal` with `createServer` and attach WebSocket

After the `app` is created and routes are mounted, but **before** `app.listen()`, add:

```diff
+//! create HTTP server wrapping app.internal and attach WebSocket
+const httpServer = createServer(app.internal);
+setupMyWebSocket(httpServer);
```

**Why `app.internal`?** The ForkLaunch `app` is a wrapper object, not a raw Express instance. `app.internal` gives you the underlying Express `Application` that `createServer()` expects. Passing `app` directly will fail.

#### Step 3: Replace `app.listen()` with `httpServer.listen()`

```diff
-//! starts the API server
-app.listen(port, host, () => {
-  openTelemetryCollector.info(
-    `Server is running at ${protocol}://${host}:${port}`
-  );
-});
+//! starts the server
+httpServer.listen(port, host, () => {
+  openTelemetryCollector.info(
+    `Server is running at ${protocol}://${host}:${port}`
+  );
+  openTelemetryCollector.info(
+    `WebSocket available at ws://${host}:${port}/ws`
+  );
+});
```

**IMPORTANT:** `app.listen()` and `httpServer.listen()` on the same port will cause a port conflict. You have two options:

**Option A (Recommended): Dual-listen with `WS_PORT`** — `app.listen()` on the main `PORT` (for ForkLaunch SDK registration/OpenAPI/HTTP routes), `httpServer.listen()` on a separate `WS_PORT` (for WebSocket). `app.listen()` implicitly handles the main-module guard so the server won't start when the file is imported:

```typescript
import { createServer } from "node:http";
import { forklaunchExpress, SchemaValidator } from "@{{app-name}}/core";
import { ForklaunchWebSocketServer } from "@forklaunch/ws";
import { setupDeploymentLogsWebSocket } from "./websocket/deployment-logs.ws";

const port = Number(process.env.PORT ?? 8000);
const wsPort = Number(process.env.WS_PORT ?? 8001);

const app = forklaunchExpress(SchemaValidator(), openTelemetryCollector, {
  auth: {
    /* ... */
  },
});

app.use(someRouter);

// app.listen on PORT for ForkLaunch HTTP + SDK registration
app.listen(port, host, () => {
  // Start WebSocket on WS_PORT inside the listen callback
  const { wss, broadcastLog, broadcastStatus } = setupDeploymentLogsWebSocket(
    wsPort,
    host,
  );
  console.log(
    `HTTP at http://${host}:${port} | WS at ws://${host}:${wsPort}/ws`,
  );
});
```

Add `WS_PORT=8001` to your `.env.local` and point Vite's proxy at it:

```typescript
// vite.config.ts
proxy: { '/ws': { target: 'ws://localhost:8001', ws: true } }
```

**Option B: Replace `app.listen()` entirely** — only use `httpServer.listen()`. Simpler but you lose ForkLaunch's SDK registration that `app.listen()` triggers:

```typescript
const httpServer = createServer(app.internal);
setupMyWebSocket(httpServer);

httpServer.listen(port, host, () => {
  openTelemetryCollector.info(
    `Server running at ${protocol}://${host}:${port}`,
  );
});
```

All existing HTTP routes and middleware work with both options — `createServer(app.internal)` wraps the same Express app so HTTP and WebSocket traffic share a single port.

### Integrating WebSocket with Services

```typescript
// domain/services/deployment.service.ts
import { broadcastLog, broadcastStatus } from "../../server";

export class DeploymentService {
  async updateDeploymentStatus(deploymentId: string, status: DeploymentStatus) {
    // Update database
    await this.deploymentRepository.update(deploymentId, { status });

    // Broadcast to WebSocket subscribers
    broadcastStatus(deploymentId, status);

    return { success: true };
  }

  async logDeploymentMessage(
    deploymentId: string,
    level: "info" | "warn" | "error",
    message: string,
  ) {
    // Save to database
    await this.deploymentLogRepository.create({
      deploymentId,
      level,
      message,
    });

    // Broadcast to WebSocket subscribers
    broadcastLog(deploymentId, level, message);
  }
}
```

## Mappers: When and How to Use Them

### What are Mappers?

Mappers transform data between different representations:

- **Entity → DTO**: Convert database entities to API responses
- **DTO → Entity**: Convert API requests to database entities
- **Entity → Entity**: Transform between different entity types

### When to Use Mappers

#### USE Mappers When:

1. **External API Responses** - Formatting data for external consumers

```typescript
// You want to hide internal fields, format dates, compute derived properties
export class DeploymentMapper {
  static toDetailDto(deployment: Deployment) {
    return {
      id: deployment.id,
      status: deployment.status,
      createdAt: deployment.createdAt.toISOString(), // Format date
      duration: this.calculateDuration(deployment), // Computed field
      // Don't expose internal fields like deployment.internalState
    };
  }
}
```

2. **Complex Transformations** - Non-trivial data restructuring

```typescript
export class ApplicationMapper {
  static toDetailDto(application: Application) {
    return {
      id: application.id,
      name: application.name,
      // Flatten nested relations
      services: application.services.map((s) => ({
        id: s.id,
        name: s.name,
        status: s.status,
      })),
      // Aggregate data
      totalDeployments: application.deployments.length,
      lastDeployment: application.deployments[0]?.createdAt,
    };
  }
}
```

3. **Multiple Representations** - Same entity, different API versions

```typescript
export class UserMapper {
  static toPublicDto(user: User) {
    return {
      id: user.id,
      name: user.name,
      avatar: user.avatar,
    };
  }

  static toDetailDto(user: User) {
    return {
      ...this.toPublicDto(user),
      email: user.email,
      createdAt: user.createdAt,
      settings: user.settings,
    };
  }

  static toAdminDto(user: User) {
    return {
      ...this.toDetailDto(user),
      internalId: user.internalId,
      auditLogs: user.auditLogs,
    };
  }
}
```

4. **Consistency Across Endpoints** - Same format everywhere

```typescript
// Use mapper to ensure consistent formatting
export class DeploymentMapper {
  static toDetailDto(deployment: Deployment) {
    // This ensures all endpoints return deployments in the same format
    return {
      id: deployment.id,
      applicationId: deployment.application.id,
      status: deployment.status,
      // ... consistent structure
    };
  }
}

// In multiple controllers
return res.json(DeploymentMapper.toDetailDto(deployment));
```

#### DON'T Use Mappers When:

1. **Simple Pass-Through** - Entity structure matches API response

```typescript
// Unnecessary mapper
export class UserMapper {
  static toDto(user: User) {
    return {
      id: user.id,
      name: user.name,
      email: user.email,
    };
  }
}

// Just return the entity
return res.json({
  id: user.id,
  name: user.name,
  email: user.email,
});
```

2. **Internal Services** - Communication between services in same module

```typescript
// Don't use mappers for internal service-to-service calls
const deployment = DeploymentMapper.toDto(entity); // NO!
await this.otherService.process(deployment);

// Pass entities directly
await this.otherService.process(entity);
```

3. **Single Use Case** - Transformation used in only one place

```typescript
// Overkill for one-off transformation
export class SpecialCaseMapper {
  static transform(data: Data) {
    return { ...data, special: true };
  }
}

// Just do it inline
return res.json({ ...data, special: true });
```

### Mapper Implementation Patterns

#### Static Class Pattern (Recommended)

```typescript
// domain/mappers/deployment.mappers.ts
import type { Deployment } from "../../persistence/entities";

export class DeploymentMapper {
  /**
   * Convert deployment entity to detailed DTO
   * Used for external API responses
   */
  static toDetailDto(deployment: Deployment) {
    return {
      id: deployment.id,
      applicationId: deployment.application.id,
      releaseId: deployment.release.id,
      releaseVersion: deployment.release.version,
      environment: deployment.environment,
      region: deployment.region,
      status: deployment.status,
      deploymentType: deployment.deploymentType,
      createdAt: deployment.createdAt ?? new Date(),
      startedAt: deployment.startedAt || undefined,
      completedAt: deployment.completedAt || undefined,
      errorMessage: deployment.errorMessage || undefined,
      // Remove null, convert to undefined for cleaner JSON
      metadata: deployment.metadata || undefined,
      // Don't expose internal fields
      // internalState: deployment.internalState  // Hidden
    };
  }

  /**
   * Convert to list item DTO (lighter version)
   */
  static toListItemDto(deployment: Deployment) {
    return {
      id: deployment.id,
      applicationId: deployment.application.id,
      status: deployment.status,
      createdAt: deployment.createdAt ?? new Date(),
    };
  }

  /**
   * Convert array of entities
   */
  static toDetailDtoList(deployments: Deployment[]) {
    return deployments.map((d) => this.toDetailDto(d));
  }
}
```

#### Usage in Controllers

```typescript
// api/controllers/deployment.controller.ts
import { DeploymentMapper } from "../../domain/mappers/deployment.mappers";

export class DeploymentController {
  async getDeployment(req: Request, res: Response) {
    const deployment = await this.deploymentService.findById(req.params.id);

    if (!deployment) {
      return res.status(404).json({ error: "Deployment not found" });
    }

    // Use mapper for external API response
    return res.json({
      data: DeploymentMapper.toDetailDto(deployment),
    });
  }

  async listDeployments(req: Request, res: Response) {
    const deployments = await this.deploymentService.list(req.query);

    // Use lighter mapper for list responses
    return res.json({
      data: deployments.map((d) => DeploymentMapper.toListItemDto(d)),
    });
  }
}
```

#### Usage in Services: When NOT to Use Mappers

```typescript
// domain/services/deployment.service.ts
export class DeploymentService {
  // GOOD: Return entities directly for internal use
  async findById(id: string): Promise<Deployment | null> {
    return this.em.findOne(Deployment, { id });
  }

  // GOOD: Service-to-service calls use entities
  async processDeployment(deploymentId: string) {
    const deployment = await this.findById(deploymentId);
    if (!deployment) throw new Error("Not found");

    // Pass entity to other services
    await this.pulumiService.deploy(deployment); // Not a DTO!
    await this.notificationService.notify(deployment); // Not a DTO!

    return deployment; // Return entity, let controller map if needed
  }

  // BAD: Don't use mappers in service layer
  async getBadExample(id: string) {
    const deployment = await this.findById(id);
    return DeploymentMapper.toDetailDto(deployment); // NO! Controllers do this
  }
}
```

### Best Practices for Mappers

1. **Keep Mappers in `domain/mappers/`** - Centralized location
2. **Use Static Methods** - No need for instances
3. **Name Methods Clearly** - `toDetailDto`, `toListItemDto`, `toEntity`
4. **Document Purpose** - Explain when each mapper is used
5. **Handle Nulls Gracefully** - Convert `null` to `undefined` for cleaner JSON
6. **Don't Map in Services** - Services work with entities, controllers map for API
7. **Type Everything** - Use TypeScript interfaces for DTOs

```typescript
// Define DTO types
export interface DeploymentDetailDto {
  id: string;
  applicationId: string;
  status: string;
  createdAt: Date;
  // ...
}

export class DeploymentMapper {
  static toDetailDto(deployment: Deployment): DeploymentDetailDto {
    return {
      // typed transformation
    };
  }
}
```

## When Claude Code Should Use This Skill

1. **Implementing real-time features**: Use ForklaunchWebSocket patterns
2. **Streaming logs/data**: Apply WebSocket broadcasting patterns
3. **Building external APIs**: Use mappers to format responses
4. **Internal service calls**: Don't use mappers, pass entities
5. **Deciding on mappers**: Apply the when/when-not guidelines
6. **Complex transformations**: Create mapper classes
7. **Simple responses**: Skip mappers, format inline

## Proven Patterns (from Whiteboard App — Real-Time Collaborative Drawing)

These patterns were validated in a working real-time collaborative whiteboard built on ForkLaunch.

### Import Options

You can use either `ForklaunchWebSocketServer` from `@forklaunch/ws` (provides automatic schema validation and type-safe messaging) or `WebSocketServer` from `ws` directly (simpler, manual JSON handling):

```typescript
// Option 1: With validation (recommended for structured message protocols)
import { ForklaunchWebSocketServer } from "@forklaunch/ws";

// Option 2: Plain ws (simpler for unstructured/freestyle messaging)
import { WebSocketServer } from "ws";
```

### State Service Pattern

Extract WebSocket state (connected users, shared data, broadcast logic) into a dedicated service class rather than inlining everything in the connection handler. This keeps the WS setup clean and makes state testable:

```typescript
// domain/services/whiteboard-state.service.ts
import type { WebSocket } from "ws";

export class WhiteboardStateService {
  private users = new Map<
    string,
    { name: string; color: string; ws: WebSocket }
  >();

  addUser(id: string, name: string, color: string, ws: WebSocket) {
    this.users.set(id, { name, color, ws });
  }

  removeUser(id: string) {
    this.users.delete(id);
  }

  getSerializableUsers() {
    return [...this.users.entries()].map(([id, u]) => ({
      id,
      name: u.name,
      color: u.color,
    }));
  }

  broadcast(data: unknown, excludeUserId?: string) {
    const msg = JSON.stringify(data);
    for (const [id, user] of this.users) {
      if (id !== excludeUserId && user.ws.readyState === 1) {
        user.ws.send(msg);
      }
    }
  }
}
```

Then the WebSocket setup file stays focused on message routing:

```typescript
// websocket/whiteboard.ws.ts
import { WebSocketServer } from "ws";
import type { Server } from "http";
import { WhiteboardStateService } from "../domain/services/whiteboard-state.service";

const state = new WhiteboardStateService();

export function setupWhiteboardWebSocket(httpServer: Server) {
  const wss = new WebSocketServer({ server: httpServer, path: "/ws" });

  wss.on("connection", (ws) => {
    // ... message routing using state service
  });

  return wss;
}
```

### Join/Welcome Handshake Pattern

For multi-user real-time apps, use a join/welcome handshake so new users receive current state:

1. Client connects and sends `{ type: 'join', name: '...' }`
2. Server assigns an ID and color, adds user to state
3. Server sends `{ type: 'welcome', userId, color, users: [...], strokes: [...] }` back to the joining user with full current state
4. Server broadcasts `{ type: 'user-joined', userId, name, color }` to all other users
5. On disconnect, server broadcasts `{ type: 'user-left', userId }`

This ensures late joiners see all existing state and all users see presence changes.

### Node.js 24 + tsx: Avoid MikroORM Decorator Imports in server.ts

When using Node.js 24 with `tsx`, legacy TypeScript decorators (used by MikroORM entities) cause TC39 decorator mismatch errors. **server.ts must import directly from `@forklaunch/*` packages** — never import from a barrel that re-exports MikroORM entities:

```typescript
// server.ts — GOOD: import directly from packages
import { SchemaValidator } from "@forklaunch/validator/zod";
import { forklaunchExpress } from "@forklaunch/express";
import { OpenTelemetryCollector } from "@forklaunch/core/http";

// BAD: this pulls in MikroORM entities with decorators
// import { forklaunchExpress, SchemaValidator } from '@{{app-name}}/core';
```

### Client-Side React WebSocket Hook

Use refs (not state) for high-frequency real-time event callbacks to avoid re-render cascades. State is only for data that needs to trigger re-renders (user list, connection status):

```typescript
// hooks/useWebSocket.ts
export function useWebSocket(name: string | null) {
  const wsRef = useRef<WebSocket | null>(null);
  const [connected, setConnected] = useState(false);
  const [myId, setMyId] = useState<string | null>(null);
  const [users, setUsers] = useState<UserInfo[]>([]);

  // HIGH-FREQUENCY callbacks: use refs, NOT state
  const onRemoteDrawStartRef = useRef<((msg: DrawStartMsg) => void) | null>(
    null,
  );
  const onRemoteDrawMoveRef = useRef<((msg: DrawMoveMsg) => void) | null>(null);

  const send = useCallback((msg: ClientMessage) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(msg));
    }
  }, []);

  useEffect(() => {
    if (!name) return;
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(`${protocol}//${window.location.host}/ws`);
    wsRef.current = ws;

    ws.onopen = () => {
      setConnected(true);
      ws.send(JSON.stringify({ type: "join", name }));
    };

    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      switch (msg.type) {
        case "welcome":
          setMyId(msg.userId);
          setUsers(msg.users);
          break;
        case "draw-move":
          onRemoteDrawMoveRef.current?.(msg); // ref, no re-render
          break;
        // ...
      }
    };

    ws.onclose = () => setConnected(false);
    return () => {
      ws.close();
      wsRef.current = null;
    };
  }, [name]);

  return { connected, myId, users, send, onRemoteDrawMoveRef /* ... */ };
}
```

The canvas component sets `onRemoteDrawMoveRef.current = (msg) => { /* draw on canvas */ }` — this avoids re-rendering the entire component tree on every mouse move from every user.

### Vite Dev Server Proxy for WebSocket

When using Vite for the client, proxy `/ws` to the WebSocket port so the client can connect via `window.location.host`:

```typescript
// vite.config.ts
export default defineConfig({
  server: {
    proxy: {
      "/ws": {
        target: "ws://localhost:8001", // WS_PORT, not PORT
        ws: true,
      },
    },
  },
});
```

### Standalone Server Pattern (No Database)

For services that don't need a database (e.g., ephemeral in-memory rooms, WebSocket-only services), you can bypass the bootstrapper and MikroORM entirely. Create the `OpenTelemetryCollector` directly with `metricsDefinitions({})` for empty metrics:

```typescript
// server.ts — standalone, no bootstrapper, no MikroORM
import {
  metricsDefinitions,
  OpenTelemetryCollector,
} from "@forklaunch/core/http";
import { forklaunchExpress, SchemaValidator } from "@{{app-name}}/core";
import { ForklaunchWebSocketServer } from "@forklaunch/ws";
import { schemas, handleConnection } from "./wsHandler";

const host = process.env.HOST ?? "localhost";
const port = Number(process.env.PORT ?? 8000);
const wsPort = Number(process.env.WS_PORT ?? 8001);

// Empty metrics — no monitoring module needed
const metrics = metricsDefinitions({});
const openTelemetryCollector = new OpenTelemetryCollector(
  "my-service",
  "info",
  metrics,
);
const validator = SchemaValidator();

const app = forklaunchExpress(validator, openTelemetryCollector, {
  auth: {
    surfaceRoles: async () => {
      throw new Error("Not implemented");
    },
    surfacePermissions: async () => {
      throw new Error("Not implemented");
    },
    surfaceFeatures: async () => {
      throw new Error("Not implemented");
    },
    surfaceSubscription: async () => {
      throw new Error("Not implemented");
    },
  },
});

app.internal.get("/health", (_req, res) => {
  res.json({ status: "ok" });
});

// app.listen on PORT for ForkLaunch HTTP + SDK registration
app.listen(port, host, () => {
  // Start WebSocket on separate WS_PORT inside the listen callback
  const wss = new ForklaunchWebSocketServer(validator, schemas, {
    port: wsPort,
    host,
    path: "/ws",
  });
  wss.on("connection", handleConnection);
  console.log(
    `HTTP at http://${host}:${port} | WS at ws://${host}:${wsPort}/ws`,
  );
});
```

This avoids `bootstrapper.ts` → `registrations.ts` → `MikroORM.initSync()` which requires a running database. The service dev script can also skip `pnpm migrate:up`.

### Core Package Must Be Built Before Services Can Import

The `@{{app-name}}/core` package has `"main": "lib/index.js"` — this means you must run `pnpm build` (which runs `tsgo` in all workspace packages) before services can resolve core imports. If you see `ERR_MODULE_NOT_FOUND` for `@{{app-name}}/core/lib/index.js`, the fix is:

```bash
cd src/modules && pnpm build
```

## Important Notes

- **EventSchema uses `Record<string, { shape: IdiomaticSchema }>` format** — NOT `z.object()` or `z.discriminatedUnion()`. Use plain objects with ForkLaunch validator primitives (`string`, `number`, `literal()`, `array()`) for the `shape` values.
- **`ForklaunchWebSocketServer` provides automatic validation** — use it for structured message protocols. For simpler unstructured messaging, plain `WebSocketServer` from `ws` also works.
- **Return type annotations needed** when functions return `WebSocketServer` to avoid "inferred type cannot be named" errors with pnpm-nested `@types/ws`.
- **Node.js 24 + tsx**: server.ts must avoid importing MikroORM entity barrels — import directly from `@forklaunch/*` packages.
- **Use refs for high-frequency WS callbacks** in React — state updates on every draw-move will kill performance.
- Always handle authentication in WebSocket connections
- Use mappers for external API boundaries, not internal service calls
- Keep entity-to-entity communication direct (no DTOs)
- Mappers live in `domain/mappers/`, not in services
- Services return entities, controllers map to DTOs
