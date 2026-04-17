/* eslint-disable @typescript-eslint/no-unused-vars */

import type {
  AddressInfo,
  Socket,
  SocketConnectOpts,
  SocketReadyState
} from 'net';
import { Duplex } from 'stream';

export class BunSocketShim extends Duplex implements Socket {
  public remoteAddress: string;
  public remotePort: number;
  public remoteFamily: string;
  public localAddress: string;
  public localPort: number;
  public localFamily: string;
  public bytesRead: number = 0;
  public bytesWritten: number = 0;
  public connecting: boolean = false;
  public pending: boolean = false;
  public timeout?: number;
  public readyState: SocketReadyState = 'closed';
  public bufferSize: number = 0;
  public autoSelectFamilyAttemptedAddresses: string[] = [];

  private _destroyed: boolean = false;
  private _socketTimeoutId?: Timer;
  private _abortController?: AbortController;
  private _request?: Request;
  private _responseWriter?: WritableStreamDefaultWriter<Uint8Array>;

  constructor(
    options: {
      remoteAddress?: string;
      remotePort?: number;
      localAddress?: string;
      localPort?: number;
      abortController?: AbortController;
      request?: Request;
      responseWriter?: WritableStreamDefaultWriter<Uint8Array>;
    } = {}
  ) {
    super({
      allowHalfOpen: false,
      objectMode: false
    });

    this.remoteAddress = options.remoteAddress ?? '127.0.0.1';
    this.remotePort = options.remotePort ?? 0;
    this.remoteFamily = 'IPv4';
    this.localAddress = options.localAddress ?? 'localhost';
    this.localPort = options.localPort ?? 0;
    this.localFamily = 'IPv4';
    this._abortController = options.abortController || new AbortController();
    this._request = options.request;
    this._responseWriter = options.responseWriter;
    this.readyState = 'open';

    this._setupAbortHandling();
    this._setupRequestReading();
  }

  private _setupAbortHandling(): void {
    if (this._abortController?.signal) {
      this._abortController.signal.addEventListener('abort', () => {
        if (!this._destroyed) {
          this.destroy(new Error('Socket aborted'));
        }
      });
    }
  }

  private _setupRequestReading(): void {
    if (this._request?.body) {
      const reader = this._request.body.getReader();

      const pump = async (): Promise<void> => {
        try {
          while (!this._destroyed) {
            const { done, value } = await reader.read();
            if (done) break;

            this.bytesRead += value.length;
            this.push(Buffer.from(value));
          }
          this.push(null);
        } catch (error) {
          if (!this._destroyed) {
            this.destroy(
              error instanceof Error ? error : new Error(String(error))
            );
          }
        }
      };

      pump();
    }
  }

  _read(_size?: number): void {
    // Reading is handled by _setupRequestReading for HTTP requests
    // For other cases, data is pushed externally
  }

  _write(
    chunk: Buffer | Uint8Array | string,
    encoding: BufferEncoding,
    callback: (error?: Error | null) => void
  ): void {
    try {
      if (this._destroyed || !this.writable) {
        callback(new Error('Socket is not writable'));
        return;
      }

      let buffer: Buffer;
      if (Buffer.isBuffer(chunk)) {
        buffer = chunk;
      } else if (chunk instanceof Uint8Array) {
        buffer = Buffer.from(chunk);
      } else {
        buffer = Buffer.from(chunk, encoding);
      }

      this.bytesWritten += buffer.length;

      if (this._responseWriter) {
        this._responseWriter
          .write(new Uint8Array(buffer))
          .then(() => callback())
          .catch(callback);
      } else {
        queueMicrotask(() => callback());
      }
    } catch (error) {
      callback(error instanceof Error ? error : new Error(String(error)));
    }
  }

  _final(callback: (error?: Error | null) => void): void {
    if (this._responseWriter) {
      this._responseWriter
        .close()
        .then(() => callback())
        .catch(callback);
    } else {
      callback();
    }
  }

  _destroy(
    error: Error | null,
    callback: (error?: Error | null) => void
  ): void {
    this._destroyed = true;
    this.readyState = 'closed';
    this.connecting = false;
    this.pending = false;

    if (this._socketTimeoutId) {
      clearTimeout(this._socketTimeoutId);
      this._socketTimeoutId = undefined;
    }

    if (this._responseWriter && !this._responseWriter.closed) {
      this._responseWriter.abort().catch(() => {});
    }

    if (this._abortController && !this._abortController.signal.aborted) {
      this._abortController.abort();
    }

    callback(error);
  }

  destroyed: boolean = this._destroyed;

  destroy(error?: Error): this {
    if (this._destroyed) return this;
    super.destroy(error);
    return this;
  }

  destroySoon(): void {
    this.end();
  }

  connect(options: SocketConnectOpts, connectionListener?: () => void): this;
  connect(port: number, host: string, connectionListener?: () => void): this;
  connect(port: number, connectionListener?: () => void): this;
  connect(path: string, connectionListener?: () => void): this;
  connect(
    optionsPortOrPath: SocketConnectOpts | number | string,
    hostOrConnectionListener?: string | (() => void),
    connectionListener?: () => void
  ): this {
    let port: number | undefined;
    let host: string | undefined;
    let listener: (() => void) | undefined;

    if (typeof optionsPortOrPath === 'number') {
      port = optionsPortOrPath;
      if (typeof hostOrConnectionListener === 'string') {
        host = hostOrConnectionListener;
        listener = connectionListener;
      } else if (typeof hostOrConnectionListener === 'function') {
        host = 'localhost';
        listener = hostOrConnectionListener;
      } else {
        host = 'localhost';
      }

      // TCP connection with number port
      this.remoteAddress = host;
      this.remotePort = port;
      this.localAddress = 'localhost';
      this.localPort = 0;
      this.remoteFamily = 'IPv4';
      this.localFamily = 'IPv4';
    } else if (typeof optionsPortOrPath === 'string') {
      // IPC connection with string path
      this.remoteAddress = optionsPortOrPath;
      this.remotePort = 0;
      this.localAddress = optionsPortOrPath;
      this.localPort = 0;
      this.remoteFamily = 'Unix';
      this.localFamily = 'Unix';

      listener =
        typeof hostOrConnectionListener === 'function'
          ? hostOrConnectionListener
          : connectionListener;
    } else {
      const options = optionsPortOrPath;

      // Discriminate between IPC and TCP connections
      if ('path' in options) {
        // IPC connection - Unix domain socket or named pipe
        this.remoteAddress = options.path;
        this.remotePort = 0;
        this.localAddress = options.path;
        this.localPort = 0;
        this.remoteFamily = 'Unix';
        this.localFamily = 'Unix';
      } else {
        // TCP connection
        port = options.port;
        host = options.host || 'localhost';
        this.remoteAddress = host;
        this.remotePort = port;
        this.localAddress = options.localAddress || 'localhost';
        this.localPort = options.localPort || 0;
        this.remoteFamily =
          typeof options.family === 'number'
            ? 'IPv4'
            : options.family || 'IPv4';
        this.localFamily =
          typeof options.family === 'number'
            ? 'IPv4'
            : options.family || 'IPv4';
      }

      listener =
        typeof hostOrConnectionListener === 'function'
          ? hostOrConnectionListener
          : connectionListener;
    }

    this.connecting = true;
    this.pending = true;
    this.readyState = 'opening';

    if (listener) {
      this.once('connect', listener);
    }

    queueMicrotask(() => {
      if (this._destroyed) return;

      this.connecting = false;
      this.pending = false;
      this.readyState = 'open';
      if (port !== undefined) {
        this.remotePort = port;
      }
      if (host !== undefined) {
        this.remoteAddress = host;
      }
      this.emit('connect');
    });

    return this;
  }

  setTimeout(timeout: number, callback?: () => void): this {
    if (this._socketTimeoutId) {
      clearTimeout(this._socketTimeoutId);
    }

    this.timeout = timeout;

    if (timeout > 0) {
      this._socketTimeoutId = setTimeout(() => {
        if (this._destroyed) return;
        this.emit('timeout');
        if (callback) {
          try {
            callback();
          } catch (error) {
            this.emit('error', error);
          }
        }
      }, timeout);
    }

    return this;
  }

  setNoDelay(_noDelay: boolean = true): this {
    return this;
  }

  setKeepAlive(enable: boolean = false, initialDelay: number = 0): this {
    return this;
  }

  getTypeOfService(): number {
    return 0;
  }

  setTypeOfService(_typeOfService: number): this {
    return this;
  }

  address(): AddressInfo | object {
    if (!this.localAddress || !this.localPort) {
      return {};
    }
    return {
      address: this.localAddress,
      family: this.localFamily || 'IPv4',
      port: this.localPort
    };
  }

  unref(): this {
    if (
      this._socketTimeoutId &&
      typeof this._socketTimeoutId === 'object' &&
      'unref' in this._socketTimeoutId
    ) {
      (this._socketTimeoutId as NodeJS.Timeout).unref();
    }
    return this;
  }

  ref(): this {
    if (
      this._socketTimeoutId &&
      typeof this._socketTimeoutId === 'object' &&
      'ref' in this._socketTimeoutId
    ) {
      (this._socketTimeoutId as NodeJS.Timeout).ref();
    }
    return this;
  }

  setEncoding(encoding?: BufferEncoding): this {
    super.setEncoding(encoding || 'utf-8');
    return this;
  }

  resetAndDestroy(): this {
    this.destroy();
    return this;
  }

  end(): this;
  end(chunk: Buffer | Uint8Array | string): this;
  end(chunk: Buffer | Uint8Array | string, encoding: BufferEncoding): this;
  end(cb: () => void): this;
  end(chunk: Buffer | Uint8Array | string, cb: () => void): this;
  end(
    chunk: Buffer | Uint8Array | string,
    encoding: BufferEncoding,
    cb: () => void
  ): this;
  end(
    chunkOrCb?: Buffer | Uint8Array | string | (() => void),
    encodingOrCb?: BufferEncoding | (() => void),
    cb?: () => void
  ): this {
    let chunk: Buffer | Uint8Array | string | undefined;
    let encoding: BufferEncoding | undefined;
    let callback: (() => void) | undefined;

    if (typeof chunkOrCb === 'function') {
      callback = chunkOrCb;
    } else {
      chunk = chunkOrCb;
      if (typeof encodingOrCb === 'function') {
        callback = encodingOrCb;
      } else {
        encoding = encodingOrCb;
        callback = cb;
      }
    }

    if (callback) {
      this.once('finish', callback);
    }

    super.end(chunk, encoding || 'utf-8');
    return this;
  }

  write(chunk: Buffer | Uint8Array | string): boolean;
  write(
    chunk: Buffer | Uint8Array | string,
    cb: (error: Error | null | undefined) => void
  ): boolean;
  write(chunk: Buffer | Uint8Array | string, encoding: BufferEncoding): boolean;
  write(
    chunk: Buffer | Uint8Array | string,
    encoding: BufferEncoding,
    cb: (error: Error | null | undefined) => void
  ): boolean;
  write(
    chunk: Buffer | Uint8Array | string,
    encodingOrCb?: BufferEncoding | ((error: Error | null | undefined) => void),
    cb?: (error: Error | null | undefined) => void
  ): boolean {
    let encoding: BufferEncoding | undefined;
    let callback: ((error: Error | null | undefined) => void) | undefined;

    if (typeof encodingOrCb === 'function') {
      callback = encodingOrCb;
    } else {
      encoding = encodingOrCb;
      callback = cb;
    }

    return super.write(chunk, encoding || 'utf-8', callback);
  }
}

export function createBunSocket(
  options: {
    remoteAddress?: string;
    remotePort?: number;
    localAddress?: string;
    localPort?: number;
    request?: Request;
    responseWriter?: WritableStreamDefaultWriter<Uint8Array>;
    abortController?: AbortController;
  } = {}
): Socket {
  return new BunSocketShim(options);
}

export function createSocketFromBunRequest(
  request: Request,
  serverInfo: {
    hostname: string;
    port: number;
  }
): Socket {
  const forwardedFor = request.headers.get('x-forwarded-for');
  const realIP = request.headers.get('x-real-ip');
  const cfConnectingIP = request.headers.get('cf-connecting-ip');

  const remoteAddress =
    forwardedFor?.split(',')[0]?.trim() ||
    realIP ||
    cfConnectingIP ||
    '127.0.0.1';

  return createBunSocket({
    remoteAddress,
    remotePort: 0,
    localAddress: serverInfo.hostname,
    localPort: serverInfo.port,
    request
  });
}

export function createSocketPair(): [Socket, Socket] {
  const socket1 = createBunSocket();
  const socket2 = createBunSocket();

  socket1.pipe(socket2);
  socket2.pipe(socket1);

  return [socket1, socket2];
}

export type { Socket } from 'net';
