---
title: AsyncAPI for WebSockets
category: Guides
description: Generate AsyncAPI 3.0 specifications for WebSocket endpoints with event schemas and channel definitions.
---

## Overview

ForkLaunch provides AsyncAPI 3.0.0 specification generation for **WebSocket endpoints**. The `generateAsyncApi` function creates comprehensive AsyncAPI documentation from your event schemas, including client-to-server and server-to-client message definitions.

**Note**: AsyncAPI generation is currently **only available for WebSocket channels**. Queue-based messaging (BullMQ, Redis, Kafka, Database) does not include AsyncAPI generation at this time.

## WebSocket AsyncAPI Generation

### Event Schema Definition

Define your WebSocket event schemas using the `EventSchema` type:

```typescript
// ws/event-schemas.ts
import { EventSchema } from '@forklaunch/framework/ws';
import { schemaValidator } from '@/validators';
import { object, string, number, boolean } from '@forklaunch/blueprint-core';

// Define message schemas
const chatMessageSchema = object({
  roomId: string,
  userId: string,
  message: string,
  timestamp: number
});

const userTypingSchema = object({
  roomId: string,
  userId: string,
  isTyping: boolean
});

const notificationSchema = object({
  type: string,
  title: string,
  message: string,
  timestamp: number
});

// Define event schema with channels
export const chatEventSchemas: EventSchema<typeof schemaValidator> = {
  // Messages sent FROM client TO server
  clientMessages: {
    sendMessage: {
      shape: chatMessageSchema,
      channel: 'chat'  // Optional: specify channel
    },
    userTyping: {
      shape: userTypingSchema,
      channel: 'chat'
    }
  },

  // Messages sent FROM server TO client
  serverMessages: {
    messageReceived: {
      shape: chatMessageSchema,
      channel: 'chat'
    },
    notification: {
      shape: notificationSchema
      // No channel = uses message key as channel name
    }
  },

  // Error messages
  errors: {
    invalidMessage: {
      shape: object({
        code: string,
        message: string
      })
    }
  }
};
```

### Generating AsyncAPI Spec

```typescript
// ws/asyncapi-gen.ts
import { generateAsyncApi } from '@forklaunch/framework/ws';
import { chatEventSchemas } from './event-schemas';
import fs from 'fs';

const asyncApiSpec = generateAsyncApi({
  schemas: chatEventSchemas,
  info: {
    title: 'Chat WebSocket API',
    version: '1.0.0',
    description: 'Real-time chat messaging API'
  },
  servers: {
    development: {
      url: 'ws://localhost:3000',
      protocol: 'ws',
      description: 'Development WebSocket server'
    },
    production: {
      url: 'wss://api.example.com',
      protocol: 'wss',
      description: 'Production WebSocket server (TLS)'
    }
  }
});

// Export as JSON
fs.writeFileSync(
  'asyncapi.json',
  JSON.stringify(asyncApiSpec, null, 2)
);

// Export as YAML (requires js-yaml)
import yaml from 'js-yaml';
fs.writeFileSync(
  'asyncapi.yaml',
  yaml.dump(asyncApiSpec)
);
```

### Generated AsyncAPI Example

```yaml
asyncapi: 3.0.0
info:
  title: Chat WebSocket API
  version: 1.0.0
  description: Real-time chat messaging API

servers:
  development:
    url: ws://localhost:3000
    protocol: ws
    description: Development WebSocket server
  production:
    url: wss://api.example.com
    protocol: wss
    description: Production WebSocket server (TLS)

channels:
  chat:
    address: chat
    messages:
      sendMessage:
        $ref: '#/components/messages/sendMessage'
      userTyping:
        $ref: '#/components/messages/userTyping'
      messageReceived:
        $ref: '#/components/messages/messageReceived'

  notification:
    address: notification
    messages:
      notification:
        $ref: '#/components/messages/notification'

operations:
  # Client -> Server operations
  sendMessageOp:
    action: send
    channel:
      $ref: '#/channels/chat'
    messages:
      - $ref: '#/components/messages/sendMessage'

  userTypingOp:
    action: send
    channel:
      $ref: '#/channels/chat'
    messages:
      - $ref: '#/components/messages/userTyping'

  # Server -> Client operations
  receiveMessageOp:
    action: receive
    channel:
      $ref: '#/channels/chat'
    messages:
      - $ref: '#/components/messages/messageReceived'

  receiveNotificationOp:
    action: receive
    channel:
      $ref: '#/channels/notification'
    messages:
      - $ref: '#/components/messages/notification'

components:
  messages:
    sendMessage:
      name: sendMessage
      payload:
        type: object
        properties:
          roomId:
            type: string
          userId:
            type: string
          message:
            type: string
          timestamp:
            type: number
        required:
          - roomId
          - userId
          - message
          - timestamp

    userTyping:
      name: userTyping
      payload:
        type: object
        properties:
          roomId:
            type: string
          userId:
            type: string
          isTyping:
            type: boolean
        required:
          - roomId
          - userId
          - isTyping

    messageReceived:
      name: messageReceived
      payload:
        type: object
        properties:
          roomId:
            type: string
          userId:
            type: string
          message:
            type: string
          timestamp:
            type: number
        required:
          - roomId
          - userId
          - message
          - timestamp

    notification:
      name: notification
      payload:
        type: object
        properties:
          type:
            type: string
          title:
            type: string
          message:
            type: string
          timestamp:
            type: number
        required:
          - type
          - title
          - message
          - timestamp

    invalidMessage:
      name: invalidMessage
      payload:
        type: object
        properties:
          code:
            type: string
          message:
            type: string
        required:
          - code
          - message
```

## Channel Mapping

Event messages can map to channels in three ways:

### 1. Explicit Channel

Specify a channel for the message:

```typescript
{
  clientMessages: {
    sendMessage: {
      shape: chatMessageSchema,
      channel: 'chat'  // Explicit channel name
    }
  }
}
```

### 2. Multiple Channels

Map a message to multiple channels:

```typescript
{
  serverMessages: {
    broadcast: {
      shape: broadcastSchema,
      channels: ['chat', 'notifications', 'announcements']
    }
  }
}
```

### 3. Default (Message Key as Channel)

If no channel is specified, the message key becomes the channel name:

```typescript
{
  serverMessages: {
    userNotification: {
      shape: notificationSchema
      // Channel will be 'userNotification'
    }
  }
}
```

## Real-World Example

Complete WebSocket API with AsyncAPI generation:

```typescript
// ws/game-events.ts
import { EventSchema } from '@forklaunch/framework/ws';
import { generateAsyncApi } from '@forklaunch/framework/ws';
import { schemaValidator } from '@/validators';
import { object, string, number, boolean, array } from '@forklaunch/blueprint-core';

// Game state schema
const gameStateSchema = object({
  gameId: string,
  players: array(object({
    id: string,
    name: string,
    score: number,
    position: object({
      x: number,
      y: number
    })
  })),
  status: string  // 'waiting', 'playing', 'finished'
});

// Player action schema
const playerActionSchema = object({
  gameId: string,
  playerId: string,
  action: string,  // 'move', 'attack', 'defend'
  target: object({
    x: number,
    y: number
  }).optional
});

// Event schemas
export const gameEventSchemas: EventSchema<typeof schemaValidator> = {
  clientMessages: {
    joinGame: {
      shape: object({
        gameId: string,
        playerId: string,
        playerName: string
      }),
      channel: 'game-lobby'
    },
    playerAction: {
      shape: playerActionSchema,
      channel: 'game-play'
    },
    chatMessage: {
      shape: object({
        gameId: string,
        playerId: string,
        message: string
      }),
      channel: 'game-chat'
    }
  },

  serverMessages: {
    gameState: {
      shape: gameStateSchema,
      channel: 'game-play'
    },
    playerJoined: {
      shape: object({
        gameId: string,
        playerId: string,
        playerName: string
      }),
      channel: 'game-lobby'
    },
    chatBroadcast: {
      shape: object({
        gameId: string,
        playerId: string,
        playerName: string,
        message: string,
        timestamp: number
      }),
      channel: 'game-chat'
    }
  },

  errors: {
    invalidAction: {
      shape: object({
        code: string,
        message: string,
        action: string.optional
      })
    },
    gameNotFound: {
      shape: object({
        code: string,
        message: string,
        gameId: string
      })
    }
  }
};

// Generate AsyncAPI
const asyncApiDoc = generateAsyncApi({
  schemas: gameEventSchemas,
  info: {
    title: 'Multiplayer Game API',
    version: '2.0.0',
    description: 'Real-time multiplayer game WebSocket API'
  },
  servers: {
    development: {
      url: 'ws://localhost:3000/game',
      protocol: 'ws'
    },
    production: {
      url: 'wss://game.example.com',
      protocol: 'wss'
    }
  }
});

// Export
import fs from 'fs';
import yaml from 'js-yaml';

fs.writeFileSync('game-asyncapi.yaml', yaml.dump(asyncApiDoc));
console.log('AsyncAPI spec generated: game-asyncapi.yaml');
```

## Integration with WebSocket Server

Use event schemas with your WebSocket server:

```typescript
// ws/server.ts
import { WebSocketServer } from 'ws';
import { gameEventSchemas } from './game-events';

const wss = new WebSocketServer({ port: 3000 });

wss.on('connection', (ws, req) => {
  console.log('Client connected');

  ws.on('message', (data) => {
    try {
      const message = JSON.parse(data.toString());

      // Validate against schema
      if (message.type === 'playerAction') {
        const clientMessages = gameEventSchemas.clientMessages;
        const schema = clientMessages.playerAction.shape;

        // Validation happens here
        const validated = schemaValidator.validate(schema, message.payload);

        // Process action
        handlePlayerAction(validated);
      }
    } catch (error) {
      // Send error message
      ws.send(JSON.stringify({
        type: 'error',
        payload: {
          code: 'VALIDATION_ERROR',
          message: error.message
        }
      }));
    }
  });

  ws.on('close', () => {
    console.log('Client disconnected');
  });
});
```

## Schema Validator Integration

ForkLaunch's AsyncAPI generation works with the schema validator abstraction:

```typescript
// validators/index.ts
import { createZodValidator } from '@forklaunch/validator-zod';
import { createTypeBoxValidator } from '@forklaunch/validator-typebox';

// Use Zod
export const schemaValidator = createZodValidator();

// Or use TypeBox
// export const schemaValidator = createTypeBoxValidator();
```

The validator provides the `openapi()` method used by AsyncAPI generation to convert schemas to OpenAPI/AsyncAPI format.

## Viewing AsyncAPI Documentation

Use AsyncAPI tools to visualize generated specifications:

### AsyncAPI Studio

1. Go to [studio.asyncapi.com](https://studio.asyncapi.com)
2. Upload your `asyncapi.yaml` file
3. View interactive documentation

### AsyncAPI Generator

```bash
# Install AsyncAPI Generator
npm install -g @asyncapi/generator

# Generate HTML documentation
ag asyncapi.yaml @asyncapi/html-template -o docs/

# Generate Markdown documentation
ag asyncapi.yaml @asyncapi/markdown-template -o docs/

# Serve documentation locally
cd docs && python3 -m http.server 8080
```

## Best Practices

### 1. Organize Event Schemas by Feature

```typescript
// ws/chat-events.ts
export const chatEventSchemas: EventSchema = { /* ... */ };

// ws/notifications-events.ts
export const notificationEventSchemas: EventSchema = { /* ... */ };

// ws/game-events.ts
export const gameEventSchemas: EventSchema = { /* ... */ };
```

### 2. Use Descriptive Message Names

```typescript
// ✅ Good: Clear message intent
{
  clientMessages: {
    sendChatMessage: { /* ... */ },
    joinChatRoom: { /* ... */ },
    leaveChatRoom: { /* ... */ }
  }
}

// ❌ Bad: Generic names
{
  clientMessages: {
    message1: { /* ... */ },
    message2: { /* ... */ }
  }
}
```

### 3. Group Related Messages by Channel

```typescript
// ✅ Good: Logical channel grouping
{
  clientMessages: {
    sendMessage: { channel: 'chat' },
    userTyping: { channel: 'chat' },
    userStoppedTyping: { channel: 'chat' }
  }
}
```

### 4. Document Error Schemas

```typescript
{
  errors: {
    validationError: {
      shape: object({
        code: string,
        message: string,
        field: string.optional,
        details: array(string).optional
      })
    },
    authenticationError: {
      shape: object({
        code: string,
        message: string
      })
    }
  }
}
```

### 5. Version Your API

Include version in AsyncAPI info:

```typescript
const asyncApiDoc = generateAsyncApi({
  schemas: eventSchemas,
  info: {
    title: 'My WebSocket API',
    version: '2.1.0',  // Semantic versioning
    description: 'Version 2.1.0 - Added typing indicators'
  },
  // ...
});
```

## Limitations

### Queue-Based Messaging Not Supported

AsyncAPI generation is **only for WebSocket channels**. The following do **not** have AsyncAPI support:

- BullMQ queues
- Redis queues
- Kafka topics
- Database queues

These queue systems have their own message schemas but do not integrate with `generateAsyncApi()`.

### Single EventSchema Per Generation

Each call to `generateAsyncApi()` takes one `EventSchema`. To document multiple WebSocket endpoints, generate separate AsyncAPI specs or merge schemas:

```typescript
// Option 1: Separate specs
const chatSpec = generateAsyncApi({ schemas: chatEventSchemas, /* ... */ });
const gameSpec = generateAsyncApi({ schemas: gameEventSchemas, /* ... */ });

// Option 2: Merge schemas
const combinedSchemas: EventSchema = {
  clientMessages: {
    ...chatEventSchemas.clientMessages,
    ...gameEventSchemas.clientMessages
  },
  serverMessages: {
    ...chatEventSchemas.serverMessages,
    ...gameEventSchemas.serverMessages
  }
};

const combinedSpec = generateAsyncApi({ schemas: combinedSchemas, /* ... */ });
```

## Related Documentation

- [Contract-First Development](/docs/guides/contract-first-development.md)
- [Dependency Management](/docs/guides/dependency-management.md)
- [Queues](/docs/infrastructure/queues.md)
