import { afterEach, describe, expect, expectTypeOf, it, vi } from 'vitest';
import { createServer } from 'node:http';
import { pathToFileURL } from 'node:url';
import {
  createPreviewModelGatewayClient,
  type GatewayPreviewModels,
  type GatewayPreviewChatCompletion,
  type PreviewModelGatewayClient
} from '../src/http/previewModelGatewayClient';
import {
  createModelGatewayClient,
  type GatewayModels,
  type GatewayChatCompletion,
  type ModelGatewayClient,
  ModelGatewayRequestError
} from '../src/http/modelGatewayClient';
import { createHmacToken } from '../src/http/createHmacToken';
const key = 'synthetic-preview-only',
  id = 'preview-1';
const catalog = () => ({
  models: [{ id: 'reviewed-model' }],
  preview: { expiresAt: Date.now() + 600000, maxCalls: 20, maxUnits: 100000 }
});
function client(raw: unknown = catalog(), status = 200) {
  const calls: { url: string; init: RequestInit }[] = [];
  const fetchImpl = (async (url: string, init: RequestInit) => {
    calls.push({ url, init });
    return Response.json(raw, { status });
  }) as typeof fetch;
  return {
    client: createPreviewModelGatewayClient({
      gatewayUrl: 'https://local-preview.example',
      instanceId: id,
      hmacKey: key,
      fetch: fetchImpl
    }),
    calls
  };
}
afterEach(() => vi.useRealTimers());
describe('Separate preview catalog contract', () => {
  it('retains managed default types and exposes no monthly or compliance claims in preview types', () => {
    expectTypeOf<
      ReturnType<typeof createModelGatewayClient>
    >().toEqualTypeOf<ModelGatewayClient>();
    expectTypeOf<
      Awaited<ReturnType<ModelGatewayClient['models']>>
    >().toEqualTypeOf<GatewayModels>();
    expectTypeOf<
      GatewayModels['models'][number]['baa']
    >().toEqualTypeOf<boolean>();
    expectTypeOf<GatewayModels['monthlyTokenBudget']>().toEqualTypeOf<number>();
    expectTypeOf<GatewayChatCompletion['created']>().toEqualTypeOf<number>();
    expectTypeOf<GatewayPreviewChatCompletion['created']>().toEqualTypeOf<
      number | undefined
    >();
    expectTypeOf<
      ReturnType<typeof createPreviewModelGatewayClient>
    >().toEqualTypeOf<PreviewModelGatewayClient>();
    expectTypeOf<
      Awaited<ReturnType<PreviewModelGatewayClient['models']>>
    >().toEqualTypeOf<GatewayPreviewModels>();
    // @ts-expect-error Preview catalog does not claim a managed monthly budget.
    type PreviewMonthly = GatewayPreviewModels['monthlyTokenBudget'];
    // @ts-expect-error Preview model selection does not establish a BAA.
    type PreviewBaa = GatewayPreviewModels['models'][number]['baa'];
    type PreviewCompletions = PreviewModelGatewayClient['chat']['completions'];
    // @ts-expect-error The preview gateway does not support streaming.
    type PreviewStreaming = PreviewCompletions['stream'];
    expectTypeOf<PreviewMonthly>().toEqualTypeOf<PreviewBaa>();
    expectTypeOf<PreviewStreaming>().toEqualTypeOf<PreviewBaa>();
  });
  it('validates the preview catalog and uses the exact existing signed gateway route', async () => {
    const raw = catalog(),
      f = client(raw);
    expect(await f.client.models()).toEqual(raw);
    const { url, init } = f.calls[0];
    expect(url).toBe('https://local-preview.example/instance-gateway/models');
    const header = (init.headers as Record<string, string>).authorization;
    const match =
      /^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(header)!;
    expect(match[1]).toBe(id);
    expect(match[4]).toBe(
      createHmacToken({
        method: 'GET',
        path: '/models',
        body: undefined,
        timestamp: new Date(match[2]),
        nonce: match[3],
        secretKey: key
      })
    );
  });
  it.each([
    { scope: 'managed' },
    { monthlyTokenBudget: 1000 },
    { tokensUsedThisMonth: 0 },
    { requestsPerMinute: 10 },
    { models: [{ id: 'reviewed-model', baa: false }] },
    { models: [] },
    { models: [{ id: 'a' }, { id: 'b' }] },
    { maxCalls: 21 },
    { maxCalls: 1.5 },
    { maxUnits: 100001 },
    { maxUnits: 0 },
    { expiresAt: 0 },
    { expiresAt: Number.MAX_SAFE_INTEGER }
  ])('refuses invalid or fabricated preview metadata %j', async (patch) => {
    await expect(
      client(
        'maxCalls' in patch || 'maxUnits' in patch || 'expiresAt' in patch
          ? { ...catalog(), preview: { ...catalog().preview, ...patch } }
          : { ...catalog(), ...patch }
      ).client.models()
    ).rejects.toMatchObject({ status: 502 });
  });
  it('rejects managed catalog instead of silently relabeling it preview', async () => {
    await expect(
      client({
        models: [{ id: 'managed', baa: true }],
        monthlyTokenBudget: 1000,
        tokensUsedThisMonth: 10,
        requestsPerMinute: 30
      }).client.models()
    ).rejects.toBeInstanceOf(ModelGatewayRequestError);
  });
  it('preserves gateway denial without exposing the provider response', async () => {
    await expect(
      client({ error: 'private provider detail' }, 429).client.models()
    ).rejects.toMatchObject({ status: 429 });
  });
  it('cancels oversized catalog output and rejects malformed JSON without echoing content', async () => {
    const cancelled = vi.fn();
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new TextEncoder().encode('private'.repeat(2000)));
      },
      cancel: cancelled
    });
    const options = {
      gatewayUrl: 'https://local-preview.example',
      instanceId: id,
      hmacKey: key
    };
    const oversized = createPreviewModelGatewayClient({
      ...options,
      fetch: async () => new Response(stream)
    });
    await expect(oversized.models()).rejects.toMatchObject({ status: 502 });
    expect(cancelled).toHaveBeenCalledTimes(1);
    const malformed = createPreviewModelGatewayClient({
      ...options,
      fetch: async () => new Response('private invalid JSON')
    });
    await expect(malformed.models()).rejects.toThrow(
      'The local preview model catalog is invalid or expired.'
    );
  });
  it('keeps completion request/body semantics and omits streaming from preview surface', async () => {
    const f = client({
      id: 'reply',
      object: 'chat.completion',
      model: 'reviewed-model',
      choices: []
    });
    const body = {
      model: 'reviewed-model',
      messages: [{ role: 'user' as const, content: 'Hello' }]
    };
    const reply = await f.client.chat.completions.create(body);
    expect(reply.id).toBe('reply');
    expect(reply.created).toBeUndefined();
    expect(f.calls[0].url).toBe(
      'https://local-preview.example/instance-gateway/models/chat/completions'
    );
    expect(JSON.parse(String(f.calls[0].init.body))).toEqual(body);
    expect('stream' in f.client.chat.completions).toBe(false);
  });
});

const publishedModule = process.env.FL_PUBLISHED_MODEL_SDK_MODULE;
it.skipIf(!publishedModule)(
  'published core3.0.1 and additive preview client share HMAC wire transport while managed consumers stay unchanged',
  async () => {
    // Explicit published tarball path supplied by the runner; no test downloads packages.
    const published = await import(pathToFileURL(publishedModule!).href);
    let preview = false;
    const seen = new Set<string>();
    const server = createServer(async (req, res) => {
      const header = req.headers.authorization ?? '';
      const match =
        /^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(header);
      const route = req.url?.replace(/^\/instance-gateway/, '');
      const valid =
        match &&
        match[1] === id &&
        !seen.has(match[3]) &&
        match[4] ===
          createHmacToken({
            method: 'GET',
            path: '/models',
            body: undefined,
            timestamp: new Date(match[2]),
            nonce: match[3],
            secretKey: key
          });
      if (!valid || route !== '/models') {
        res.writeHead(401);
        res.end();
        return;
      }
      seen.add(match![3]);
      res.setHeader('content-type', 'application/json');
      res.end(
        JSON.stringify(
          preview
            ? catalog()
            : {
                models: [{ id: 'managed-model', baa: true }],
                monthlyTokenBudget: 10000,
                tokensUsedThisMonth: 25,
                requestsPerMinute: 30
              }
        )
      );
    });
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
    const options = {
      gatewayUrl: `http://127.0.0.1:${(server.address() as { port: number }).port}`,
      instanceId: id,
      hmacKey: key
    };
    try {
      const old = published.createModelGatewayClient(options),
        managed = createModelGatewayClient(options);
      expect(await managed.models()).toEqual(await old.models());
      preview = true;
      const local = createPreviewModelGatewayClient(options);
      expect((await local.models()).preview.maxCalls).toBe(20);
      // Existing generated code reading only model ids still works with the published client.
      expect((await old.models()).models[0].id).toBe('reviewed-model');
      expect(seen.size).toBe(4);
    } finally {
      await new Promise<void>((r) => server.close(() => r()));
    }
  }
);
