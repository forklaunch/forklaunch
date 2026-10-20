import { type ChildProcess, spawn } from 'node:child_process';
import path from 'node:path';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import {
  createModelGatewayClient,
  ModelGatewayRequestError
} from '../src/http';

/**
 * The local model gateway mock (bin/model-gateway-mock.mjs), driven over real
 * HTTP by the real client: it has to accept exactly what the client signs,
 * refuse what the platform would refuse, and speak Foundry's API too.
 */

const PORT = 18000 + Math.floor(Math.random() * 1000);
const KEY = 'local-dev-key';
const BASE = `http://127.0.0.1:${PORT}`;
let mock: ChildProcess;

beforeAll(async () => {
  mock = spawn(
    process.execPath,
    [path.join(__dirname, '..', 'bin', 'model-gateway-mock.mjs')],
    {
      env: {
        ...process.env,
        PORT: String(PORT),
        MOCK_INSTANCE_HMAC_KEY: KEY,
        MOCK_MONTHLY_TOKENS: '60'
      },
      stdio: 'ignore'
    }
  );
  for (let i = 0; i < 50; i++) {
    try {
      if ((await fetch(`${BASE}/health`)).ok) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error('mock did not start');
});

afterAll(() => {
  mock?.kill();
});

const client = (hmacKey = KEY) =>
  createModelGatewayClient({
    gatewayUrl: BASE,
    instanceId: 'local-instance',
    hmacKey
  });

describe('model gateway mock', () => {
  it('lists models and answers a signed completion', async () => {
    const models = await client().models();
    expect(models.models.map((m) => m.id)).toEqual(['sol', 'terra', 'luna']);

    const reply = await client().chat.completions.create({
      model: 'terra',
      messages: [{ role: 'user', content: 'hello there' }]
    });
    expect(reply.choices[0].message.content).toBe('[mock terra] hello there');
    expect(reply.usage?.total_tokens).toBeGreaterThan(0);
  });

  it('streams chunks the client reassembles', async () => {
    let text = '';
    for await (const chunk of client().chat.completions.stream({
      model: 'luna',
      messages: [{ role: 'user', content: 'stream me' }]
    })) {
      text += chunk.choices[0]?.delta?.content ?? '';
    }
    expect(text).toBe('[mock luna] stream me');
  });

  it('refuses a request signed with the wrong key, as the platform would', async () => {
    const error = await client('wrong-key')
      .models()
      .catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ModelGatewayRequestError);
    expect((error as ModelGatewayRequestError).status).toBe(401);
  });

  it('refuses a model the product did not enable', async () => {
    const error = await client()
      .chat.completions.create({
        model: 'gpt-4o',
        messages: [{ role: 'user', content: 'x' }]
      })
      .catch((e: unknown) => e);
    expect((error as ModelGatewayRequestError).status).toBe(403);
  });

  it('speaks the Foundry API for the platform side', async () => {
    const response = await fetch(`${BASE}/openai/v1/chat/completions`, {
      method: 'POST',
      headers: {
        authorization: 'Bearer anything',
        'content-type': 'application/json'
      },
      body: JSON.stringify({
        model: 'gpt-5.6-terra',
        messages: [{ role: 'user', content: 'ping' }]
      })
    });
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.choices[0].message.content).toBe('[mock gpt-5.6-terra] ping');
  });

  it('runs the monthly budget down and then answers 429 with retry-after', async () => {
    let error: unknown;
    for (let i = 0; i < 20 && !error; i++) {
      await client()
        .chat.completions.create({
          model: 'sol',
          messages: [{ role: 'user', content: 'spend some tokens please' }]
        })
        .catch((e: unknown) => {
          error = e;
        });
    }
    expect(error).toMatchObject({ status: 429, retryAfterSeconds: 3600 });
  });
});
