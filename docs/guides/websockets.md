---
title: WebSocket Module
category: Framework
description: Complete guide to using type-safe WebSockets with automatic schema validation and AsyncAPI 3.0 generation.
---

## Overview

ForkLaunch provides a type-safe WebSocket implementation through `@forklaunch/ws` that extends the standard `ws` library with automatic schema validation, data transformation, and AsyncAPI 3.0 specification generation. All messages are validated at runtime and transformed between Buffer and JavaScript objects automatically.

## Quick Start

### Installation

```bash
# WebSocket library
pnpm add @forklaunch/ws ws

# Schema validator (choose one)
pnpm add @forklaunch/validator zod
# or
pnpm add @forklaunch/validator @sinclair/typebox
```

### Basic Client

```typescript
import { ForklaunchWebSocket } from '@forklaunch/ws';
import { ZodSchemaValidator } from '@forklaunch/validator/zod';
import { z } from 'zod';

// Define message schemas
const validator = new ZodSchemaValidator();
const schemas = {
  ping: { shape: z.object({ ts: z.number() }) },
  pong: { shape: z.object({ ts: z.number() }) },
  clientMessages: {
    chat: {
      shape: z.object({
        type: z.literal('chat'),
        message: z.string(),
        userId: z.string()
      })
    }
  },
  serverMessages: {
    response: {
      shape: z.object({
        type: z.literal('response'),
        data: z.string()
      })
    }
  }
};

// Create client
const ws = new ForklaunchWebSocket(
  validator,
  schemas,
  'ws://localhost:8080'
);

// Listen for messages (automatically validated and decoded)
ws.on('message', (data, isBinary) => {
  console.log('Received:', data); // Typed as serverMessages schema
});

// Send messages (automatically validated and encoded)
ws.on('open', () => {
  ws.send({ type: 'chat', message: 'Hello!', userId: '123' });
});
```

### Basic Server

```typescript
import { ForklaunchWebSocketServer } from '@forklaunch/ws';

// Create server
const wss = new ForklaunchWebSocketServer(
  validator,
  schemas,
  { port: 8080 }
);

// Handle connections
wss.on('connection', (ws, request) => {
  console.log('Client connected:', request.socket.remoteAddress);

  // Listen for client messages (automatically validated)
  ws.on('message', (data, isBinary) => {
    console.log('Received:', data); // Typed as clientMessages schema

    // Send response (automatically validated and encoded)
    ws.send({ type: 'response', data: 'Message received!' });
  });

  ws.on('close', (code, reason) => {
    console.log('Client disconnected:', code, reason);
  });
});
```

## Core Concepts

### EventSchema Structure

The `EventSchema` defines all message types for your WebSocket connection:

```typescript
type EventSchema<SV extends AnySchemaValidator> = {
  ping?: EventSchemaEntry<SV>;           // Optional ping frame schema
  pong?: EventSchemaEntry<SV>;           // Optional pong frame schema
  clientMessages: Record<string, EventSchemaEntry<SV>>;  // Client → Server
  serverMessages: Record<string, EventSchemaEntry<SV>>;  // Server → Client
  errors?: Record<string, EventSchemaEntry<SV>>;         // Optional error schemas
  closeReason?: Record<string, EventSchemaEntry<SV>>;    // Optional close reasons
};
```

Each entry contains:
- `shape`: The schema definition (Zod, TypeBox, etc.)
- Optional AsyncAPI metadata: `channel`, `channels`, `operation`, `operations`

### Automatic Schema Swapping

The server automatically swaps `clientMessages` and `serverMessages`:
- Client sends using `clientMessages` schema
- Server receives as `serverMessages` schema
- Server sends using `serverMessages` schema
- Client receives as `clientMessages` schema

This ensures type safety on both sides of the connection.

### Automatic Data Transformation

All data is automatically transformed between formats:

**Incoming (Buffer → Object):**
1. Buffer/ArrayBuffer/TypedArray → UTF-8 string
2. String → JSON parse
3. Object → Schema validation
4. Return validated object

**Outgoing (Object → Buffer):**
1. Object → Schema validation
2. Validated object → JSON stringify
3. String → UTF-8 Buffer
4. Send Buffer

## API Reference

### ForklaunchWebSocket (Client)

#### Constructor

```typescript
new ForklaunchWebSocket<SV, ES>(
  schemaValidator: SV,
  eventSchemas: ES,
  address: string | URL,
  protocols?: string | string[],
  options?: WebSocket.ClientOptions
)
```

**Parameters:**
- `schemaValidator`: Schema validator instance (e.g., `ZodSchemaValidator`)
- `eventSchemas`: Event schema definitions
- `address`: WebSocket server URL
- `protocols`: Optional WebSocket sub-protocols
- `options`: Optional connection options

**Example:**
```typescript
const ws = new ForklaunchWebSocket(
  validator,
  schemas,
  'ws://localhost:8080',
  ['chat-protocol'],
  { handshakeTimeout: 5000 }
);
```

#### Event Listeners

##### on(event, listener)

Register an event listener with automatic validation.

```typescript
ws.on('message', (data, isBinary) => {
  // data is typed and validated according to serverMessages schema
  console.log('Message:', data);
});

ws.on('open', () => {
  console.log('Connected!');
});

ws.on('close', (code, reason) => {
  console.log('Disconnected:', code, reason);
});

ws.on('error', (error) => {
  console.error('WebSocket error:', error);
});

ws.on('ping', (data) => {
  console.log('Ping received:', data);
});

ws.on('pong', (data) => {
  console.log('Pong received:', data);
});
```

##### once(event, listener)

Register a one-time event listener.

```typescript
ws.once('message', (data, isBinary) => {
  console.log('First message:', data);
  // Listener automatically removed after first invocation
});
```

##### off(event, listener) / removeListener(event, listener)

Remove a previously registered listener.

```typescript
const messageHandler = (data, isBinary) => {
  console.log('Message:', data);
};

ws.on('message', messageHandler);
ws.off('message', messageHandler);
```

#### Sending Data

##### send(data, options?, callback?)

Send a message with automatic validation and encoding.

```typescript
// Simple send
ws.send({ type: 'chat', message: 'Hello!', userId: '123' });

// With options
ws.send(
  { type: 'ping', timestamp: Date.now() },
  { compress: true, binary: true },
  (error) => {
    if (error) console.error('Send failed:', error);
  }
);
```

**Parameters:**
- `data`: Message data (must match `clientMessages` schema)
- `options`: Optional send options
  - `mask?: boolean`: Whether to mask the frame
  - `binary?: boolean`: Send as binary
  - `compress?: boolean`: Enable compression
  - `fin?: boolean`: Is this the final fragment
- `callback`: Optional callback invoked on completion

##### close(code?, reason?)

Close the WebSocket connection.

```typescript
// Normal closure
ws.close();

// With code and reason
ws.close(1000, { reason: 'User logged out' });
```

**Parameters:**
- `code`: Optional close code (default: 1000)
- `reason`: Optional close reason (validated against `closeReason` schema)

##### ping(data?, mask?, callback?)

Send a ping frame.

```typescript
// Simple ping
ws.ping({ timestamp: Date.now() });

// With callback
ws.ping({ ts: Date.now() }, true, (error) => {
  if (error) console.error('Ping failed:', error);
});
```

##### pong(data?, mask?, callback?)

Send a pong frame.

```typescript
// Auto-respond to pings
ws.on('ping', (data) => {
  ws.pong(data); // Echo ping data back
});
```

#### Properties

```typescript
ws.readyState  // Current connection state: CONNECTING, OPEN, CLOSING, CLOSED
ws.url         // WebSocket server URL
ws.protocol    // Negotiated sub-protocol
ws.bufferedAmount  // Bytes queued but not yet sent
```

### ForklaunchWebSocketServer (Server)

#### Constructor

```typescript
new ForklaunchWebSocketServer<SV, ES>(
  schemaValidator: SV,
  eventSchemas: ES,
  options?: WebSocketServer.ServerOptions,
  callback?: () => void
)
```

**Parameters:**
- `schemaValidator`: Schema validator instance
- `eventSchemas`: Event schema definitions
- `options`: Server options (port, host, server, etc.)
- `callback`: Optional callback invoked when server starts

**Example:**
```typescript
const wss = new ForklaunchWebSocketServer(
  validator,
  schemas,
  { port: 8080 },
  () => console.log('Server started on port 8080')
);
```

#### Server Options

```typescript
{
  port?: number;           // Port to listen on
  host?: string;           // Host to bind to
  server?: HttpServer;     // Existing HTTP server
  noServer?: boolean;      // Manual upgrade handling
  path?: string;           // Accept connections on this path
  maxPayload?: number;     // Maximum message size
  perMessageDeflate?: boolean | object;  // Compression options
}
```

#### Event Listeners

##### on('connection', listener)

Handle new client connections.

```typescript
wss.on('connection', (ws, request) => {
  console.log('Client connected from:', request.socket.remoteAddress);

  // ws is a ForklaunchWebSocket instance with validation
  ws.on('message', (data, isBinary) => {
    console.log('Received:', data);
  });
});
```

**Note:** The `ws` parameter is a `ForklaunchWebSocket` instance, not a plain WebSocket. It has all validation and transformation features enabled.

##### on('error', listener)

Handle server errors.

```typescript
wss.on('error', (error) => {
  console.error('Server error:', error);
});
```

##### on('listening', listener)

Server started listening.

```typescript
wss.on('listening', () => {
  console.log('Server is ready to accept connections');
});
```

##### on('close', listener)

Server closed.

```typescript
wss.on('close', () => {
  console.log('Server closed');
});
```

#### Properties

```typescript
wss.clients    // Set of connected WebSocket clients
wss.address()  // Server address info
```

#### Methods

##### close(callback?)

Close the server and all connections.

```typescript
wss.close(() => {
  console.log('Server shutdown complete');
});
```

## Schema Validation

### Using Zod

```typescript
import { ZodSchemaValidator } from '@forklaunch/validator/zod';
import { z } from 'zod';

const validator = new ZodSchemaValidator();
const schemas = {
  ping: { shape: z.object({ ts: z.number() }) },
  pong: { shape: z.object({ ts: z.number() }) },
  clientMessages: {
    // Discriminated union for type safety
    chat: {
      shape: z.object({
        type: z.literal('chat'),
        message: z.string().min(1).max(1000),
        userId: z.string().uuid(),
        roomId: z.string().optional()
      })
    },
    typing: {
      shape: z.object({
        type: z.literal('typing'),
        userId: z.string().uuid(),
        isTyping: z.boolean()
      })
    }
  },
  serverMessages: {
    chat: {
      shape: z.object({
        type: z.literal('chat'),
        message: z.string(),
        userId: z.string().uuid(),
        username: z.string(),
        timestamp: z.string().datetime()
      })
    },
    userJoined: {
      shape: z.object({
        type: z.literal('user-joined'),
        userId: z.string().uuid(),
        username: z.string()
      })
    }
  },
  errors: {
    error: {
      shape: z.object({
        code: z.string(),
        message: z.string(),
        details: z.unknown().optional()
      })
    }
  },
  closeReason: {
    normal: {
      shape: z.object({
        code: z.literal(1000),
        message: z.string()
      })
    },
    unauthorized: {
      shape: z.object({
        code: z.literal(1008),
        message: z.string()
      })
    }
  }
};
```

### Using TypeBox

```typescript
import { TypeBoxSchemaValidator } from '@forklaunch/validator/typebox';
import { Type } from '@sinclair/typebox';

const validator = new TypeBoxSchemaValidator();
const schemas = {
  ping: { shape: Type.Object({ ts: Type.Number() }) },
  pong: { shape: Type.Object({ ts: Type.Number() }) },
  clientMessages: {
    chat: {
      shape: Type.Object({
        type: Type.Literal('chat'),
        message: Type.String({ minLength: 1, maxLength: 1000 }),
        userId: Type.String({ format: 'uuid' })
      })
    }
  },
  serverMessages: {
    chat: {
      shape: Type.Object({
        type: Type.Literal('chat'),
        message: Type.String(),
        userId: Type.String({ format: 'uuid' }),
        timestamp: Type.String({ format: 'date-time' })
      })
    }
  }
};
```

### Validation Errors

When validation fails, a detailed error is thrown:

```typescript
ws.on('message', (data, isBinary) => {
  // Validation happens automatically
  console.log('Valid data:', data);
});

// If client sends invalid data:
// Error: Invalid web socket event
//   Expected: { type: 'chat', message: string, userId: string }
//   Received: { type: 'chat', message: 123 }
//   Error: message must be a string
```

## AsyncAPI 3.0 Generation

Generate AsyncAPI 3.0 specifications from your schemas:

```typescript
import { generateAsyncApi } from '@forklaunch/core/ws';

const asyncApiDoc = generateAsyncApi(schemas, {
  title: 'Chat WebSocket API',
  version: '1.0.0',
  description: 'Real-time chat application WebSocket API',
  defaultContentType: 'application/json'
});

console.log(JSON.stringify(asyncApiDoc, null, 2));
```

**Generated AsyncAPI Document:**
```json
{
  "asyncapi": "3.0.0",
  "info": {
    "title": "Chat WebSocket API",
    "version": "1.0.0",
    "description": "Real-time chat application WebSocket API"
  },
  "defaultContentType": "application/json",
  "channels": {
    "chat": {
      "address": "chat",
      "messages": {
        "chat": {
          "name": "chat",
          "payload": { /* Zod/TypeBox schema */ }
        }
      }
    }
  },
  "operations": {
    "receive-chat-chat": {
      "action": "receive",
      "channel": { "$ref": "#/channels/chat" },
      "messages": [{ "$ref": "#/channels/chat/messages/chat" }]
    },
    "send-chat-chat": {
      "action": "send",
      "channel": { "$ref": "#/channels/chat" },
      "messages": [{ "$ref": "#/channels/chat/messages/chat" }]
    }
  }
}
```

### Custom Channel and Operation Names

Add AsyncAPI metadata to your schemas:

```typescript
const schemas = {
  clientMessages: {
    chat: {
      shape: z.object({ type: z.literal('chat'), message: z.string() }),
      channel: 'chat-room',           // Custom channel name
      operation: 'sendChatMessage'    // Custom operation name
    },
    notification: {
      shape: z.object({ type: z.literal('notification'), text: z.string() }),
      channels: ['notifications', 'alerts'],  // Multiple channels
      operations: ['sendNotification', 'sendAlert']  // Multiple operations
    }
  }
};
```

## Common Patterns

### Real-Time Chat

```typescript
import { ForklaunchWebSocketServer } from '@forklaunch/ws';
import { ZodSchemaValidator } from '@forklaunch/validator/zod';
import { z } from 'zod';

const validator = new ZodSchemaValidator();

// Define chat schemas
const chatSchemas = {
  ping: { shape: z.object({ ts: z.number() }) },
  pong: { shape: z.object({ ts: z.number() }) },
  clientMessages: {
    join: {
      shape: z.object({
        type: z.literal('join'),
        roomId: z.string(),
        username: z.string()
      })
    },
    leave: {
      shape: z.object({
        type: z.literal('leave'),
        roomId: z.string()
      })
    },
    chat: {
      shape: z.object({
        type: z.literal('chat'),
        roomId: z.string(),
        message: z.string().min(1).max(1000)
      })
    }
  },
  serverMessages: {
    userJoined: {
      shape: z.object({
        type: z.literal('user-joined'),
        userId: z.string(),
        username: z.string()
      })
    },
    userLeft: {
      shape: z.object({
        type: z.literal('user-left'),
        userId: z.string()
      })
    },
    chat: {
      shape: z.object({
        type: z.literal('chat'),
        userId: z.string(),
        username: z.string(),
        message: z.string(),
        timestamp: z.string()
      })
    }
  }
};

// Room management
const rooms = new Map<string, Set<ForklaunchWebSocket<any, any>>>();

// Create server
const wss = new ForklaunchWebSocketServer(validator, chatSchemas, { port: 8080 });

wss.on('connection', (ws, request) => {
  let currentRoom: string | null = null;
  let username: string | null = null;

  ws.on('message', (data, isBinary) => {
    if (data.type === 'join') {
      // Join room
      currentRoom = data.roomId;
      username = data.username;

      if (!rooms.has(currentRoom)) {
        rooms.set(currentRoom, new Set());
      }
      rooms.get(currentRoom)!.add(ws);

      // Notify others
      broadcast(currentRoom, {
        type: 'user-joined',
        userId: generateId(),
        username
      }, ws);
    } else if (data.type === 'leave') {
      // Leave room
      if (currentRoom) {
        rooms.get(currentRoom)?.delete(ws);
        broadcast(currentRoom, {
          type: 'user-left',
          userId: generateId()
        }, ws);
        currentRoom = null;
      }
    } else if (data.type === 'chat') {
      // Send chat message
      if (currentRoom && username) {
        broadcast(currentRoom, {
          type: 'chat',
          userId: generateId(),
          username,
          message: data.message,
          timestamp: new Date().toISOString()
        });
      }
    }
  });

  ws.on('close', () => {
    if (currentRoom) {
      rooms.get(currentRoom)?.delete(ws);
    }
  });
});

// Broadcast to all clients in a room
function broadcast(
  roomId: string,
  message: any,
  exclude?: ForklaunchWebSocket<any, any>
) {
  const clients = rooms.get(roomId);
  if (!clients) return;

  clients.forEach((client) => {
    if (client !== exclude && client.readyState === 1) {
      client.send(message);
    }
  });
}
```

### Real-Time Updates with Subscriptions

```typescript
const updateSchemas = {
  ping: { shape: z.object({ ts: z.number() }) },
  pong: { shape: z.object({ ts: z.number() }) },
  clientMessages: {
    subscribe: {
      shape: z.object({
        type: z.literal('subscribe'),
        channel: z.string(),
        filters: z.record(z.unknown()).optional()
      })
    },
    unsubscribe: {
      shape: z.object({
        type: z.literal('unsubscribe'),
        channel: z.string()
      })
    }
  },
  serverMessages: {
    update: {
      shape: z.object({
        type: z.literal('update'),
        channel: z.string(),
        data: z.unknown(),
        timestamp: z.string()
      })
    },
    subscribed: {
      shape: z.object({
        type: z.literal('subscribed'),
        channel: z.string()
      })
    },
    unsubscribed: {
      shape: z.object({
        type: z.literal('unsubscribed'),
        channel: z.string()
      })
    }
  }
};

const subscriptions = new Map<string, Set<ForklaunchWebSocket<any, any>>>();

wss.on('connection', (ws, request) => {
  ws.on('message', (data, isBinary) => {
    if (data.type === 'subscribe') {
      // Add to subscription
      if (!subscriptions.has(data.channel)) {
        subscriptions.set(data.channel, new Set());
      }
      subscriptions.get(data.channel)!.add(ws);

      // Confirm subscription
      ws.send({ type: 'subscribed', channel: data.channel });
    } else if (data.type === 'unsubscribe') {
      // Remove from subscription
      subscriptions.get(data.channel)?.delete(ws);
      ws.send({ type: 'unsubscribed', channel: data.channel });
    }
  });

  ws.on('close', () => {
    // Clean up all subscriptions
    subscriptions.forEach((clients) => {
      clients.delete(ws);
    });
  });
});

// Publish updates to subscribers
function publishUpdate(channel: string, data: unknown) {
  const clients = subscriptions.get(channel);
  if (!clients) return;

  const message = {
    type: 'update',
    channel,
    data,
    timestamp: new Date().toISOString()
  };

  clients.forEach((client) => {
    if (client.readyState === 1) {
      client.send(message);
    }
  });
}

// Example: Publish database changes
db.on('user:created', (user) => {
  publishUpdate('users', { event: 'created', user });
});
```

### Push Notifications

```typescript
const notificationSchemas = {
  ping: { shape: z.object({ ts: z.number() }) },
  pong: { shape: z.object({ ts: z.number() }) },
  clientMessages: {
    register: {
      shape: z.object({
        type: z.literal('register'),
        userId: z.string(),
        deviceId: z.string()
      })
    },
    ack: {
      shape: z.object({
        type: z.literal('ack'),
        notificationId: z.string()
      })
    }
  },
  serverMessages: {
    notification: {
      shape: z.object({
        type: z.literal('notification'),
        id: z.string(),
        title: z.string(),
        body: z.string(),
        priority: z.enum(['low', 'normal', 'high']),
        data: z.record(z.unknown()).optional()
      })
    },
    registered: {
      shape: z.object({
        type: z.literal('registered'),
        connectionId: z.string()
      })
    }
  }
};

// Track user connections
const userConnections = new Map<string, Set<ForklaunchWebSocket<any, any>>>();

wss.on('connection', (ws, request) => {
  let userId: string | null = null;

  ws.on('message', (data, isBinary) => {
    if (data.type === 'register') {
      userId = data.userId;

      if (!userConnections.has(userId)) {
        userConnections.set(userId, new Set());
      }
      userConnections.get(userId)!.add(ws);

      ws.send({
        type: 'registered',
        connectionId: generateId()
      });
    } else if (data.type === 'ack') {
      console.log('Notification acknowledged:', data.notificationId);
    }
  });

  ws.on('close', () => {
    if (userId) {
      userConnections.get(userId)?.delete(ws);
    }
  });
});

// Send notification to user
async function sendNotification(
  userId: string,
  notification: {
    title: string;
    body: string;
    priority: 'low' | 'normal' | 'high';
    data?: Record<string, unknown>;
  }
) {
  const connections = userConnections.get(userId);
  if (!connections || connections.size === 0) {
    // User not connected, queue for later or use push notification service
    return;
  }

  const message = {
    type: 'notification',
    id: generateId(),
    ...notification
  };

  connections.forEach((ws) => {
    if (ws.readyState === 1) {
      ws.send(message);
    }
  });
}
```

### Authentication and Authorization

```typescript
import { URL } from 'url';

const authSchemas = {
  ping: { shape: z.object({ ts: z.number() }) },
  pong: { shape: z.object({ ts: z.number() }) },
  clientMessages: {
    auth: {
      shape: z.object({
        type: z.literal('auth'),
        token: z.string()
      })
    },
    message: {
      shape: z.object({
        type: z.literal('message'),
        content: z.string()
      })
    }
  },
  serverMessages: {
    authSuccess: {
      shape: z.object({
        type: z.literal('auth-success'),
        userId: z.string()
      })
    },
    authFailed: {
      shape: z.object({
        type: z.literal('auth-failed'),
        reason: z.string()
      })
    },
    message: {
      shape: z.object({
        type: z.literal('message'),
        content: z.string()
      })
    }
  },
  closeReason: {
    unauthorized: {
      shape: z.object({
        code: z.literal(1008),
        message: z.string()
      })
    }
  }
};

wss.on('connection', (ws, request) => {
  let authenticated = false;
  let userId: string | null = null;

  // Optional: Authenticate via query parameter
  const url = new URL(request.url!, 'ws://localhost');
  const token = url.searchParams.get('token');

  if (token && isValidToken(token)) {
    authenticated = true;
    userId = getUserIdFromToken(token);
  }

  // Set authentication timeout
  const authTimeout = setTimeout(() => {
    if (!authenticated) {
      ws.close(1008, { code: 1008, message: 'Authentication timeout' });
    }
  }, 5000);

  ws.on('message', (data, isBinary) => {
    if (data.type === 'auth') {
      // Authenticate via message
      if (isValidToken(data.token)) {
        authenticated = true;
        userId = getUserIdFromToken(data.token);
        clearTimeout(authTimeout);

        ws.send({ type: 'auth-success', userId: userId! });
      } else {
        ws.send({ type: 'auth-failed', reason: 'Invalid token' });
        ws.close(1008, { code: 1008, message: 'Authentication failed' });
      }
    } else if (data.type === 'message') {
      // Require authentication
      if (!authenticated) {
        ws.close(1008, { code: 1008, message: 'Not authenticated' });
        return;
      }

      // Process authenticated message
      ws.send({ type: 'message', content: `Echo: ${data.content}` });
    }
  });
});

function isValidToken(token: string): boolean {
  // Implement token validation
  return true;
}

function getUserIdFromToken(token: string): string {
  // Extract user ID from token
  return 'user-123';
}
```

### Heartbeat and Connection Management

```typescript
const HEARTBEAT_INTERVAL = 30000; // 30 seconds
const HEARTBEAT_TIMEOUT = 35000;  // 35 seconds

wss.on('connection', (ws, request) => {
  let isAlive = true;

  // Send ping every 30 seconds
  const heartbeat = setInterval(() => {
    if (!isAlive) {
      console.log('Client unresponsive, terminating connection');
      clearInterval(heartbeat);
      ws.terminate();
      return;
    }

    isAlive = false;
    ws.ping({ ts: Date.now() });
  }, HEARTBEAT_INTERVAL);

  // Listen for pong responses
  ws.on('pong', (data) => {
    isAlive = true;
    const latency = Date.now() - data.ts;
    console.log(`Client latency: ${latency}ms`);
  });

  ws.on('close', () => {
    clearInterval(heartbeat);
  });
});
```

## Integration with HTTP Server

### With Express

```typescript
import express from 'express';
import { createServer } from 'http';
import { ForklaunchWebSocketServer } from '@forklaunch/ws';

const app = express();
const server = createServer(app);

// HTTP routes
app.get('/', (req, res) => {
  res.send('Hello World!');
});

// WebSocket server on same port
const wss = new ForklaunchWebSocketServer(
  validator,
  schemas,
  { server }
);

wss.on('connection', (ws, request) => {
  console.log('WebSocket connected');
});

server.listen(8080, () => {
  console.log('Server listening on port 8080');
});
```

### With Manual Upgrade Handling

```typescript
import { createServer } from 'http';
import { ForklaunchWebSocketServer } from '@forklaunch/ws';

const server = createServer();
const wss = new ForklaunchWebSocketServer(
  validator,
  schemas,
  { noServer: true }
);

server.on('upgrade', (request, socket, head) => {
  // Custom authentication/authorization logic
  const token = new URL(request.url!, 'ws://localhost').searchParams.get('token');

  if (!isValidToken(token)) {
    socket.write('HTTP/1.1 401 Unauthorized\r\n\r\n');
    socket.destroy();
    return;
  }

  // Accept connection
  wss.handleUpgrade(request, socket, head, (ws) => {
    wss.emit('connection', ws, request);
  });
});

server.listen(8080);
```

## Best Practices

### 1. Always Define Schemas

```typescript
// Good - explicit schemas with validation
const schemas = {
  clientMessages: {
    chat: {
      shape: z.object({
        type: z.literal('chat'),
        message: z.string().min(1).max(1000)
      })
    }
  }
};

// Avoid - no validation
const ws = new WebSocket('ws://localhost:8080');
ws.send(JSON.stringify({ message: 'hi' })); // No type checking
```

### 2. Use Discriminated Unions

```typescript
// Good - discriminated union with type field
const schemas = {
  clientMessages: {
    chat: { shape: z.object({ type: z.literal('chat'), message: z.string() }) },
    ping: { shape: z.object({ type: z.literal('ping'), ts: z.number() }) }
  }
};

// Client knows exactly what type it received
ws.on('message', (data) => {
  if (data.type === 'chat') {
    console.log(data.message); // TypeScript knows this exists
  }
});
```

### 3. Handle Connection Lifecycle

```typescript
ws.on('open', () => {
  console.log('Connected');
  // Initialize connection (authenticate, subscribe, etc.)
});

ws.on('close', (code, reason) => {
  console.log('Disconnected:', code, reason);
  // Clean up resources, clear timers
});

ws.on('error', (error) => {
  console.error('WebSocket error:', error);
  // Log error, notify monitoring
});
```

### 4. Implement Reconnection Logic

```typescript
class ReconnectingWebSocket {
  private ws: ForklaunchWebSocket<any, any> | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000;

  constructor(
    private validator: any,
    private schemas: any,
    private url: string
  ) {
    this.connect();
  }

  private connect() {
    this.ws = new ForklaunchWebSocket(this.validator, this.schemas, this.url);

    this.ws.on('open', () => {
      console.log('Connected');
      this.reconnectAttempts = 0;
    });

    this.ws.on('close', () => {
      console.log('Disconnected');
      this.reconnect();
    });

    this.ws.on('error', (error) => {
      console.error('WebSocket error:', error);
    });
  }

  private reconnect() {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      console.error('Max reconnection attempts reached');
      return;
    }

    this.reconnectAttempts++;
    const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);

    console.log(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);

    setTimeout(() => {
      this.connect();
    }, delay);
  }

  send(data: any) {
    if (this.ws && this.ws.readyState === 1) {
      this.ws.send(data);
    } else {
      console.error('Cannot send: WebSocket not connected');
    }
  }
}
```

### 5. Use Type-Safe Message Handlers

```typescript
// Define message type
type ChatMessage = {
  type: 'chat';
  message: string;
  userId: string;
};

type TypingMessage = {
  type: 'typing';
  userId: string;
  isTyping: boolean;
};

type ClientMessage = ChatMessage | TypingMessage;

// Type-safe handler
ws.on('message', (data: ClientMessage, isBinary) => {
  switch (data.type) {
    case 'chat':
      handleChatMessage(data);
      break;
    case 'typing':
      handleTypingMessage(data);
      break;
  }
});

function handleChatMessage(msg: ChatMessage) {
  console.log(`${msg.userId}: ${msg.message}`);
}

function handleTypingMessage(msg: TypingMessage) {
  console.log(`${msg.userId} is ${msg.isTyping ? 'typing' : 'not typing'}`);
}
```

### 6. Graceful Shutdown

```typescript
const clients = new Set<ForklaunchWebSocket<any, any>>();

wss.on('connection', (ws) => {
  clients.add(ws);

  ws.on('close', () => {
    clients.delete(ws);
  });
});

// Graceful shutdown
process.on('SIGTERM', async () => {
  console.log('Shutting down WebSocket server...');

  // Close all client connections
  clients.forEach((ws) => {
    ws.close(1001, { code: 1001, message: 'Server shutting down' });
  });

  // Close server
  wss.close(() => {
    console.log('WebSocket server closed');
    process.exit(0);
  });
});
```

## Testing

### Unit Testing WebSocket Client

```typescript
import { describe, it, expect, vi } from 'vitest';
import { ForklaunchWebSocket } from '@forklaunch/ws';
import { ZodSchemaValidator } from '@forklaunch/validator/zod';

// Mock ws module
vi.mock('ws', () => {
  const { EventEmitter } = require('events');

  class MockWebSocket extends EventEmitter {
    constructor(url: string) {
      super();
      this.url = url;
      this.readyState = 1; // OPEN
    }
    send = vi.fn();
    close = vi.fn();
    ping = vi.fn();
    pong = vi.fn();
  }

  return { WebSocket: MockWebSocket };
});

describe('ForklaunchWebSocket', () => {
  const validator = new ZodSchemaValidator();
  const schemas = {
    ping: { shape: z.object({ ts: z.number() }) },
    pong: { shape: z.object({ ts: z.number() }) },
    clientMessages: {
      chat: { shape: z.object({ type: z.literal('chat'), message: z.string() }) }
    },
    serverMessages: {
      response: { shape: z.object({ type: z.literal('response'), data: z.string() }) }
    }
  };

  it('should validate and encode outgoing messages', () => {
    const ws = new ForklaunchWebSocket(validator, schemas, 'ws://localhost:8080');

    ws.send({ type: 'chat', message: 'Hello!' });

    expect(ws.send).toHaveBeenCalled();
  });

  it('should throw error on invalid message', () => {
    const ws = new ForklaunchWebSocket(validator, schemas, 'ws://localhost:8080');

    expect(() => {
      // @ts-expect-error - Invalid message type
      ws.send({ type: 'invalid', message: 'Hello!' });
    }).toThrow();
  });
});
```

### Integration Testing with Test Server

```typescript
import { ForklaunchWebSocket, ForklaunchWebSocketServer } from '@forklaunch/ws';

describe('WebSocket Integration', () => {
  let wss: ForklaunchWebSocketServer<any, any>;
  let ws: ForklaunchWebSocket<any, any>;

  beforeAll((done) => {
    wss = new ForklaunchWebSocketServer(validator, schemas, { port: 8081 }, done);
  });

  afterAll((done) => {
    wss.close(done);
  });

  it('should connect and send message', (done) => {
    ws = new ForklaunchWebSocket(validator, schemas, 'ws://localhost:8081');

    ws.on('open', () => {
      ws.send({ type: 'chat', message: 'Test' });
    });

    wss.once('connection', (client) => {
      client.once('message', (data) => {
        expect(data.type).toBe('chat');
        expect(data.message).toBe('Test');
        ws.close();
        done();
      });
    });
  });
});
```

## Performance Optimization

### 1. Use Binary Format for Large Messages

```typescript
// Automatically handled by ForklaunchWebSocket
ws.send(largeObject); // Automatically encoded as binary Buffer
```

### 2. Enable Compression

```typescript
const wss = new ForklaunchWebSocketServer(
  validator,
  schemas,
  {
    port: 8080,
    perMessageDeflate: {
      zlibDeflateOptions: {
        chunkSize: 1024,
        memLevel: 7,
        level: 3
      },
      zlibInflateOptions: {
        chunkSize: 10 * 1024
      },
      threshold: 1024 // Only compress messages > 1KB
    }
  }
);
```

### 3. Batch Updates

```typescript
// Instead of sending multiple messages
ws.send({ type: 'update', id: 1, value: 'a' });
ws.send({ type: 'update', id: 2, value: 'b' });
ws.send({ type: 'update', id: 3, value: 'c' });

// Send batch update
ws.send({
  type: 'batch-update',
  updates: [
    { id: 1, value: 'a' },
    { id: 2, value: 'b' },
    { id: 3, value: 'c' }
  ]
});
```

### 4. Monitor Connection Count

```typescript
setInterval(() => {
  console.log(`Active connections: ${wss.clients.size}`);
}, 60000);
```

## Related Documentation

- [HTTP Framework](/docs/framework/http.md): Integrating WebSockets with HTTP
- [Validation](/docs/framework/validation.md): Schema validation details
- [Testing](/docs/guides/testing.md): Testing strategies
- [Telemetry](/docs/framework/telemetry.md): Monitoring WebSocket performance
