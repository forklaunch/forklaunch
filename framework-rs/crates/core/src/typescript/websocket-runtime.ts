// Node WebSocket runtime emitted with raw-schema clients.
import { WebSocket, type ClientOptions, type RawData } from 'ws';
import type { IncomingHttpHeaders } from 'node:http';

export interface SocketClientOptions {
  /** Synchronous host RegExp execution has no interruptible native work budget. */
  zodRegexExecution?: ZodRegexExecution;
  headers?: ClientOptions['headers'];
  protocols?: string | string[];
  webSocketOptions?: Omit<
    ClientOptions,
    'headers' | 'maxPayload' | 'autoPong' | 'handshakeTimeout' | 'followRedirects' | 'rejectUnauthorized'
  >;
  handshakeTimeoutMs?: number;
  writeTimeoutMs?: number;
  closeTimeoutMs?: number;
  maxMessageBytes?: number;
  maxQueuedEvents?: number;
  maxQueuedBytes?: number;
  signal?: AbortSignal;
}
export interface SocketReadOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
}
export interface SocketWriteOptions {
  signal?: AbortSignal;
}
export interface SocketUpgradeResponse {
  readonly statusCode: number;
  readonly headers: IncomingHttpHeaders;
  readonly protocol: string;
}
export type SocketEvent<I, P, Q, C> =
  | { type: 'message'; value: I }
  | { type: 'ping'; value: P }
  | { type: 'pong'; value: Q }
  | { type: 'close'; code: number; reason: C };
type SocketSchemaSet = Record<
  'incoming' | 'outgoing' | 'ping' | 'pong' | 'close',
  AnySchema
>;
type SocketWaiter<T> = {
  resolve(value: T | null): void;
  reject(error: Error): void;
  cleanup(): void;
};

function socketBytes(data: RawData): Buffer {
  return Buffer.isBuffer(data)
    ? data
    : Array.isArray(data)
      ? Buffer.concat(data)
      : Buffer.from(data);
}

class SocketCodec<T> extends PortableSocketCodec<T> {
  override encode(value: T): Buffer { return Buffer.from(super.encode(value)); }
}

class SocketRuntime<I, O, P, Q, C> implements AsyncIterable<
  SocketEvent<I, P, Q, C>
> {
  private readonly ws: WebSocket;
  private readonly incoming: SocketCodec<I>;
  private readonly outgoing: SocketCodec<O>;
  private readonly pingCodec: SocketCodec<P>;
  private readonly pongCodec: SocketCodec<Q>;
  private readonly closeCodec: SocketCodec<C>;
  private readonly options: Required<
    Pick<
      SocketClientOptions,
      | 'writeTimeoutMs'
      | 'closeTimeoutMs'
      | 'maxMessageBytes'
      | 'maxQueuedEvents'
      | 'maxQueuedBytes'
    >
  >;
  private readonly queue: Array<{
    event: SocketEvent<I, P, Q, C>;
    bytes: number;
  }> = [];
  private queuedBytes = 0;
  private waiter?: SocketWaiter<SocketEvent<I, P, Q, C>>;
  private failure?: Error;
  private failureDelivered = false;
  private ended = false;
  private disposed = false;
  private writing = false;
  private closeTimer?: ReturnType<typeof setTimeout>;
  private readonly controlWrites = new Set<() => void>();
  private upgrade?: SocketUpgradeResponse;
  private readonly opened: Promise<void>;

  protected constructor(
    url: string,
    schemas: SocketSchemaSet,
    options: SocketClientOptions,
    binary: Partial<Record<keyof SocketSchemaSet, boolean>> = {},
    jsonEncoding: Partial<Record<keyof SocketSchemaSet, boolean>> = {},
  ) {
    const handshakeTimeoutMs = socketTimeout(
      options.handshakeTimeoutMs,
      10_000,
      'handshake timeout',
    );
    this.options = {
      writeTimeoutMs: socketTimeout(
        options.writeTimeoutMs,
        5_000,
        'write timeout',
      ),
      closeTimeoutMs: socketTimeout(
        options.closeTimeoutMs,
        5_000,
        'close timeout',
      ),
      maxMessageBytes: socketInteger(
        options.maxMessageBytes,
        1024 * 1024,
        'message byte limit',
      ),
      maxQueuedEvents: socketInteger(
        options.maxQueuedEvents,
        128,
        'queue event limit',
      ),
      maxQueuedBytes: socketInteger(
        options.maxQueuedBytes,
        4 * 1024 * 1024,
        'queue byte limit',
      ),
    };
    this.incoming = new SocketCodec(
      schemas.incoming,
      this.options.maxMessageBytes,
      binary.incoming,
      jsonEncoding.incoming,
      options.zodRegexExecution,
    );
    this.outgoing = new SocketCodec(
      schemas.outgoing,
      this.options.maxMessageBytes,
      binary.outgoing,
      jsonEncoding.outgoing,
      options.zodRegexExecution,
    );
    this.pingCodec = new SocketCodec(
      schemas.ping,
      Math.min(125, this.options.maxMessageBytes),
      binary.ping,
      jsonEncoding.ping,
      options.zodRegexExecution,
    );
    this.pongCodec = new SocketCodec(
      schemas.pong,
      Math.min(125, this.options.maxMessageBytes),
      binary.pong,
      jsonEncoding.pong,
      options.zodRegexExecution,
    );
    this.closeCodec = new SocketCodec(
      schemas.close,
      Math.min(123, this.options.maxMessageBytes),
      binary.close,
      jsonEncoding.close,
      options.zodRegexExecution,
    );
    if (options.signal?.aborted) throw socketAbort();
    this.ws = new WebSocket(socketTransportUrl(url), options.protocols ?? [], {
      ...options.webSocketOptions,
      // Never forward authorization headers across redirects or TLS downgrades.
      followRedirects: false,
      rejectUnauthorized: true,
      headers: options.headers,
      maxPayload: this.options.maxMessageBytes,
      autoPong: false,
      handshakeTimeout: handshakeTimeoutMs,
    });
    this.opened = new Promise((resolve, reject) => {
      const abort = () => {
        const error = socketAbort();
        this.fail(error);
        reject(error);
      };
      const cleanup = () => {
        options.signal?.removeEventListener('abort', abort);
        this.ws.off('open', open);
        this.ws.off('error', error);
        this.ws.off('close', close);
      };
      const open = () => {
        cleanup();
        resolve();
      };
      const error = (cause: Error) => {
        cleanup();
        reject(cause);
      };
      const close = () => {
        cleanup();
        reject(new Error('WebSocket closed before upgrade'));
      };
      this.ws.once('open', open);
      this.ws.once('error', error);
      this.ws.once('close', close);
      options.signal?.addEventListener('abort', abort, { once: true });
    });
    this.opened.catch(() => {});
    this.ws.on('upgrade', (response) => {
      this.upgrade = {
        statusCode: response.statusCode ?? 101,
        headers: { ...response.headers },
        protocol: '',
      };
    });
    this.ws.on('open', () => {
      if (this.upgrade)
        this.upgrade = { ...this.upgrade, protocol: this.ws.protocol };
    });
    this.ws.on('error', (error) => this.fail(error));
    this.ws.on('message', (data) =>
      this.receive(
        () => ({
          type: 'message',
          value: this.incoming.decode(socketBytes(data)),
        }),
        socketBytes(data).length,
      ),
    );
    this.ws.on('pong', (bytes) =>
      this.receive(
        () => ({ type: 'pong', value: this.pongCodec.decode(bytes) }),
        bytes.length,
      ),
    );
    this.ws.on('ping', (bytes) =>
      this.receive(() => {
        const value = this.pingCodec.decode(bytes);
        this.pongCodec.decode(bytes); // Validate the exact automatic reply, without re-encoding.
        this.automaticPong(bytes);
        return { type: 'ping', value };
      }, bytes.length),
    );
    this.ws.on('close', (code, bytes) => {
      clearTimeout(this.closeTimer);
      this.clearControlWrites();
      if (code === 1006 && !this.disposed && !this.failure)
        this.fail(new Error('WebSocket closed without a handshake'));
      if (!this.disposed && !this.failure)
        this.receive(
          () => {
            if (
              bytes.length + (code === 1005 ? 0 : 2) >
              this.options.maxMessageBytes
            )
              throw new Error('WebSocket close frame byte limit');
            return {
              type: 'close',
              code,
              reason: this.closeCodec.decode(bytes),
            };
          },
          bytes.length + (code === 1005 ? 0 : 2),
        );
      this.ended = true;
      const waiter = this.takeWaiter();
      if (waiter)
        this.failure ? waiter.reject(this.failure) : waiter.resolve(null);
    });
  }

  protected async waitForOpen(): Promise<void> {
    await this.opened;
    if (this.failure) throw this.failure;
  }
  get response(): SocketUpgradeResponse {
    if (!this.upgrade)
      throw new Error('WebSocket upgrade response is unavailable');
    return this.upgrade;
  }
  get protocol(): string {
    return this.ws.protocol;
  }
  private takeWaiter(): SocketWaiter<SocketEvent<I, P, Q, C>> | undefined {
    const waiter = this.waiter;
    this.waiter = undefined;
    waiter?.cleanup();
    return waiter;
  }
  private fail(error: unknown, policy = false): void {
    if (this.disposed || this.failure) return;
    this.failure = socketError(error);
    this.ended = true;
    clearTimeout(this.closeTimer);
    this.clearControlWrites();
    const waiter = this.takeWaiter();
    if (waiter) {
      this.failureDelivered = true;
      waiter.reject(this.failure);
    }
    if (policy && this.ws.readyState === WebSocket.OPEN) {
      this.ws.close(1008, 'invalid payload');
      this.closeTimer = setTimeout(
        () => this.ws.terminate(),
        this.options.closeTimeoutMs,
      );
    } else this.ws.terminate();
  }
  private receive(make: () => SocketEvent<I, P, Q, C>, bytes: number): void {
    if (this.failure || this.disposed) return;
    try {
      const event = make();
      if (this.failure || this.disposed) return;
      const waiter = this.takeWaiter();
      if (waiter) {
        waiter.resolve(event);
        return;
      }
      if (
        this.queue.length >= this.options.maxQueuedEvents ||
        this.queuedBytes + bytes > this.options.maxQueuedBytes
      )
        throw new Error('WebSocket receive queue limit');
      this.queue.push({ event, bytes });
      this.queuedBytes += bytes;
    } catch (error) {
      this.fail(error, true);
    }
  }
  private clearControlWrites(): void {
    for (const cleanup of this.controlWrites) cleanup();
  }
  private automaticPong(bytes: Buffer): void {
    if (
      this.controlWrites.size >= this.options.maxQueuedEvents ||
      this.ws.bufferedAmount + bytes.length + 14 >
        this.options.maxMessageBytes + 128
    )
      throw new Error('WebSocket automatic reply buffer limit');
    let settled = false;
    const cleanup = () => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      this.controlWrites.delete(cleanup);
    };
    const timer = setTimeout(() => {
      cleanup();
      this.fail(new Error('WebSocket pong write timed out'));
    }, this.options.writeTimeoutMs);
    this.controlWrites.add(cleanup);
    try {
      this.ws.pong(bytes, undefined, (error) => {
        if (settled) return;
        cleanup();
        if (error) this.fail(error);
      });
    } catch (error) {
      cleanup();
      throw error;
    }
  }
  async recv(
    options: SocketReadOptions = {},
  ): Promise<SocketEvent<I, P, Q, C> | null> {
    if (options.signal?.aborted) throw socketAbort();
    const timeout =
      options.timeoutMs === undefined
        ? undefined
        : socketTimeout(options.timeoutMs, 1, 'read timeout');
    if (this.waiter)
      throw new Error('Only one WebSocket receive may be pending');
    const queued = this.queue.shift();
    if (queued) {
      this.queuedBytes -= queued.bytes;
      return queued.event;
    }
    if (this.failure && !this.failureDelivered) {
      this.failureDelivered = true;
      throw this.failure;
    }
    if (this.ended) return null;
    return new Promise((resolve, reject) => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const abort = () => {
        this.takeWaiter()?.reject(socketAbort());
      };
      const cleanup = () => {
        clearTimeout(timer);
        options.signal?.removeEventListener('abort', abort);
      };
      this.waiter = { resolve, reject, cleanup };
      if (timeout !== undefined)
        timer = setTimeout(() => {
          this.takeWaiter()?.reject(new Error('WebSocket read timed out'));
        }, timeout);
      options.signal?.addEventListener('abort', abort, { once: true });
    });
  }
  private async write(
    kind: 'message' | 'ping' | 'pong',
    bytes: Buffer,
    options: SocketWriteOptions,
  ): Promise<void> {
    if (options.signal?.aborted) throw socketAbort();
    if (this.failure) throw this.failure;
    if (this.ended || this.ws.readyState !== WebSocket.OPEN)
      throw new Error('WebSocket is closed');
    if (this.writing)
      throw new Error('Only one WebSocket write may be pending');
    if (
      this.ws.bufferedAmount + bytes.length + 14 >
      this.options.maxMessageBytes + 128
    )
      throw new Error('WebSocket write buffer limit');
    this.writing = true;
    try {
      await new Promise<void>((resolve, reject) => {
        let settled = false;
        const finish = (error?: Error) => {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          options.signal?.removeEventListener('abort', abort);
          if (error) {
            this.fail(error);
            this.failureDelivered = true;
            reject(error);
          } else resolve();
        };
        const abort = () => finish(socketAbort());
        const timer = setTimeout(
          () => finish(new Error('WebSocket write timed out')),
          this.options.writeTimeoutMs,
        );
        options.signal?.addEventListener('abort', abort, { once: true });
        try {
          if (kind === 'message') this.ws.send(bytes, finish);
          else if (kind === 'ping') this.ws.ping(bytes, undefined, finish);
          else this.ws.pong(bytes, undefined, finish);
        } catch (error) {
          finish(socketError(error));
        }
      });
    } finally {
      this.writing = false;
    }
  }
  async send(value: O, options: SocketWriteOptions = {}): Promise<void> {
    await this.write('message', this.outgoing.encode(value), options);
  }
  async ping(value?: P, options: SocketWriteOptions = {}): Promise<void> {
    await this.write(
      'ping',
      value === undefined ? Buffer.alloc(0) : this.pingCodec.encode(value),
      options,
    );
  }
  async pong(value?: Q, options: SocketWriteOptions = {}): Promise<void> {
    await this.write(
      'pong',
      value === undefined ? Buffer.alloc(0) : this.pongCodec.encode(value),
      options,
    );
  }
  close(code = 1000, reason?: C): void {
    if (
      !Number.isInteger(code) ||
      !(
        (code >= 1000 && code <= 1014 && ![1004, 1005, 1006].includes(code)) ||
        (code >= 3000 && code <= 4999)
      )
    )
      throw new Error('Reserved WebSocket close code');
    if (this.failure) throw this.failure;
    if (this.ended || this.ws.readyState !== WebSocket.OPEN)
      throw new Error('WebSocket is closed');
    if (this.writing)
      throw new Error('Only one WebSocket write may be pending');
    const bytes =
      reason === undefined ? undefined : this.closeCodec.encode(reason);
    if ((bytes?.length ?? 0) + 2 > this.options.maxMessageBytes)
      throw new Error('WebSocket close frame byte limit');
    this.closeTimer = setTimeout(
      () => this.fail(new Error('WebSocket close timed out')),
      this.options.closeTimeoutMs,
    );
    try {
      this.ws.close(code, bytes);
    } catch (error) {
      clearTimeout(this.closeTimer);
      throw error;
    }
  }
  terminate(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.failureDelivered = true;
    this.ended = true;
    this.queue.length = 0;
    this.queuedBytes = 0;
    clearTimeout(this.closeTimer);
    this.clearControlWrites();
    this.takeWaiter()?.resolve(null);
    this.ws.terminate();
  }
  [Symbol.asyncIterator](): AsyncIterator<SocketEvent<I, P, Q, C>> {
    return {
      next: async () => {
        const value = await this.recv();
        return value === null
          ? { done: true, value: undefined }
          : { done: false, value };
      },
      return: async () => {
        this.terminate();
        return { done: true, value: undefined };
      },
    };
  }
}
