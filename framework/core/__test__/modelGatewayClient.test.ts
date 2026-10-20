import { describe, expect, it } from 'vitest';
import {
  createHmacToken,
  createModelGatewayClient,
  ModelGatewayRequestError
} from '../src/http';

/**
 * The instance-side model gateway client signs every call the way the
 * platform verifies it (router-relative path, the body as parsed, the
 * instance id as key id), and turns the gateway's event stream into chunks.
 */

const KEY = 'instance-hmac-key';

function recordingFetch(
  respond: (request: { url: string; method: string; body?: string }) => Response
) {
  const requests: {
    url: string;
    method: string;
    authorization: string;
    body?: string;
  }[] = [];
  const fetchImpl = (async (url: string, init: RequestInit) => {
    const request = {
      url,
      method: String(init.method),
      authorization: (init.headers as Record<string, string>).authorization,
      body: init.body ? String(init.body) : undefined
    };
    requests.push(request);
    return respond(request);
  }) as unknown as typeof fetch;
  return { fetchImpl, requests };
}

/** What the platform does: parse the header, recompute over path + parsed body. */
function verifies(
  request: { method: string; authorization: string; body?: string },
  path: string
): boolean {
  const match = /^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(
    request.authorization
  );
  if (!match) return false;
  const expected = createHmacToken({
    method: request.method,
    path,
    body: request.body ? JSON.parse(request.body) : undefined,
    timestamp: new Date(match[2]),
    nonce: match[3],
    secretKey: KEY
  });
  return match[1] === 'inst-1' && expected === match[4];
}

const client = (fetchImpl: typeof fetch) =>
  createModelGatewayClient({
    gatewayUrl: 'https://platform.example.com/',
    instanceId: 'inst-1',
    hmacKey: KEY,
    fetch: fetchImpl
  });

describe('model gateway client', () => {
  it('refuses to exist outside managed mode', () => {
    const saved = { ...process.env };
    delete process.env.PLATFORM_GATEWAY_URL;
    delete process.env.INSTANCE_ID;
    delete process.env.INSTANCE_HMAC_KEY;
    try {
      expect(() => createModelGatewayClient()).toThrow(/managed mode/);
    } finally {
      process.env = saved;
    }
  });

  it('signs a completion exactly as the gateway verifies it', async () => {
    const { fetchImpl, requests } = recordingFetch(
      () =>
        new Response(
          JSON.stringify({
            id: 'x',
            object: 'chat.completion',
            created: 1,
            model: 'terra',
            choices: []
          })
        )
    );
    const reply = await client(fetchImpl).chat.completions.create({
      model: 'terra',
      messages: [{ role: 'user', content: 'hi' }],
      temperature: 0
    });
    expect(reply.model).toBe('terra');
    expect(requests[0].url).toBe(
      'https://platform.example.com/instance-gateway/models/chat/completions'
    );
    expect(verifies(requests[0], '/models/chat/completions')).toBe(true);
    expect(
      verifies(requests[0], '/instance-gateway/models/chat/completions')
    ).toBe(false);
  });

  it('signs a GET with no body', async () => {
    const { fetchImpl, requests } = recordingFetch(
      () =>
        new Response(
          JSON.stringify({
            models: [],
            monthlyTokenBudget: 1,
            tokensUsedThisMonth: 0,
            requestsPerMinute: 1
          })
        )
    );
    await client(fetchImpl).models();
    expect(requests[0].body).toBeUndefined();
    expect(verifies(requests[0], '/models')).toBe(true);
  });

  it('streams chunks until [DONE], across split reads', async () => {
    const events =
      'id: 0\ndata: {"id":"c","object":"chat.completion.chunk","model":"luna","choices":[{"index":0,"delta":{"content":"Hel"}}]}\n\n' +
      'id: 1\ndata: {"id":"c","object":"chat.completion.chunk","model":"luna","choices":[{"index":0,"delta":{"content":"lo"}}]}\n\n' +
      'id: 2\ndata: [DONE]\n\n';
    const bytes = new TextEncoder().encode(events);
    const { fetchImpl, requests } = recordingFetch(
      () =>
        new Response(
          new ReadableStream({
            start(controller) {
              controller.enqueue(bytes.slice(0, 40));
              controller.enqueue(bytes.slice(40, 120));
              controller.enqueue(bytes.slice(120));
              controller.close();
            }
          })
        )
    );
    let text = '';
    for await (const chunk of client(fetchImpl).chat.completions.stream({
      model: 'luna',
      messages: [{ role: 'user', content: 'hi' }]
    })) {
      text += chunk.choices[0]?.delta?.content ?? '';
    }
    expect(text).toBe('Hello');
    expect(requests[0].url).toMatch(
      /\/instance-gateway\/models\/chat\/completions\/stream$/
    );
    expect(verifies(requests[0], '/models/chat/completions/stream')).toBe(true);
  });

  it('turns a refusal into an error with its status and retry-after', async () => {
    const { fetchImpl } = recordingFetch(
      () =>
        new Response('Monthly token budget of 1000 is spent', {
          status: 429,
          headers: { 'retry-after': '3600' }
        })
    );
    const error = await client(fetchImpl)
      .chat.completions.create({ model: 'terra', messages: [] })
      .catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ModelGatewayRequestError);
    expect(error).toMatchObject({
      status: 429,
      retryAfterSeconds: 3600,
      message: 'Monthly token budget of 1000 is spent'
    });
  });
});
