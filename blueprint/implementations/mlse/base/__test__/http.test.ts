import {
  FetchLike,
  InProcessSchedule,
  RateLimitedClient,
  RequestSchedule,
  SourceBusyError,
  SourceRequestError,
  SourceTimeoutError
} from '../domain/http';

// A fetch that never answers on its own and fails when its signal aborts,
// as the real fetch does.
const hangingFetch =
  (seen: { signal?: AbortSignal }): FetchLike =>
  (_url, init) => {
    seen.signal = init?.signal;
    return new Promise((_, reject) => {
      init?.signal?.addEventListener('abort', () => reject(init.signal?.reason), {
        once: true
      });
    });
  };

const okFetch: FetchLike = async () => ({
  ok: true,
  status: 200,
  text: async () => 'body'
});

describe('RateLimitedClient', () => {
  it('cancels a request that takes longer than its timeout', async () => {
    const seen: { signal?: AbortSignal } = {};
    const client = new RateLimitedClient('pubmed', hangingFetch(seen), 0, undefined, undefined, {
      timeoutMs: 30
    });

    await expect(client.getText('https://x.test/?api_key=secret')).rejects.toThrow(SourceTimeoutError);
    expect(seen.signal?.aborted).toBe(true);
  });

  it('never puts the API key in the timeout error', async () => {
    const client = new RateLimitedClient('pubmed', hangingFetch({}), 0, undefined, undefined, {
      timeoutMs: 10
    });
    const error = await client.getText('https://x.test/?api_key=secret').catch((e: Error) => e);
    expect(String(error)).not.toContain('secret');
  });

  it('asks once more, a moment later, when the source says too many requests', async () => {
    const statuses = [429, 200];
    const waits: number[] = [];
    const busyThenOk: FetchLike = async () => {
      const status = statuses.shift() ?? 429;
      return { ok: status === 200, status, text: async () => 'answer' };
    };
    const client = new RateLimitedClient('pubmed', busyThenOk, 0, async (ms) => {
      waits.push(ms);
    });

    await expect(client.getText('https://x.test/')).resolves.toBe('answer');
    expect(waits).toContain(1000);
  });

  it('gives up after a second "too many requests"', async () => {
    const alwaysBusy: FetchLike = async () => ({ ok: false, status: 429, text: async () => '' });
    const client = new RateLimitedClient('pubmed', alwaysBusy, 0, async () => undefined);

    const error = await client.getText('https://x.test/').catch((e: SourceRequestError) => e);
    expect(error).toBeInstanceOf(SourceRequestError);
    expect((error as SourceRequestError).status).toBe(429);
  });

  it('cancels a body that stalls after the headers arrive', async () => {
    const stalledBody: FetchLike = async () => ({
      ok: true,
      status: 200,
      text: () => new Promise<string>(() => undefined)
    });
    const client = new RateLimitedClient('pubmed', stalledBody, 0, undefined, undefined, {
      timeoutMs: 30
    });
    await expect(client.getText('https://x.test/')).rejects.toThrow(SourceTimeoutError);
  });

  it("cancels a request in flight when the caller's signal aborts, and says so rather than timing out", async () => {
    const seen: { signal?: AbortSignal } = {};
    const client = new RateLimitedClient('pubmed', hangingFetch(seen), 0);
    const controller = new AbortController();
    const request = client.getText('https://x.test/', { signal: controller.signal });
    while (!seen.signal) await new Promise((resolve) => setTimeout(resolve, 1));
    controller.abort(new Error('caller gave up'));

    await expect(request).rejects.toThrow('caller gave up');
    expect(seen.signal?.aborted).toBe(true);
  });

  it('does not send the request when the caller gave up before its slot came', async () => {
    let fetched = false;
    const client = new RateLimitedClient(
      'pubmed',
      async () => {
        fetched = true;
        return { ok: true, status: 200, text: async () => '' };
      },
      0
    );
    const controller = new AbortController();
    const request = client.getText('https://x.test/', { signal: controller.signal });
    controller.abort(new Error('caller gave up'));

    await expect(request).rejects.toThrow('caller gave up');
    expect(fetched).toBe(false);
  });

  it('stops waiting for its slot when the caller aborts', async () => {
    const neverEnds = () => new Promise<void>(() => undefined);
    const booked: RequestSchedule = { reserve: async () => 5_000 };
    let fetched = false;
    const client = new RateLimitedClient(
      'pubmed',
      async () => {
        fetched = true;
        return { ok: true, status: 200, text: async () => '' };
      },
      100,
      neverEnds,
      booked
    );
    const controller = new AbortController();
    const request = client.getText('https://x.test/', { signal: controller.signal });
    controller.abort(new Error('caller gave up'));

    await expect(request).rejects.toThrow('caller gave up');
    expect(fetched).toBe(false);
  });

  it('refuses, without fetching, when its slot is further away than it will wait', async () => {
    let fetched = false;
    const full: RequestSchedule = { reserve: async () => undefined };
    const client = new RateLimitedClient(
      'pubmed',
      async () => {
        fetched = true;
        return { ok: true, status: 200, text: async () => '' };
      },
      100,
      undefined,
      full
    );
    await expect(client.getText('https://x.test/')).rejects.toThrow(SourceBusyError);
    expect(fetched).toBe(false);
  });

  it('passes a signal to every request', async () => {
    let signal: AbortSignal | undefined;
    const client = new RateLimitedClient(
      'pubmed',
      async (url, init) => {
        signal = init?.signal;
        return okFetch(url, init);
      },
      0
    );
    await expect(client.getText('https://x.test/')).resolves.toBe('body');
    expect(signal).toBeInstanceOf(AbortSignal);
  });
});

describe('InProcessSchedule', () => {
  it('spaces requests, and refuses rather than booking slots far ahead', async () => {
    let now = 0;
    const schedule = new InProcessSchedule(() => now);

    expect(await schedule.reserve(100, 250)).toBe(0);
    expect(await schedule.reserve(100, 250)).toBe(100);
    expect(await schedule.reserve(100, 250)).toBe(200);
    // the next slot is 300 ms away: refused, and nothing is booked
    expect(await schedule.reserve(100, 250)).toBeUndefined();
    expect(await schedule.reserve(100, 250)).toBeUndefined();

    now = 100;
    expect(await schedule.reserve(100, 250)).toBe(200);
  });
});
