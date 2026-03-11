import type { EventSchema } from '@forklaunch/core/ws';
import { ZodSchemaValidator } from '@forklaunch/validator/zod';
import { afterEach, describe, expect, it } from 'vitest';
import { WebSocket } from 'ws';
import { z } from 'zod/v3';
import { ForklaunchWebSocket } from '../webSocket';
import { ForklaunchWebSocketServer } from '../webSocketServer';

const schemas = {
  ping: { shape: z.object({ ts: z.number() }) },
  pong: { shape: z.object({ ts: z.number() }) },
  serverMessages: {
    chat: {
      shape: z.object({
        type: z.literal('chat'),
        message: z.string(),
        userId: z.string()
      })
    }
  },
  clientMessages: {
    response: {
      shape: z.object({
        status: z.string(),
        data: z.unknown()
      })
    }
  },
  errors: {
    error: {
      shape: z.object({
        code: z.string(),
        message: z.string()
      })
    }
  },
  closeReason: {
    reason: {
      shape: z.object({
        code: z.number(),
        message: z.string()
      })
    }
  }
} satisfies EventSchema<ZodSchemaValidator>;

const validator = new ZodSchemaValidator();

type TestClient =
  | WebSocket
  | ForklaunchWebSocket<ZodSchemaValidator, typeof schemas>;

function waitForOpen(ws: TestClient): Promise<void> {
  return new Promise((resolve) => {
    if (ws.readyState === WebSocket.OPEN) {
      resolve();
    } else {
      ws.on('open', () => resolve());
    }
  });
}

async function createServer() {
  const server = new ForklaunchWebSocketServer(validator, schemas, { port: 0 });
  const port = await new Promise<number>((resolve) => {
    server.on('listening', () => {
      const addr = server.address();
      resolve((addr as { port: number }).port);
    });
  });
  return { server, port };
}

describe('WebSocket Integration', () => {
  let server: ForklaunchWebSocketServer<
    ZodSchemaValidator,
    typeof schemas
  > | null = null;
  const clients: TestClient[] = [];

  afterEach(async () => {
    for (const client of clients) {
      if (client.readyState === WebSocket.OPEN) {
        client.close();
      }
    }
    clients.length = 0;

    if (server) {
      // Close all server-side sockets
      for (const ws of server.clients) {
        ws.terminate();
      }
      await new Promise<void>((resolve) => {
        server!.close(() => resolve());
      });
      server = null;
    }
  });

  it('should not infinitely recurse when a client connects', async () => {
    const result = await createServer();
    server = result.server;

    const connected = new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(
        () => reject(new Error('Connection timed out')),
        3000
      );
      server!.on('connection', () => {
        clearTimeout(timeout);
        resolve();
      });
    });

    const client = new WebSocket(`ws://localhost:${result.port}`);
    clients.push(client);

    await connected;
  });

  it('should deliver enhanced ForklaunchWebSocket to connection handler', async () => {
    const result = await createServer();
    server = result.server;

    const wsReceived = new Promise<unknown>((resolve, reject) => {
      const timeout = setTimeout(
        () => reject(new Error('Connection timed out')),
        3000
      );
      server!.on('connection', (ws) => {
        clearTimeout(timeout);
        resolve(ws);
      });
    });

    const client = new WebSocket(`ws://localhost:${result.port}`);
    clients.push(client);

    const serverWs = await wsReceived;
    expect(serverWs).toBeInstanceOf(ForklaunchWebSocket);
  });

  it('should allow server to send validated data to client via ws.send', async () => {
    const result = await createServer();
    server = result.server;

    server.on('connection', (ws) => {
      // Server sends a message (server's clientMessages = original serverMessages = chat)
      ws.send({ type: 'chat', message: 'hello', userId: 'server' });
    });

    const client = new WebSocket(`ws://localhost:${result.port}`);
    clients.push(client);

    const messageReceived = new Promise<unknown>((resolve, reject) => {
      const timeout = setTimeout(
        () => reject(new Error('Message timed out')),
        3000
      );
      client.on('message', (data) => {
        clearTimeout(timeout);
        resolve(JSON.parse(data.toString()));
      });
    });

    const msg = await messageReceived;
    expect(msg).toEqual({
      type: 'chat',
      message: 'hello',
      userId: 'server'
    });
  });

  it('should allow client to send data and server to receive it via on("message")', async () => {
    const result = await createServer();
    server = result.server;

    const messageReceived = new Promise<unknown>((resolve, reject) => {
      const timeout = setTimeout(
        () => reject(new Error('Message timed out')),
        3000
      );
      server!.on('connection', (ws) => {
        ws.on('message', (data) => {
          clearTimeout(timeout);
          resolve(data);
        });
      });
    });

    const client = new WebSocket(`ws://localhost:${result.port}`);
    clients.push(client);
    await waitForOpen(client);

    const payload = { status: 'ok', data: { foo: 'bar' } };
    client.send(JSON.stringify(payload));

    const msg = await messageReceived;
    expect(msg).toEqual(payload);
  });

  it('should allow ForklaunchWebSocket client to send validated data to server', async () => {
    const result = await createServer();
    server = result.server;

    const messageReceived = new Promise<unknown>((resolve, reject) => {
      const timeout = setTimeout(
        () => reject(new Error('Message timed out')),
        3000
      );
      server!.on('connection', (ws) => {
        ws.on('message', (data) => {
          clearTimeout(timeout);
          resolve(data);
        });
      });
    });

    const client = new ForklaunchWebSocket(
      validator,
      schemas,
      `ws://localhost:${result.port}`
    );
    clients.push(client);
    await waitForOpen(client);

    client.send({ status: 'ok', data: { foo: 'bar' } });

    const msg = await messageReceived;
    expect(msg).toEqual({ status: 'ok', data: { foo: 'bar' } });
  });

  it('should allow bidirectional communication', async () => {
    const result = await createServer();
    server = result.server;

    // Server echoes back with a chat message when it receives a response
    server.on('connection', (ws) => {
      ws.on('message', (data) => {
        const msg = data as { status: string; data: unknown };
        ws.send({
          type: 'chat',
          message: `Echo: ${msg.status}`,
          userId: 'server'
        });
      });
    });

    const client = new ForklaunchWebSocket(
      validator,
      schemas,
      `ws://localhost:${result.port}`
    );
    clients.push(client);
    await waitForOpen(client);

    const responseReceived = new Promise<unknown>((resolve, reject) => {
      const timeout = setTimeout(
        () => reject(new Error('Response timed out')),
        3000
      );
      client.on('message', (data) => {
        clearTimeout(timeout);
        resolve(data);
      });
    });

    client.send({ status: 'ok', data: null });

    const msg = await responseReceived;
    expect(msg).toEqual({
      type: 'chat',
      message: 'Echo: ok',
      userId: 'server'
    });
  });

  describe('Outgoing validation (send)', () => {
    it('should reject invalid data from ForklaunchWebSocket client send()', async () => {
      const result = await createServer();
      server = result.server;

      const client = new ForklaunchWebSocket(
        validator,
        schemas,
        `ws://localhost:${result.port}`
      );
      clients.push(client);
      await waitForOpen(client);

      // clientMessages expects { status: string, data: unknown }
      // Sending an object that doesn't match should throw
      expect(() => {
        // @ts-expect-error - Intentionally sending invalid data
        client.send({ wrong: 'shape' });
      }).toThrow();
    });

    it('should reject invalid data from server-side ws.send()', async () => {
      const result = await createServer();
      server = result.server;

      const sendError = new Promise<Error>((resolve, reject) => {
        const timeout = setTimeout(
          () => reject(new Error('Timed out waiting for error')),
          3000
        );
        server!.on('connection', (ws) => {
          clearTimeout(timeout);
          try {
            // Server's clientMessages = original serverMessages = chat schema
            // Sending data that doesn't match the chat schema should throw
            // @ts-expect-error - Intentionally sending invalid data
            ws.send({ wrong: 'shape' });
            reject(new Error('Expected send to throw'));
          } catch (err) {
            resolve(err as Error);
          }
        });
      });

      const client = new WebSocket(`ws://localhost:${result.port}`);
      clients.push(client);

      const err = await sendError;
      expect(err).toBeInstanceOf(Error);
      expect(err.message).toContain('validation failed');
    });
  });

  describe('Incoming validation (on message)', () => {
    it('should reject invalid incoming data on server-side on("message")', async () => {
      const result = await createServer();
      server = result.server;

      const validationError = new Promise<Error>((resolve, reject) => {
        const timeout = setTimeout(
          () => reject(new Error('Timed out waiting for error')),
          3000
        );
        server!.on('connection', (ws) => {
          // The on() wrapper validates incoming data against serverMessagesSchema
          // (swapped: server's serverMessages = original clientMessages = response schema)
          // Sending data that doesn't match { status: string, data: unknown } should cause
          // the validation to throw, which the ws library surfaces as an 'error' event
          ws.on('error', (err) => {
            clearTimeout(timeout);
            resolve(err as unknown as Error);
          });
          ws.on('message', () => {
            clearTimeout(timeout);
            reject(new Error('Expected message handler not to be called'));
          });
        });
      });

      const client = new WebSocket(`ws://localhost:${result.port}`);
      clients.push(client);
      await waitForOpen(client);

      // Send data that doesn't match the response schema { status: string, data: unknown }
      client.send(JSON.stringify({ invalid: 'data' }));

      const err = await validationError;
      expect(err).toBeInstanceOf(Error);
      expect(err.message).toContain('validation failed');
    });

    it('should reject invalid incoming data on ForklaunchWebSocket client on("message")', async () => {
      const result = await createServer();
      server = result.server;

      // When a message arrives, manually emit raw data to the server ws
      // to simulate the server sending invalid data
      server.on('connection', (ws) => {
        // Bypass validation by using the raw WebSocket.prototype.send
        // This simulates a misbehaving server sending invalid data
        WebSocket.prototype.send.call(
          ws,
          JSON.stringify({ invalid: 'data' }),
          {}
        );
      });

      const client = new ForklaunchWebSocket(
        validator,
        schemas,
        `ws://localhost:${result.port}`
      );
      clients.push(client);

      const validationError = new Promise<Error>((resolve, reject) => {
        const timeout = setTimeout(
          () => reject(new Error('Timed out waiting for error')),
          3000
        );
        // Client's serverMessages = chat schema { type: 'chat', message: string, userId: string }
        // Invalid data should cause a validation error
        client.on('error', (err) => {
          clearTimeout(timeout);
          resolve(err);
        });
        client.on('message', () => {
          clearTimeout(timeout);
          reject(
            new Error(
              'Expected message handler not to be called with invalid data'
            )
          );
        });
      });

      const err = await validationError;
      expect(err).toBeInstanceOf(Error);
      expect(err.message).toContain('validation failed');
    });
  });
});
