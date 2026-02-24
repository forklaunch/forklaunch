import {
  createWebSocketSchemas,
  EventSchema,
  ServerEventSchema
} from '@forklaunch/core/ws';
import type { AnySchemaValidator } from '@forklaunch/validator';
import { SchemaValidator } from '@forklaunch/validator';
import type { IncomingMessage } from 'http';
import { WebSocketServer } from 'ws';
import { ForklaunchWebSocket } from './webSocket';

/**
 * Extended WebSocketServer that automatically injects schema validation into all connected clients.
 *
 * When clients connect, they receive a {@link ForklaunchWebSocket} instance with automatic:
 * - Message validation against provided schemas
 * - Buffer ↔ Object transformation
 * - Type-safe event handlers
 *
 * Note: The server-side WebSocket swaps `clientMessages` and `serverMessages` schemas,
 * so what the client sends as `clientMessages` arrives at the server as `serverMessages`.
 *
 * @template SV - The schema validator type (e.g., ZodSchemaValidator)
 * @template ES - The event schema defining message types for each event
 *
 * @example Basic Chat Server
 * ```typescript
 * import { ZodSchemaValidator } from '@forklaunch/validator';
 * import { z } from 'zod';
 * import { ForklaunchWebSocketServer } from '@forklaunch/ws';
 *
 * const validator = new ZodSchemaValidator();
 * const schemas = {
 *   ping: z.object({ ts: z.number() }),
 *   pong: z.object({ ts: z.number() }),
 *   clientMessages: z.discriminatedUnion('type', [
 *     z.object({ type: z.literal('chat'), message: z.string(), userId: z.string() }),
 *     z.object({ type: z.literal('join'), roomId: z.string() })
 *   ]),
 *   serverMessages: z.discriminatedUnion('type', [
 *     z.object({ type: z.literal('chat'), message: z.string(), userId: z.string() }),
 *     z.object({ type: z.literal('user-joined'), userId: z.string() })
 *   ])
 * };
 *
 * const wss = new ForklaunchWebSocketServer(validator, schemas, { port: 8080 });
 *
 * wss.on('connection', (ws, request) => {
 *   console.log('Client connected:', request.socket.remoteAddress);
 *
 *   // Send welcome message (validated and encoded automatically)
 *   ws.send({ type: 'chat', message: 'Welcome!', userId: 'server' });
 *
 *   // Listen for incoming messages (validated and decoded automatically)
 *   ws.on('message', (data, isBinary) => {
 *     console.log('Received:', data); // data is typed and validated
 *
 *     // Broadcast to other clients
 *     wss.clients.forEach((client) => {
 *       if (client !== ws && client.readyState === WebSocket.OPEN) {
 *         client.send(data);
 *       }
 *     });
 *   });
 *
 *   ws.on('close', (code, reason) => {
 *     console.log('Client disconnected:', code, reason);
 *   });
 * });
 * ```
 *
 * @example With Authentication
 * ```typescript
 * wss.on('connection', (ws, request) => {
 *   const token = new URL(request.url!, 'ws://localhost').searchParams.get('token');
 *
 *   if (!token || !isValidToken(token)) {
 *     ws.close(1008, { reason: 'Unauthorized' });
 *     return;
 *   }
 *
 *   // Proceed with authenticated connection
 *   ws.on('message', (data) => {
 *     // Handle authenticated messages
 *   });
 * });
 * ```
 *
 * @see {@link ForklaunchWebSocket} for client-side usage
 * @see {@link EventSchema} for schema structure
 */
export class ForklaunchWebSocketServer<
  SV extends AnySchemaValidator,
  const ES extends EventSchema<SV>
> extends WebSocketServer {
  private readonly schemaValidator: SV;
  private readonly eventSchemas: ES;

  /**
   * Creates a new ForklaunchWebSocketServer with schema validation.
   *
   * @param schemaValidator - The schema validator instance (e.g., ZodSchemaValidator)
   * @param eventSchemas - Schema definitions for all WebSocket events
   * @param options - Standard WebSocketServer options (port, host, server, etc.)
   * @param callback - Optional callback invoked when the server starts listening
   *
   * @example Create a server on port 8080
   * ```typescript
   * const wss = new ForklaunchWebSocketServer(
   *   validator,
   *   schemas,
   *   { port: 8080 },
   *   () => console.log('Server started on port 8080')
   * );
   * ```
   *
   * @example Create a server with existing HTTP server
   * ```typescript
   * import { createServer } from 'http';
   *
   * const httpServer = createServer();
   * const wss = new ForklaunchWebSocketServer(
   *   validator,
   *   schemas,
   *   { server: httpServer }
   * );
   *
   * httpServer.listen(8080);
   * ```
   *
   * @example No server mode (for upgrade handling)
   * ```typescript
   * const wss = new ForklaunchWebSocketServer(
   *   validator,
   *   schemas,
   *   { noServer: true }
   * );
   *
   * httpServer.on('upgrade', (request, socket, head) => {
   *   wss.handleUpgrade(request, socket, head, (ws) => {
   *     wss.emit('connection', ws, request);
   *   });
   * });
   * ```
   */
  constructor(
    schemaValidator: SV,
    eventSchemas: ES,
    options?: ConstructorParameters<typeof WebSocketServer>[0],
    callback?: () => void
  ) {
    super(options, callback);
    this.schemaValidator = schemaValidator;
    this.eventSchemas = eventSchemas;
  }

  /**
   * Intercepts emit to enhance WebSocket instances on 'connection' events.
   *
   * When the ws library internally emits 'connection', we swap the raw WebSocket
   * for an enhanced ForklaunchWebSocket before listeners receive it. This avoids
   * infinite recursion that would occur if we used a listener + re-emit pattern.
   *
   * @internal
   */
  override emit(event: string | symbol, ...args: unknown[]): boolean {
    if (event === 'connection' && args.length >= 1) {
      const ws = args[0] as InstanceType<typeof import('ws').WebSocket>;
      const enhancedWs = this.enhanceWebSocket(ws);
      return super.emit(event, enhancedWs, ...args.slice(1));
    }
    return super.emit(event, ...args);
  }

  /**
   * Enhances a plain WebSocket instance with ForklaunchWebSocket functionality.
   *
   * This method:
   * 1. Changes the prototype to ForklaunchWebSocket.prototype to inject all methods
   * 2. Initializes all required properties (schemaValidator, eventSchemas, schemas)
   * 3. Ensures methods like on(), send(), close() etc. work with validation
   *
   * @internal
   */
  private enhanceWebSocket(
    ws: InstanceType<typeof import('ws').WebSocket>
  ): ForklaunchWebSocket<SV, ServerEventSchema<SV, ES>> {
    Object.setPrototypeOf(ws, ForklaunchWebSocket.prototype);

    const enhancedWs = ws as unknown as ForklaunchWebSocket<
      SV,
      ServerEventSchema<SV, ES>
    >;
    // @ts-expect-error - Intentionally restricting types for compile-time schema validation
    enhancedWs.schemaValidator = this.schemaValidator;

    // @ts-expect-error - Intentionally restricting types for compile-time schema validation
    enhancedWs.eventSchemas = this.eventSchemas as ServerEventSchema<SV, ES>;

    const serverEventSchemas: ServerEventSchema<SV, ES> = {
      ...this.eventSchemas,
      clientMessages: this.eventSchemas.serverMessages,
      serverMessages: this.eventSchemas.clientMessages
    } as ServerEventSchema<SV, ES>;

    // @ts-expect-error - Intentionally restricting types for compile-time schema validation
    enhancedWs.schemas = createWebSocketSchemas<SV, ServerEventSchema<SV, ES>>(
      this.schemaValidator as SchemaValidator,
      serverEventSchemas
    );

    return enhancedWs as ForklaunchWebSocket<SV, ServerEventSchema<SV, ES>>;
  }

  /**
   * Registers an event listener for server events.
   *
   * The `connection` event provides a {@link ForklaunchWebSocket} instance (not a plain WebSocket),
   * which includes automatic validation and transformation for all messages.
   *
   * @param event - The server event name to listen for
   * @param listener - The callback function to invoke when the event occurs
   * @returns `this` for method chaining
   *
   * @example Handle new connections
   * ```typescript
   * wss.on('connection', (ws, request) => {
   *   console.log('Client connected from:', request.socket.remoteAddress);
   *
   *   ws.on('message', (data, isBinary) => {
   *     console.log('Received:', data); // Auto-validated and typed
   *   });
   * });
   * ```
   *
   * @example Handle server errors
   * ```typescript
   * wss.on('error', (error) => {
   *   console.error('Server error:', error);
   * });
   * ```
   *
   * @example Listen for server lifecycle events
   * ```typescript
   * wss.on('listening', () => {
   *   console.log('Server is listening');
   * });
   *
   * wss.on('close', () => {
   *   console.log('Server closed');
   * });
   * ```
   *
   * @remarks
   * Note: TypeScript shows `ws` as `WebSocket` in the type signature due to override
   * limitations, but at runtime it's actually a `ForklaunchWebSocket` with full validation.
   */
  on(
    event: 'connection',
    listener: (
      ws: ForklaunchWebSocket<SV, ServerEventSchema<SV, ES>>,
      request: IncomingMessage
    ) => void
  ): this;
  on(event: 'error', listener: (error: Error) => void): this;
  on(
    event: 'headers',
    listener: (headers: string[], request: IncomingMessage) => void
  ): this;
  on(event: 'close', listener: () => void): this;
  on(event: 'listening', listener: () => void): this;
  on(event: string | symbol, listener: (...args: never[]) => void): this;
  on(event: string | symbol, listener: (...args: never[]) => void): this {
    return super.on(event, listener as (...args: unknown[]) => void);
  }

  /**
   * Registers a one-time event listener for server events.
   *
   * The listener will be invoked at most once after being registered, and then removed.
   *
   * @param event - The server event name to listen for
   * @param listener - The callback function to invoke when the event occurs
   * @returns `this` for method chaining
   *
   * @example Wait for first connection
   * ```typescript
   * wss.once('connection', (ws, request) => {
   *   console.log('First client connected!');
   *   // This will only fire for the first connection
   * });
   * ```
   *
   * @example Wait for server to start
   * ```typescript
   * wss.once('listening', () => {
   *   console.log('Server is now ready to accept connections');
   * });
   * ```
   */
  once(
    event: 'connection',
    listener: (
      ws: ForklaunchWebSocket<SV, ServerEventSchema<SV, ES>>,
      request: IncomingMessage
    ) => void
  ): this;
  once(event: 'error', listener: (error: Error) => void): this;
  once(
    event: 'headers',
    listener: (headers: string[], request: IncomingMessage) => void
  ): this;
  once(event: 'close', listener: () => void): this;
  once(event: 'listening', listener: () => void): this;
  once(event: string | symbol, listener: (...args: never[]) => void): this;
  once(event: string | symbol, listener: (...args: never[]) => void): this {
    return super.once(event, listener as (...args: unknown[]) => void);
  }

  /**
   * Removes a previously registered event listener.
   *
   * To successfully remove a listener, you must pass the exact same function
   * reference that was used when registering it.
   *
   * @param event - The server event name
   * @param listener - The exact function reference to remove
   * @returns `this` for method chaining
   *
   * @example
   * ```typescript
   * const connectionHandler = (ws, request) => {
   *   console.log('Client connected');
   * };
   *
   * // Register
   * wss.on('connection', connectionHandler);
   *
   * // Later, remove
   * wss.off('connection', connectionHandler);
   * ```
   */
  off(
    event: 'connection',
    listener: (
      ws: ForklaunchWebSocket<SV, ServerEventSchema<SV, ES>>,
      request: IncomingMessage
    ) => void
  ): this;
  off(event: 'error', listener: (error: Error) => void): this;
  off(
    event: 'headers',
    listener: (headers: string[], request: IncomingMessage) => void
  ): this;
  off(event: 'close', listener: () => void): this;
  off(event: 'listening', listener: () => void): this;
  off(event: string | symbol, listener: (...args: never[]) => void): this;
  off(event: string | symbol, listener: (...args: never[]) => void): this {
    return super.off(event, listener as (...args: unknown[]) => void);
  }

  /**
   * Registers an event listener (alias for `on()`).
   *
   * This method is functionally identical to `on()` and is provided for
   * compatibility with the EventEmitter API.
   *
   * @param event - The server event name to listen for
   * @param listener - The callback function to invoke when the event occurs
   * @returns `this` for method chaining
   *
   * @see {@link on} for detailed documentation and examples
   */
  addListener(
    event: 'connection',
    listener: (
      ws: ForklaunchWebSocket<SV, ServerEventSchema<SV, ES>>,
      request: IncomingMessage
    ) => void
  ): this;
  addListener(event: 'error', listener: (error: Error) => void): this;
  addListener(
    event: 'headers',
    listener: (headers: string[], request: IncomingMessage) => void
  ): this;
  addListener(event: 'close', listener: () => void): this;
  addListener(event: 'listening', listener: () => void): this;
  addListener(
    event: string | symbol,
    listener: (...args: never[]) => void
  ): this;
  addListener(
    event: string | symbol,
    listener: (...args: never[]) => void
  ): this {
    return super.addListener(event, listener as (...args: unknown[]) => void);
  }

  /**
   * Removes a previously registered event listener (alias for `off()`).
   *
   * This method is functionally identical to `off()` and is provided for
   * compatibility with the EventEmitter API.
   *
   * @param event - The server event name
   * @param listener - The exact function reference to remove
   * @returns `this` for method chaining
   *
   * @see {@link off} for detailed documentation and examples
   */
  removeListener(
    event: 'connection',
    listener: (
      ws: ForklaunchWebSocket<SV, ServerEventSchema<SV, ES>>,
      request: IncomingMessage
    ) => void
  ): this;
  removeListener(event: 'error', listener: (error: Error) => void): this;
  removeListener(
    event: 'headers',
    listener: (headers: string[], request: IncomingMessage) => void
  ): this;
  removeListener(event: 'close', listener: () => void): this;
  removeListener(event: 'listening', listener: () => void): this;
  removeListener(
    event: string | symbol,
    listener: (...args: never[]) => void
  ): this;
  removeListener(
    event: string | symbol,
    listener: (...args: never[]) => void
  ): this {
    return super.removeListener(
      event,
      listener as (...args: unknown[]) => void
    );
  }
}
