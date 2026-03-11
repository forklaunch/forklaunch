import type {
  ExtractSchemaFromEntry,
  ExtractSchemaFromRecord
} from '@forklaunch/core/ws';
import {
  createWebSocketSchemas,
  decodeSchemaValue,
  encodeSchemaValue,
  EventSchema,
  normalizeEncodedValue
} from '@forklaunch/core/ws';
import {
  AnySchemaValidator,
  IdiomaticSchema,
  Schema,
  SchemaValidator
} from '@forklaunch/validator';
import { ClientRequest, IncomingMessage } from 'http';
import { WebSocket } from 'ws';

/**
 * Event map for outgoing events (emit)
 * Maps event names to their argument signatures for type-safe event emission
 *
 * @internal
 */
type OutgoingEventMap<
  SV extends AnySchemaValidator,
  ES extends EventSchema<SV>
> = {
  message: [
    data: Schema<ExtractSchemaFromRecord<SV, ES['clientMessages']>, SV>,
    isBinary: boolean
  ];
  close: [
    code: number,
    reason: Schema<ExtractSchemaFromRecord<SV, ES['closeReason']>, SV>
  ];
  error: [error: Schema<ExtractSchemaFromRecord<SV, ES['errors']>, SV>];
  open: [];
  ping: [data: Schema<ExtractSchemaFromEntry<SV, ES['ping']>, SV>];
  pong: [data: Schema<ExtractSchemaFromEntry<SV, ES['pong']>, SV>];
};

/**
 * Extended WebSocket with built-in schema validation and automatic data transformation.
 *
 * ForklaunchWebSocket automatically:
 * - Validates incoming and outgoing messages against provided schemas
 * - Decodes Buffer data to JavaScript objects for incoming messages
 * - Encodes JavaScript objects to Buffer data for outgoing messages
 * - Provides type-safe event handlers with full TypeScript support
 *
 * @template SV - The schema validator type (e.g., ZodSchemaValidator)
 * @template ES - The event schema defining message types for each event
 *
 * @example Basic Usage
 * ```typescript
 * import { ZodSchemaValidator } from '@forklaunch/validator';
 * import { z } from 'zod';
 * import { ForklaunchWebSocket } from '@forklaunch/ws';
 *
 * const validator = new ZodSchemaValidator();
 * const schemas = {
 *   ping: z.object({ ts: z.number() }),
 *   pong: z.object({ ts: z.number() }),
 *   clientMessages: z.object({ type: z.string(), data: z.any() }),
 *   serverMessages: z.object({ type: z.string(), data: z.any() }),
 *   errors: z.object({ code: z.number(), message: z.string() }),
 *   closeReason: z.object({ reason: z.string() })
 * };
 *
 * const ws = new ForklaunchWebSocket(
 *   validator,
 *   schemas,
 *   'ws://localhost:8080'
 * );
 *
 * // Incoming messages are automatically validated and decoded
 * ws.on('message', (data, isBinary) => {
 *   console.log('Received:', data); // data is typed and validated
 * });
 *
 * // Outgoing messages are automatically validated and encoded
 * ws.send({ type: 'hello', data: 'world' });
 * ```
 *
 * @example With Custom Schemas
 * ```typescript
 * const chatSchemas = {
 *   ping: z.object({ timestamp: z.number() }),
 *   pong: z.object({ timestamp: z.number() }),
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
 * const ws = new ForklaunchWebSocket(validator, chatSchemas, 'ws://chat.example.com');
 * ```
 *
 * @see {@link EventSchema} for schema structure
 * @see {@link ForklaunchWebSocketServer} for server-side usage
 */
export class ForklaunchWebSocket<
  SV extends AnySchemaValidator,
  const ES extends EventSchema<SV>
> extends WebSocket {
  private eventSchemas: ES;
  private schemas;
  private schemaValidator: SV;

  /**
   * Creates a new ForklaunchWebSocket instance with schema validation.
   *
   * @param schemaValidator - The schema validator instance (e.g., ZodSchemaValidator)
   * @param eventSchemas - Schema definitions for all WebSocket events
   * @param websocketParams - Standard WebSocket constructor parameters (address, protocols, options)
   *
   * @example
   * ```typescript
   * const ws = new ForklaunchWebSocket(
   *   validator,
   *   schemas,
   *   'ws://localhost:8080',
   *   ['chat-protocol'],
   *   { handshakeTimeout: 5000 }
   * );
   * ```
   */
  constructor(
    schemaValidator: SV,
    eventSchemas: ES,
    ...websocketParams: ConstructorParameters<typeof WebSocket>
  ) {
    super(...websocketParams);
    this.schemaValidator = schemaValidator;
    this.eventSchemas = eventSchemas;
    this.schemas = createWebSocketSchemas<SV, ES>(
      schemaValidator as SchemaValidator,
      eventSchemas
    );
  }

  /**
   * Decodes and validates incoming data from the WebSocket.
   *
   * This method handles multiple data formats:
   * - Buffer → decoded to UTF-8 string → parsed as JSON → validated
   * - ArrayBuffer → converted to Buffer → decoded → parsed → validated
   * - TypedArray → converted to Buffer → decoded → parsed → validated
   * - String → parsed as JSON → validated
   * - Object → validated directly (already parsed)
   *
   * @param data - The raw data received from the WebSocket
   * @param schema - Optional schema to validate against
   * @returns The validated and decoded data
   * @throws {Error} If validation fails with pretty-printed error messages
   *
   * @internal
   */
  protected decodeAndValidate(
    data: unknown,
    schema: IdiomaticSchema<SV> | undefined
  ): unknown {
    return decodeSchemaValue(
      this.schemaValidator as SchemaValidator,
      data,
      schema,
      'web socket event'
    );
  }

  /**
   * Validates and encodes outgoing data for transmission over the WebSocket.
   *
   * This method:
   * 1. Validates data against the provided schema (if present)
   * 2. Encodes data to Buffer format:
   *    - Buffer → returned as-is
   *    - Object/Array → JSON.stringify → Buffer
   *    - String → Buffer
   *    - Number/Boolean → JSON.stringify → Buffer
   *    - null/undefined → returned as-is
   *
   * @param data - The data to validate and encode
   * @param schema - Optional schema to validate against
   * @returns The validated and encoded data as Buffer, or null/undefined
   * @throws {Error} If validation fails with pretty-printed error messages
   *
   * @internal
   */
  protected validateAndEncode(
    data: unknown,
    schema: IdiomaticSchema<SV> | undefined,
    allowUndefined = false,
    context = 'web socket event'
  ) {
    const encoded = encodeSchemaValue(
      this.schemaValidator as SchemaValidator,
      data,
      schema,
      context
    );

    return normalizeEncodedValue(encoded, context, allowUndefined);
  }

  /**
   * Transforms incoming event arguments by decoding and validating them.
   *
   * Applies transformation for:
   * - `message` events: validates message data
   * - `close` events: validates close reason
   * - `ping` events: validates ping data
   * - `pong` events: validates pong data
   *
   * @param event - The event name
   * @param args - The raw event arguments
   * @returns Transformed and validated arguments
   *
   * @internal
   */
  protected transformIncomingArgs(
    event: string | symbol,
    args: unknown[]
  ): unknown[] {
    let transformedArgs = args;

    if (event === 'message' && args.length >= 2) {
      const data = args[0];

      const serverSchema = this.schemas.serverMessagesSchema;
      if (serverSchema) {
        const validated = this.decodeAndValidate(data, serverSchema);
        transformedArgs = [validated, false, ...args.slice(2)];
      }
    } else if (event === 'close' && args.length >= 2) {
      const code = args[0];
      const reason = args[1];
      const validated = this.decodeAndValidate(
        reason,
        this.schemas.closeReasonSchema
      );
      transformedArgs = [code, validated, ...args.slice(2)];
    } else if (event === 'ping' && args.length >= 1) {
      const data = args[0];
      const validated = this.decodeAndValidate(data, this.schemas.pingSchema);
      transformedArgs = [validated, ...args.slice(1)];
    } else if (event === 'pong' && args.length >= 1) {
      const data = args[0];
      const validated = this.decodeAndValidate(data, this.schemas.pongSchema);
      transformedArgs = [validated, ...args.slice(1)];
    }

    return transformedArgs;
  }

  /**
   * Wraps an event listener with data transformation logic.
   *
   * This helper intercepts listener registration to inject automatic
   * decoding and validation of incoming event data before passing it
   * to the user's listener function.
   *
   * @param superMethod - The parent class method to call (super.on, super.once, etc.)
   * @param event - The event name
   * @param listener - The user's listener function
   * @returns `this` for method chaining
   *
   * @internal
   */
  protected wrapListenerWithTransformation<Event extends string | symbol>(
    superMethod: (
      event: Event,
      listener: (this: WebSocket, ...args: never[]) => void
    ) => this,
    event: Event,
    listener: (this: WebSocket, ...args: unknown[]) => void
  ): this {
    // eslint-disable-next-line @typescript-eslint/no-this-alias
    const self = this;
    return superMethod(event, function (this: WebSocket, ...args: never[]) {
      try {
        const transformedArgs = self.transformIncomingArgs(event, args);
        return listener.call(this, ...transformedArgs);
      } catch (err) {
        // Surface validation errors as 'error' events rather than
        // letting them propagate as uncaught exceptions
        WebSocket.prototype.emit.call(self, 'error', err);
      }
    });
  }

  /**
   * Registers an event listener with automatic data validation and transformation.
   *
   * All incoming data is automatically:
   * - Decoded from Buffer to JavaScript objects
   * - Parsed from JSON
   * - Validated against the provided schemas
   *
   * @param event - The event name to listen for
   * @param listener - The callback function to invoke when the event occurs
   * @returns `this` for method chaining
   *
   * @example Listen for validated messages
   * ```typescript
   * ws.on('message', (data, isBinary) => {
   *   // data is automatically validated and typed according to serverMessages schema
   *   console.log('Received:', data);
   * });
   * ```
   *
   * @example Listen for connection events
   * ```typescript
   * ws.on('open', () => {
   *   console.log('Connected!');
   * });
   *
   * ws.on('close', (code, reason) => {
   *   console.log('Disconnected:', code, reason);
   * });
   *
   * ws.on('error', (error) => {
   *   console.error('WebSocket error:', error);
   * });
   * ```
   */
  on(
    event: 'close',
    listener: (
      this: WebSocket,
      code: number,
      reason: Schema<ExtractSchemaFromRecord<SV, ES['closeReason']>, SV>
    ) => void
  ): this;
  on(event: 'error', listener: (this: WebSocket, error: Error) => void): this;
  on(
    event: 'upgrade',
    listener: (this: WebSocket, request: IncomingMessage) => void
  ): this;
  on(
    event: 'message',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromRecord<SV, ES['serverMessages']>, SV>,
      isBinary: boolean
    ) => void
  ): this;
  on(event: 'open', listener: (this: WebSocket) => void): this;
  on(
    event: 'ping',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromEntry<SV, ES['ping']>, SV>
    ) => void
  ): this;
  on(
    event: 'pong',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromEntry<SV, ES['pong']>, SV>
    ) => void
  ): this;
  on(
    event: 'redirect',
    listener: (this: WebSocket, url: string, request: ClientRequest) => void
  ): this;
  on(
    event: 'unexpected-response',
    listener: (
      this: WebSocket,
      request: ClientRequest,
      response: IncomingMessage
    ) => void
  ): this;
  on(
    event: string | symbol,
    listener: (this: WebSocket, ...args: never[]) => void
  ): this;
  on(
    event: string | symbol,
    listener: (this: WebSocket, ...args: never[]) => void
  ): this {
    return this.wrapListenerWithTransformation(
      super.on.bind(this) as (
        event: string | symbol,
        listener: (this: WebSocket, ...args: never[]) => void
      ) => this,
      event,
      listener as (this: WebSocket, ...args: unknown[]) => void
    );
  }

  /**
   * Registers a one-time event listener with automatic data validation and transformation.
   *
   * The listener will be invoked at most once after being registered, and then removed.
   * All incoming data is automatically decoded, parsed, and validated.
   *
   * @param event - The event name to listen for
   * @param listener - The callback function to invoke when the event occurs
   * @returns `this` for method chaining
   *
   * @example
   * ```typescript
   * // Listen for the first message only
   * ws.once('message', (data, isBinary) => {
   *   console.log('First message:', data);
   * });
   *
   * // Wait for connection
   * ws.once('open', () => {
   *   console.log('Connected! This will only fire once.');
   * });
   * ```
   */
  once(
    event: 'close',
    listener: (
      this: WebSocket,
      code: number,
      reason: Schema<ExtractSchemaFromRecord<SV, ES['closeReason']>, SV>
    ) => void
  ): this;
  once(event: 'error', listener: (this: WebSocket, error: Error) => void): this;
  once(
    event: 'upgrade',
    listener: (this: WebSocket, request: IncomingMessage) => void
  ): this;
  once(
    event: 'message',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromRecord<SV, ES['serverMessages']>, SV>,
      isBinary: boolean
    ) => void
  ): this;
  once(event: 'open', listener: (this: WebSocket) => void): this;
  once(
    event: 'ping',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromEntry<SV, ES['ping']>, SV>
    ) => void
  ): this;
  once(
    event: 'pong',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromEntry<SV, ES['pong']>, SV>
    ) => void
  ): this;
  once(
    event: 'redirect',
    listener: (this: WebSocket, url: string, request: ClientRequest) => void
  ): this;
  once(
    event: 'unexpected-response',
    listener: (
      this: WebSocket,
      request: ClientRequest,
      response: IncomingMessage
    ) => void
  ): this;
  once(
    event: string | symbol,
    listener: (this: WebSocket, ...args: never[]) => void
  ): this;
  once(
    event: string | symbol,
    listener: (this: WebSocket, ...args: never[]) => void
  ): this {
    return this.wrapListenerWithTransformation(
      super.once.bind(this) as (
        event: string | symbol,
        listener: (this: WebSocket, ...args: never[]) => void
      ) => this,
      event,
      listener as (this: WebSocket, ...args: unknown[]) => void
    );
  }

  /**
   * Removes a previously registered event listener.
   *
   * To successfully remove a listener, you must pass the exact same function
   * reference that was used when registering it.
   *
   * @param event - The event name
   * @param listener - The exact function reference to remove
   * @returns `this` for method chaining
   *
   * @example
   * ```typescript
   * const messageHandler = (data, isBinary) => {
   *   console.log('Message:', data);
   * };
   *
   * // Register
   * ws.on('message', messageHandler);
   *
   * // Later, remove
   * ws.off('message', messageHandler);
   * ```
   */
  off(
    event: 'close',
    listener: (
      this: WebSocket,
      code: number,
      reason: Schema<ExtractSchemaFromRecord<SV, ES['closeReason']>, SV>
    ) => void
  ): this;
  off(event: 'error', listener: (this: WebSocket, error: Error) => void): this;
  off(
    event: 'upgrade',
    listener: (this: WebSocket, request: IncomingMessage) => void
  ): this;
  off(
    event: 'message',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromRecord<SV, ES['serverMessages']>, SV>,
      isBinary: boolean
    ) => void
  ): this;
  off(event: 'open', listener: (this: WebSocket) => void): this;
  off(
    event: 'ping',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromEntry<SV, ES['ping']>, SV>
    ) => void
  ): this;
  off(
    event: 'pong',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromEntry<SV, ES['pong']>, SV>
    ) => void
  ): this;
  off(
    event: 'redirect',
    listener: (this: WebSocket, url: string, request: ClientRequest) => void
  ): this;
  off(
    event: 'unexpected-response',
    listener: (
      this: WebSocket,
      request: ClientRequest,
      response: IncomingMessage
    ) => void
  ): this;
  off(
    event: string | symbol,
    listener: (this: WebSocket, ...args: never[]) => void
  ): this;
  off(
    event: string | symbol,
    listener: (this: WebSocket, ...args: never[]) => void
  ): this {
    return this.wrapListenerWithTransformation(
      super.off.bind(this) as (
        event: string | symbol,
        listener: (this: WebSocket, ...args: never[]) => void
      ) => this,
      event,
      listener as (this: WebSocket, ...args: unknown[]) => void
    );
  }

  /**
   * Registers an event listener (alias for `on()`).
   *
   * This method is functionally identical to `on()` and is provided for
   * compatibility with the EventEmitter API.
   *
   * @param event - The event name to listen for
   * @param listener - The callback function to invoke when the event occurs
   * @returns `this` for method chaining
   *
   * @see {@link on} for detailed documentation and examples
   */
  addListener(
    event: 'close',
    listener: (
      this: WebSocket,
      code: number,
      reason: Schema<ExtractSchemaFromRecord<SV, ES['closeReason']>, SV>
    ) => void
  ): this;
  addListener(
    event: 'error',
    listener: (this: WebSocket, error: Error) => void
  ): this;
  addListener(
    event: 'upgrade',
    listener: (this: WebSocket, request: IncomingMessage) => void
  ): this;
  addListener(
    event: 'message',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromRecord<SV, ES['serverMessages']>, SV>,
      isBinary: boolean
    ) => void
  ): this;
  addListener(event: 'open', listener: (this: WebSocket) => void): this;
  addListener(
    event: 'ping',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromEntry<SV, ES['ping']>, SV>
    ) => void
  ): this;
  addListener(
    event: 'pong',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromEntry<SV, ES['pong']>, SV>
    ) => void
  ): this;
  addListener(
    event: 'redirect',
    listener: (this: WebSocket, url: string, request: ClientRequest) => void
  ): this;
  addListener(
    event: 'unexpected-response',
    listener: (
      this: WebSocket,
      request: ClientRequest,
      response: IncomingMessage
    ) => void
  ): this;
  addListener(
    event: string | symbol,
    listener: (this: WebSocket, ...args: never[]) => void
  ): this;
  addListener(
    event: string | symbol,
    listener: (this: WebSocket, ...args: never[]) => void
  ): this {
    return this.wrapListenerWithTransformation(
      super.addListener.bind(this) as (
        event: string | symbol,
        listener: (this: WebSocket, ...args: never[]) => void
      ) => this,
      event,
      listener as (this: WebSocket, ...args: unknown[]) => void
    );
  }

  /**
   * Removes a previously registered event listener (alias for `off()`).
   *
   * This method is functionally identical to `off()` and is provided for
   * compatibility with the EventEmitter API.
   *
   * @param event - The event name
   * @param listener - The exact function reference to remove
   * @returns `this` for method chaining
   *
   * @see {@link off} for detailed documentation and examples
   */
  removeListener(
    event: 'close',
    listener: (
      this: WebSocket,
      code: number,
      reason: Schema<ExtractSchemaFromRecord<SV, ES['closeReason']>, SV>
    ) => void
  ): this;
  removeListener(
    event: 'error',
    listener: (this: WebSocket, error: Error) => void
  ): this;
  removeListener(
    event: 'upgrade',
    listener: (this: WebSocket, request: IncomingMessage) => void
  ): this;
  removeListener(
    event: 'message',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromRecord<SV, ES['serverMessages']>, SV>,
      isBinary: boolean
    ) => void
  ): this;
  removeListener(event: 'open', listener: (this: WebSocket) => void): this;
  removeListener(
    event: 'ping',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromEntry<SV, ES['ping']>, SV>
    ) => void
  ): this;
  removeListener(
    event: 'pong',
    listener: (
      this: WebSocket,
      data: Schema<ExtractSchemaFromEntry<SV, ES['pong']>, SV>
    ) => void
  ): this;
  removeListener(
    event: 'redirect',
    listener: (this: WebSocket, url: string, request: ClientRequest) => void
  ): this;
  removeListener(
    event: 'unexpected-response',
    listener: (
      this: WebSocket,
      request: ClientRequest,
      response: IncomingMessage
    ) => void
  ): this;
  removeListener(
    event: string | symbol,
    listener: (this: WebSocket, ...args: never[]) => void
  ): this;
  removeListener(
    event: string | symbol,
    listener: (this: WebSocket, ...args: never[]) => void
  ): this {
    return this.wrapListenerWithTransformation(
      super.removeListener.bind(this) as (
        event: string | symbol,
        listener: (this: WebSocket, ...args: never[]) => void
      ) => this,
      event,
      listener as (this: WebSocket, ...args: unknown[]) => void
    );
  }

  /**
   * Emits an event with automatic data validation and encoding.
   *
   * All outgoing data is automatically:
   * - Validated against the provided schemas
   * - Encoded from JavaScript objects to Buffer format
   * - Transmitted over the WebSocket connection
   *
   * @param event - The event name to emit
   * @param args - The event arguments (type-checked based on event name)
   * @returns `true` if the event had listeners, `false` otherwise
   *
   * @example Emit a message
   * ```typescript
   * // Data is validated against clientMessages schema and auto-encoded
   * ws.emit('message', { type: 'chat', text: 'Hello!' }, true);
   * ```
   *
   * @example Emit other events
   * ```typescript
   * ws.emit('ping', { timestamp: Date.now() });
   * ws.emit('close', 1000, { reason: 'Normal closure' });
   * ws.emit('error', { code: 500, message: 'Server error' });
   * ```
   */
  /**
   * Emits an event on the WebSocket.
   *
   * This method passes through to the underlying EventEmitter without
   * transforming data. The ws library uses emit() internally to dispatch
   * incoming events (message, close, ping, pong), so encoding here would
   * incorrectly process incoming data as outgoing data.
   *
   * For sending data, use the dedicated methods instead:
   * - {@link send} for messages
   * - {@link close} for closing with a reason
   * - {@link ping} for ping frames
   * - {@link pong} for pong frames
   */
  emit<K extends keyof OutgoingEventMap<SV, ES>>(
    event: K,
    ...args: OutgoingEventMap<SV, ES>[K]
  ): boolean {
    return super.emit(event, ...(args as unknown[]));
  }

  /**
   * Sends data over the WebSocket with automatic validation and encoding.
   *
   * The data is:
   * 1. Validated against the `clientMessages` schema
   * 2. Automatically encoded to Buffer format (JSON.stringify)
   * 3. Transmitted to the server
   *
   * @param data - The message data to send (must match clientMessages schema)
   * @param options - Optional send options (mask, binary, compress, fin)
   * @param cb - Optional callback invoked when send completes or errors
   * @throws {Error} If validation fails or data cannot be encoded
   *
   * @example Send a message
   * ```typescript
   * // Data is type-checked and validated at runtime
   * ws.send({ type: 'chat', message: 'Hello, world!', userId: '123' });
   * ```
   *
   * @example Send with options and callback
   * ```typescript
   * ws.send(
   *   { type: 'ping', timestamp: Date.now() },
   *   { compress: true, binary: true },
   *   (error) => {
   *     if (error) console.error('Send failed:', error);
   *     else console.log('Send succeeded');
   *   }
   * );
   * ```
   *
   * @remarks
   * This method intentionally restricts the parameter type from `BufferLike` to
   * schema-validated types to provide compile-time type safety. This is a deliberate
   * design choice to catch type errors at compile time rather than runtime.
   */
  // @ts-expect-error - Intentionally restricting types for compile-time schema validation
  override send(
    data: Schema<ExtractSchemaFromRecord<SV, ES['clientMessages']>, SV>,
    cb?: (err?: Error) => void
  ): void;
  // @ts-expect-error - Intentionally restricting types for compile-time schema validation
  override send(
    data: Schema<ExtractSchemaFromRecord<SV, ES['clientMessages']>, SV>,
    options: {
      mask?: boolean;
      binary?: boolean;
      compress?: boolean;
      fin?: boolean;
    },
    cb?: (err?: Error) => void
  ): void;
  // @ts-expect-error - Implementation accepts unknown for internal validation
  override send(
    data: Schema<ExtractSchemaFromRecord<SV, ES['clientMessages']>, SV>,
    optionsOrCb?: unknown,
    cb?: (err?: Error) => void
  ): void {
    const options =
      typeof optionsOrCb === 'function'
        ? undefined
        : (optionsOrCb as
            | {
                mask?: boolean;
                binary?: boolean;
                compress?: boolean;
                fin?: boolean;
              }
            | undefined);
    const callback =
      typeof optionsOrCb === 'function'
        ? (optionsOrCb as (err?: Error) => void)
        : cb;

    const encoded = this.validateAndEncode(
      data,
      this.schemas.clientMessagesSchema,
      false,
      'web socket message'
    );

    if (!encoded) {
      throw new Error('Invalid data');
    }
    return super.send(encoded, options || {}, callback);
  }

  /**
   * Closes the WebSocket connection with optional validated close reason.
   *
   * @param code - Optional close code (default: 1000 for normal closure)
   * @param reason - Optional close reason (validated against closeReason schema)
   *
   * @example Close with default code
   * ```typescript
   * ws.close();
   * ```
   *
   * @example Close with code and reason
   * ```typescript
   * ws.close(1000, { reason: 'User logged out' });
   * ```
   *
   * @see {@link https://developer.mozilla.org/en-US/docs/Web/API/CloseEvent/code|WebSocket Close Codes}
   */
  // @ts-expect-error - Intentionally restricting types for compile-time schema validation
  override close(
    code?: number,
    reason?: Schema<ExtractSchemaFromRecord<SV, ES['closeReason']>, SV>
  ): void {
    if (reason) {
      const encoded = this.validateAndEncode(
        reason,
        this.schemas.closeReasonSchema,
        false,
        'web socket close'
      );
      if (encoded) return super.close(code, encoded);
    }
    return super.close(code);
  }

  /**
   * Sends a ping frame with optional validated data payload.
   *
   * Ping frames are used to check if the connection is alive. The server
   * should respond with a pong frame.
   *
   * @param data - Optional ping data (validated against ping schema)
   * @param mask - Whether to mask the frame (default: true for clients)
   * @param cb - Optional callback invoked when ping is sent or errors
   *
   * @example Send a ping
   * ```typescript
   * ws.ping({ timestamp: Date.now() });
   * ```
   *
   * @example Send ping with callback
   * ```typescript
   * ws.ping({ ts: Date.now() }, true, (error) => {
   *   if (error) console.error('Ping failed:', error);
   * });
   * ```
   */
  override ping(
    data?: Schema<ExtractSchemaFromEntry<SV, ES['ping']>, SV>,
    mask?: boolean,
    cb?: (err: Error) => void
  ): void {
    if (data !== undefined) {
      const encoded = this.validateAndEncode(
        data,
        this.schemas.pingSchema,
        false,
        'web socket ping'
      );
      super.ping(encoded, mask, cb);
    } else {
      super.ping(undefined, mask, cb);
    }
  }

  /**
   * Sends a pong frame with optional validated data payload.
   *
   * Pong frames are typically sent in response to ping frames, but can
   * also be sent unsolicited as a unidirectional heartbeat.
   *
   * @param data - Optional pong data (validated against pong schema)
   * @param mask - Whether to mask the frame (default: true for clients)
   * @param cb - Optional callback invoked when pong is sent or errors
   *
   * @example Send a pong
   * ```typescript
   * ws.pong({ timestamp: Date.now() });
   * ```
   *
   * @example Respond to ping
   * ```typescript
   * ws.on('ping', (data) => {
   *   console.log('Received ping:', data);
   *   ws.pong(data); // Echo the ping data back
   * });
   * ```
   */
  override pong(
    data?: Schema<ExtractSchemaFromEntry<SV, ES['pong']>, SV>,
    mask?: boolean,
    cb?: (err: Error) => void
  ): void {
    if (data !== undefined) {
      const encoded = this.validateAndEncode(
        data,
        this.schemas.pongSchema,
        false,
        'web socket pong'
      );
      super.pong(encoded, mask, cb);
    } else {
      super.pong(undefined, mask, cb);
    }
  }
}
