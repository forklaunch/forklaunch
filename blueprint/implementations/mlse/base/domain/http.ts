// The subset of the Fetch API the source fetchers use. Injected rather than
// read from globalThis so tests run against recorded responses and never
// reach the network.
export type FetchLike = (
  url: string,
  init?: { headers?: Record<string, string>; signal?: AbortSignal }
) => Promise<{
  ok: boolean;
  status: number;
  text: () => Promise<string>;
}>;

/** Per call: a caller's signal ends the request (and its wait) early. */
export type RequestOptions = { signal?: AbortSignal };

export class SourceRequestError extends Error {
  constructor(
    readonly sourceKey: string,
    readonly status: number,
    url: string
  ) {
    super(`${sourceKey} request failed with HTTP ${status}: ${redact(url)}`);
    this.name = 'SourceRequestError';
  }
}

/** The source did not answer within the client's timeout; the request was cancelled. */
export class SourceTimeoutError extends Error {
  constructor(
    readonly sourceKey: string,
    readonly timeoutMs: number,
    url: string
  ) {
    super(`${sourceKey} request timed out after ${timeoutMs} ms: ${redact(url)}`);
    this.name = 'SourceTimeoutError';
  }
}

/**
 * The source's rate limit is already booked further ahead than the client
 * will wait, so the request is refused rather than queued.
 */
export class SourceBusyError extends Error {
  constructor(
    readonly sourceKey: string,
    readonly waitMs: number
  ) {
    super(`${sourceKey} is busy: the next request slot is ${waitMs} ms away`);
    this.name = 'SourceBusyError';
  }
}

// API keys travel in query strings for NCBI and openFDA; never put them in
// errors or logs.
function redact(url: string): string {
  return url.replace(/([?&](api_key|apikey)=)[^&]*/gi, '$1***');
}

/**
 * When the next request to a source may go. Clients of one source share a
 * schedule when the source's limit covers all of them (NCBI's covers PubMed
 * and PubMed Central together, and every process using the same key).
 */
export interface RequestSchedule {
  /**
   * Books the next slot `intervalMs` after the last one and returns how long
   * to wait for it, or undefined, booking nothing, when that wait would be
   * longer than `maxWaitMs`.
   */
  reserve(intervalMs: number, maxWaitMs: number): Promise<number | undefined>;
}

/** A schedule shared within one process. */
export class InProcessSchedule implements RequestSchedule {
  private nextSlot = 0;

  constructor(private readonly now: () => number = Date.now) {}

  async reserve(
    intervalMs: number,
    maxWaitMs: number
  ): Promise<number | undefined> {
    const now = this.now();
    const wait = Math.max(0, this.nextSlot - now);
    // bounded: a burst of callers is refused instead of booking slots
    // minutes ahead
    if (wait > maxWaitMs) return undefined;
    this.nextSlot = Math.max(now, this.nextSlot) + intervalMs;
    return wait;
  }
}

export type RateLimitedClientOptions = {
  // how long one request may take before it is cancelled
  timeoutMs?: number;
  // how long a request may wait for its slot before it is refused
  maxWaitMs?: number;
};

const DEFAULT_TIMEOUT_MS = 15_000;
const DEFAULT_MAX_WAIT_MS = 10_000;

/**
 * Spaces requests to one source at least `minIntervalMs` apart, so a batch of
 * fetches stays inside the source's published rate limit (for example NCBI
 * E-utilities: 3 requests/second without a key, 10 with one). Every request
 * is cancelled after `timeoutMs`, and refused when its slot is more than
 * `maxWaitMs` away.
 */
export class RateLimitedClient {
  private readonly timeoutMs: number;
  private readonly maxWaitMs: number;

  constructor(
    private readonly sourceKey: string,
    private readonly fetchImpl: FetchLike,
    private readonly minIntervalMs: number,
    private readonly sleep: (ms: number) => Promise<void> = (ms) =>
      new Promise((resolve) => setTimeout(resolve, ms)),
    private readonly schedule: RequestSchedule = new InProcessSchedule(),
    options: RateLimitedClientOptions = {}
  ) {
    this.timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    this.maxWaitMs = options.maxWaitMs ?? DEFAULT_MAX_WAIT_MS;
  }

  async getText(url: string, options: RequestOptions = {}): Promise<string> {
    const { signal } = options;
    signal?.throwIfAborted();

    const wait = await this.schedule.reserve(
      this.minIntervalMs,
      this.maxWaitMs
    );
    if (wait === undefined) {
      throw new SourceBusyError(this.sourceKey, this.maxWaitMs);
    }
    if (wait > 0) {
      await abortable(this.sleep(wait), signal);
    }
    // the caller may have given up while the slot was being booked
    signal?.throwIfAborted();

    const timeout = AbortSignal.timeout(this.timeoutMs);
    const requestSignal = signal ? AbortSignal.any([signal, timeout]) : timeout;
    try {
      const response = await this.fetchImpl(url, {
        headers: {
          accept: 'application/json, application/xml;q=0.9, */*;q=0.1'
        },
        signal: requestSignal
      });
      if (!response.ok) {
        throw new SourceRequestError(this.sourceKey, response.status, url);
      }
      // the body is read under the same signal, so a stalled body is
      // cancelled too
      return await abortable(response.text(), requestSignal);
    } catch (error) {
      if (timeout.aborted && !signal?.aborted) {
        throw new SourceTimeoutError(this.sourceKey, this.timeoutMs, url);
      }
      throw error;
    }
  }

  async getJson<T>(url: string, options: RequestOptions = {}): Promise<T> {
    return JSON.parse(await this.getText(url, options)) as T;
  }
}

// Settles with the promise, or rejects as soon as the signal aborts.
function abortable<T>(promise: Promise<T>, signal?: AbortSignal): Promise<T> {
  if (!signal) return promise;
  if (signal.aborted) return Promise.reject(signal.reason);
  return new Promise<T>((resolve, reject) => {
    const onAbort = () => reject(signal.reason);
    signal.addEventListener('abort', onAbort, { once: true });
    promise.then(
      (value) => {
        signal.removeEventListener('abort', onAbort);
        resolve(value);
      },
      (error) => {
        signal.removeEventListener('abort', onAbort);
        reject(error);
      }
    );
  });
}

// openFDA dates are YYYYMMDD; everything downstream uses ISO YYYY-MM-DD.
export function compactDateToIso(value: string | undefined): string | undefined {
  if (!value || !/^\d{8}$/.test(value)) {
    return undefined;
  }
  return `${value.slice(0, 4)}-${value.slice(4, 6)}-${value.slice(6, 8)}`;
}

export function collapseWhitespace(text: string): string {
  return text.replace(/\s+/g, ' ').trim();
}
