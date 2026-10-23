// Native browser transport. Cookies follow browser origin policy; no header or TLS overrides.
export interface SocketClientOptions {
  zodRegexExecution?: ZodRegexExecution;
  protocols?: string | string[];
  handshakeTimeoutMs?: number;
  maxMessageBytes?: number;
  maxQueuedEvents?: number;
  maxQueuedBytes?: number;
  signal?: AbortSignal;
}
export interface SocketReadOptions { signal?: AbortSignal; timeoutMs?: number }
export interface SocketWriteOptions { signal?: AbortSignal }
export type SocketEvent<I, P, Q, C> =
  | { type: 'message'; value: I }
  | { type: 'close'; code: number; reason: C };
type SocketSchemaSet = Record<'incoming' | 'outgoing' | 'ping' | 'pong' | 'close', AnySchema>;
type SocketWaiter<T> = { resolve(value: T | null): void; reject(error: Error): void; cleanup(): void };

class SocketRuntime<I, O, P, Q, C> implements AsyncIterable<SocketEvent<I, P, Q, C>> {
  private readonly ws: WebSocket;
  private readonly incoming: PortableSocketCodec<I>;
  private readonly outgoing: PortableSocketCodec<O>;
  private readonly closeCodec: PortableSocketCodec<C>;
  private readonly queue: Array<{event: SocketEvent<I, P, Q, C>; bytes: number}> = [];
  private readonly maxEvents: number;
  private readonly maxBytes: number;
  private readonly maxMessage: number;
  private queuedBytes = 0;
  private waiter?: SocketWaiter<SocketEvent<I, P, Q, C>>;
  private failure?: Error;
  private ended = false;
  private readonly opened: Promise<void>;
  private rejectOpen: (error: Error) => void = () => {};
  private cleanup: () => void = () => {};

  protected constructor(url: string, schemas: SocketSchemaSet, options: SocketClientOptions,
    private readonly binary: Partial<Record<keyof SocketSchemaSet, boolean>> = {},
    jsonEncoding: Partial<Record<keyof SocketSchemaSet, boolean>> = {}) {
    const endpoint = socketTransportUrl(url);
    const timeout = socketTimeout(options.handshakeTimeoutMs, 10_000, 'handshake timeout');
    this.maxEvents = socketInteger(options.maxQueuedEvents, 128, 'queue event limit');
    this.maxBytes = socketInteger(options.maxQueuedBytes, 4 * 1024 * 1024, 'queue byte limit');
    this.maxMessage = socketInteger(options.maxMessageBytes, 1024 * 1024, 'message byte limit');
    this.incoming = new PortableSocketCodec(schemas.incoming, this.maxMessage, binary.incoming, jsonEncoding.incoming, options.zodRegexExecution);
    this.outgoing = new PortableSocketCodec(schemas.outgoing, this.maxMessage, binary.outgoing, jsonEncoding.outgoing, options.zodRegexExecution);
    this.closeCodec = new PortableSocketCodec(schemas.close, 123, false, jsonEncoding.close, options.zodRegexExecution);
    if (options.signal?.aborted) throw socketAbort();
    this.ws = new WebSocket(endpoint.href, options.protocols);
    this.ws.binaryType = 'arraybuffer';
    let resolveOpen!: () => void;
    this.opened = new Promise<void>((resolve, reject) => { resolveOpen = resolve; this.rejectOpen = reject; });
    // A synchronous disposal before connect awaits must not produce an unhandled rejection.
    void this.opened.catch(() => {});
    const timer = setTimeout(() => this.fail(new Error('WebSocket handshake timeout')), timeout);
    const abort = () => this.fail(socketAbort());
    options.signal?.addEventListener('abort', abort, {once:true});
    this.cleanup = () => { clearTimeout(timer); options.signal?.removeEventListener('abort', abort); };
    this.ws.onopen = () => { clearTimeout(timer); if (!this.ended) resolveOpen(); };
    this.ws.onerror = () => this.fail(new Error('WebSocket transport failure'));
    this.ws.onmessage = event => {
      if (this.ended) return;
      try {
        if (typeof event.data !== 'string' && !(event.data instanceof ArrayBuffer))
          throw new Error('Unsupported WebSocket frame');
        if (typeof event.data === 'string' && event.data.length > this.maxMessage)
          throw new Error('WebSocket payload byte limit');
        const bytes = typeof event.data === 'string' ? new TextEncoder().encode(event.data) : new Uint8Array(event.data);
        this.push({type:'message', value:this.incoming.decode(bytes)}, bytes.byteLength);
      } catch (error) { this.fail(socketError(error)); }
    };
    this.ws.onclose = event => {
      if (this.ended) return;
      if (!event.wasClean) { this.fail(new Error('WebSocket closed abnormally')); return; }
      try {
        const bytes = new TextEncoder().encode(event.reason);
        this.push({type:'close',code:event.code,reason:this.closeCodec.decode(bytes)},bytes.byteLength);
        this.ended = true;
        this.cleanup();
        this.rejectOpen(new Error('WebSocket closed before opening'));
      } catch (error) { this.fail(socketError(error)); }
    };
  }
  protected async waitForOpen(): Promise<void> { await this.opened; }
  private fail(error: Error): void {
    if (this.ended) return;
    this.failure = error; this.ended = true; this.cleanup(); this.rejectOpen(error);
    this.queue.length = 0; this.queuedBytes = 0;
    if (this.waiter) { const waiter=this.waiter; this.waiter=undefined; waiter.cleanup(); waiter.reject(error); }
    try { this.ws.close(1000); } catch { /* Browser owns transport termination. */ }
  }
  private push(event: SocketEvent<I,P,Q,C>, bytes: number): void {
    if (this.waiter) { const waiter=this.waiter; this.waiter=undefined; waiter.cleanup(); waiter.resolve(event); return; }
    if (this.queue.length >= this.maxEvents || this.queuedBytes + bytes > this.maxBytes)
      throw new Error('WebSocket queue limit');
    this.queue.push({event,bytes}); this.queuedBytes += bytes;
  }
  async recv(options: SocketReadOptions = {}): Promise<SocketEvent<I,P,Q,C> | null> {
    if (options.signal?.aborted) throw socketAbort();
    if (this.waiter) throw new Error('Concurrent WebSocket reads are unsupported');
    if (this.failure) throw this.failure;
    const next=this.queue.shift();
    if (next) { this.queuedBytes -= next.bytes; return next.event; }
    if (this.ended) return null;
    const timeout=options.timeoutMs === undefined ? undefined : socketTimeout(options.timeoutMs, 1, 'read timeout');
    return new Promise((resolve,reject) => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const cancel=(error:Error) => { if (this.waiter !== waiter) return; this.waiter=undefined; waiter.cleanup(); reject(error); };
      const abort=() => cancel(socketAbort());
      const waiter: SocketWaiter<SocketEvent<I,P,Q,C>> = {resolve,reject,cleanup:() => {clearTimeout(timer);options.signal?.removeEventListener('abort',abort);}};
      this.waiter=waiter;
      options.signal?.addEventListener('abort',abort,{once:true});
      if(timeout !== undefined) timer=setTimeout(() => cancel(new Error('WebSocket read timeout')),timeout);
    });
  }
  /** Resolves when accepted by the browser send buffer, not acknowledged by the peer. */
  async send(value: O, options: SocketWriteOptions = {}): Promise<void> {
    if (options.signal?.aborted) throw socketAbort();
    if (this.failure) throw this.failure;
    if (this.ended || this.ws.readyState !== WebSocket.OPEN) throw new Error('WebSocket is not open');
    const bytes=this.outgoing.encode(value);
    if(this.ws.bufferedAmount + bytes.byteLength > this.maxBytes) throw new Error('WebSocket write buffer limit');
    this.ws.send(this.binary.outgoing ? bytes.slice().buffer as ArrayBuffer : new TextDecoder().decode(bytes));
  }
  close(code: number, reason: C): void {
    if (code !== 1000 && !(Number.isInteger(code) && code >= 3000 && code <= 4999))
      throw new Error('Invalid browser WebSocket close code');
    const bytes=this.closeCodec.encode(reason);
    this.ws.close(code,new TextDecoder('utf-8',{fatal:true}).decode(bytes));
  }
  /** Cancel the owned connection and release pending readers. */
  terminate(): void { this.fail(socketAbort()); }
  async *[Symbol.asyncIterator](): AsyncIterator<SocketEvent<I,P,Q,C>> {
    try { while(true) { const event=await this.recv(); if(event === null) return; yield event; } }
    finally { this.terminate(); }
  }
}
