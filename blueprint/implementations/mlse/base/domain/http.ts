// The subset of the Fetch API the source fetchers use. Injected rather than
// read from globalThis so tests run against recorded responses and never
// reach the network.
export type FetchLike = (
  url: string,
  init?: { headers?: Record<string, string> }
) => Promise<{
  ok: boolean;
  status: number;
  text: () => Promise<string>;
}>;

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

// API keys travel in query strings for NCBI and openFDA; never put them in
// errors or logs.
function redact(url: string): string {
  return url.replace(/([?&](api_key|apikey)=)[^&]*/gi, '$1***');
}

/**
 * Spaces requests to one source at least `minIntervalMs` apart, so a batch of
 * fetches stays inside the source's published rate limit (for example NCBI
 * E-utilities: 3 requests/second without a key, 10 with one).
 */
export class RateLimitedClient {
  private nextSlot = 0;

  constructor(
    private readonly sourceKey: string,
    private readonly fetchImpl: FetchLike,
    private readonly minIntervalMs: number,
    private readonly sleep: (ms: number) => Promise<void> = (ms) =>
      new Promise((resolve) => setTimeout(resolve, ms))
  ) {}

  async getText(url: string): Promise<string> {
    const now = Date.now();
    const wait = Math.max(0, this.nextSlot - now);
    this.nextSlot = Math.max(now, this.nextSlot) + this.minIntervalMs;
    if (wait > 0) {
      await this.sleep(wait);
    }

    const response = await this.fetchImpl(url, {
      headers: { accept: 'application/json, application/xml;q=0.9, */*;q=0.1' }
    });
    if (!response.ok) {
      throw new SourceRequestError(this.sourceKey, response.status, url);
    }
    return response.text();
  }

  async getJson<T>(url: string): Promise<T> {
    return JSON.parse(await this.getText(url)) as T;
  }
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
