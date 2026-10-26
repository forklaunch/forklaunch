import type { EventSchema } from '@forklaunch/core/ws';
import { ZodSchemaValidator } from '@forklaunch/validator/zod';
import { afterEach, describe, expect, it } from 'vitest';
import { WebSocket, WebSocketServer } from 'ws';
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
  WebSocket | ForklaunchWebSocket<ZodSchemaValidator, typeof schemas>;

function waitForOpen(ws: TestClient): Promise<void> {
  return new Promise((resolve) => {
    if (ws.readyState === WebSocket.OPEN) {
      resolve();
    } else {
      ws.on('open', () => resolve());
    }
  });
}

async function createServer(
  eventSchemas = schemas,
  options: { autoPong?: boolean } = {}
) {
  const server = new ForklaunchWebSocketServer(validator, eventSchemas, {
    port: 0,
    ...options
  });
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

  it('echoes encoded automatic pong and close payloads byte for byte', async () => {
    const result = await createServer();
    server = result.server;
    const client = new WebSocket(`ws://localhost:${result.port}`);
    clients.push(client);
    await waitForOpen(client);
    const pingBytes = Buffer.from(' { "ts" : 42 } ');
    const pong = new Promise<Buffer>((resolve) => client.once('pong', resolve));
    client.ping(pingBytes);
    expect(await pong).toEqual(pingBytes);
    const reasonBytes = Buffer.from(' { "code" : 7, "message" : "done 水" } ');
    const closed = new Promise<[number, Buffer]>((resolve) => {
      client.once('close', (code, reason) => resolve([code, reason]));
    });
    client.close(1000, reasonBytes);
    expect(await closed).toEqual([1000, reasonBytes]);
  });

  it('reports an automatic pong schema failure and releases the socket', async () => {
    const result = await createServer({
      ...schemas,
      pong: { shape: z.object({ ts: z.number().min(100) }) }
    });
    server = result.server;
    const failure = new Promise<Error>((resolve) => {
      server!.once('connection', (socket) => socket.on('error', resolve));
    });
    const client = new WebSocket(`ws://localhost:${result.port}`);
    clients.push(client);
    await waitForOpen(client);
    const closed = new Promise<number>((resolve) =>
      client.once('close', resolve)
    );
    client.ping(JSON.stringify({ ts: 1 }));
    expect(await failure).toBeInstanceOf(Error);
    expect(await closed).toBe(1008);
  });

  it('reports an invalid automatic close reply and releases the socket', async () => {
    const result = await createServer();
    server = result.server;
    const failure = new Promise<Error>((resolve) => {
      server!.once('connection', (socket) => socket.on('error', resolve));
    });
    const client = new WebSocket(`ws://localhost:${result.port}`);
    clients.push(client);
    await waitForOpen(client);
    const closed = new Promise<number>((resolve) =>
      client.once('close', resolve)
    );
    client.close(1000, 'invalid typed reason');
    expect(await failure).toBeInstanceOf(Error);
    expect(await closed).toBe(1008);
  });

  it('reports client automatic pong failures once without delivering the ping or typed close', async () => {
    const peer = new WebSocketServer({ port: 0 });
    await new Promise<void>((resolve) => peer.once('listening', resolve));
    const address = peer.address() as { port: number };
    try {
      const client = new ForklaunchWebSocket(
        validator,
        {
          ...schemas,
          pong: { shape: z.object({ ts: z.number().min(100) }) }
        },
        `ws://localhost:${address.port}`
      );
      clients.push(client as TestClient);
      const errors: Error[] = [];
      client.on('error', (error) => errors.push(error));
      let pings = 0,
        closes = 0;
      client.on('ping', () => pings++);
      client.on('close', () => closes++);
      const received = new Promise<WebSocket>((resolve) =>
        peer.once('connection', resolve)
      );
      await waitForOpen(client as TestClient);
      const socket = await received;
      const closed = new Promise<number>((resolve) =>
        socket.once('close', resolve)
      );
      socket.ping(JSON.stringify({ ts: 1 }));
      expect(await closed).toBe(1008);
      expect(errors).toHaveLength(1);
      expect(pings).toBe(0);
      expect(closes).toBe(0);
    } finally {
      for (const socket of peer.clients) socket.terminate();
      await new Promise<void>((resolve) => peer.close(() => resolve()));
    }
  });

  it('reports abnormal client disconnects through error handlers with required close schemas', async () => {
    const peer = new WebSocketServer({ port: 0 });
    await new Promise<void>((resolve) => peer.once('listening', resolve));
    const address = peer.address() as { port: number };
    try {
      const client = new ForklaunchWebSocket(
        validator,
        schemas,
        `ws://localhost:${address.port}`
      );
      clients.push(client);
      const failure = new Promise<Error>((resolve) =>
        client.on('error', resolve)
      );
      let typedCloses = 0;
      client.on('close', () => typedCloses++);
      const received = new Promise<WebSocket>((resolve) =>
        peer.once('connection', resolve)
      );
      await waitForOpen(client);
      (await received).terminate();
      expect((await failure).message).toContain('without a handshake');
      expect(typedCloses).toBe(0);
    } finally {
      for (const socket of peer.clients) socket.terminate();
      await new Promise<void>((resolve) => peer.close(() => resolve()));
    }
  });

  it('honors disabled automatic pong without requiring a matching outgoing pong', async () => {
    const peer = new WebSocketServer({ port: 0 });
    await new Promise<void>((resolve) => peer.once('listening', resolve));
    const address = peer.address() as { port: number };
    try {
      const client = new ForklaunchWebSocket(
        validator,
        {
          ...schemas,
          pong: { shape: z.object({ ts: z.number().min(100) }) }
        },
        `ws://localhost:${address.port}`,
        undefined,
        { autoPong: false }
      );
      clients.push(client as TestClient);
      client.on('error', () => {});
      const received = new Promise<WebSocket>((resolve) =>
        peer.once('connection', resolve)
      );
      await waitForOpen(client as TestClient);
      const socket = await received;
      const ping = new Promise<unknown>((resolve) =>
        client.once('ping', resolve)
      );
      socket.ping(JSON.stringify({ ts: 1 }));
      expect(await ping).toEqual({ ts: 1 });
      expect(client.readyState).toBe(WebSocket.OPEN);
      const closed = new Promise<void>((resolve) =>
        socket.once('close', () => resolve())
      );
      client.terminate();
      await closed;
    } finally {
      for (const socket of peer.clients) socket.terminate();
      await new Promise<void>((resolve) => peer.close(() => resolve()));
    }
  });

  it('keeps invalid manual control payloads local to the caller', async () => {
    const result = await createServer();
    server = result.server;
    const checked = new Promise<void>((resolve, reject) =>
      server!.once('connection', (socket) => {
        try {
          expect(() =>
            Reflect.apply(socket.pong, socket, [Buffer.from('invalid')])
          ).toThrow();
          expect(() =>
            Reflect.apply(socket.close, socket, [1000, Buffer.from('invalid')])
          ).toThrow();
          expect(socket.readyState).toBe(WebSocket.OPEN);
          resolve();
        } catch (error) {
          reject(error);
        }
      })
    );
    const client = new WebSocket(`ws://localhost:${result.port}`);
    clients.push(client);
    await waitForOpen(client);
    await checked;
  });

  it('honors disabled server automatic pong', async () => {
    const result = await createServer(
      {
        ...schemas,
        pong: { shape: z.object({ ts: z.number().min(100) }) }
      },
      { autoPong: false }
    );
    server = result.server;
    const received = new Promise<unknown>((resolve) =>
      server!.once('connection', (socket) => {
        socket.once('ping', resolve);
      })
    );
    const client = new WebSocket(`ws://localhost:${result.port}`);
    clients.push(client);
    await waitForOpen(client);
    client.ping(JSON.stringify({ ts: 1 }));
    expect(await received).toEqual({ ts: 1 });
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
