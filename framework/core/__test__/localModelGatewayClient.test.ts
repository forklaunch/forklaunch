import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  createLocalModelGatewayClient,
  type LocalModelCredential
} from '../src/http/localModelGatewayClient';
const grant = (): LocalModelCredential => ({
  grantId: 'a'.repeat(64),
  capability: 'b'.repeat(64),
  gatewayBaseUrl: `https://studio-orchestrator.forklaunch.com/studio-orchestrator/desktop-runtime-model-gateway/${'a'.repeat(64)}/v1`,
  model: 'model-a',
  expiresAt: Date.now() + 60000,
  maxCalls: 20,
  maxUnits: 100000,
  scope: 'local-runtime'
});
afterEach(() => vi.useRealTimers());
describe('local account model connection', () => {
  it('shares one credential, sends only the limited capability, and never retries refused inference', async () => {
    const provider = vi.fn(async () => grant());
    const send = vi.fn(
      async (_url: string | URL | Request, _init?: RequestInit) =>
        Response.json({ error: 'private upstream detail' }, { status: 429 })
    );
    const client = createLocalModelGatewayClient({
      credentialProvider: provider,
      fetch: send as typeof fetch
    });
    await Promise.all([client.models(), client.models()]);
    expect(provider).toHaveBeenCalledTimes(1);
    await expect(
      client.chat.completions.create({
        model: 'model-a',
        messages: [{ role: 'user', content: 'hello' }]
      })
    ).rejects.toMatchObject({ status: 429 });
    expect(send).toHaveBeenCalledTimes(1);
    expect(
      (send.mock.calls[0][1]!.headers as Record<string, string>).authorization
    ).toBe(`Bearer ${'b'.repeat(64)}`);
  });
  it('renews expired credentials on the next operation, not a failed inference', async () => {
    vi.useFakeTimers();
    const provider = vi.fn(async () => grant());
    const client = createLocalModelGatewayClient({
      credentialProvider: provider
    });
    await client.models();
    vi.advanceTimersByTime(61000);
    await client.models();
    expect(provider).toHaveBeenCalledTimes(2);
  });
  it('rejects foreign destinations and unauthorized model choices before sending', async () => {
    const send = vi.fn();
    const client = createLocalModelGatewayClient({
      credentialProvider: async () => ({
        ...grant(),
        gatewayBaseUrl: 'https://evil.invalid'
      }),
      fetch: send
    });
    await expect(client.models()).rejects.toThrow('Invalid local model');
    const good = createLocalModelGatewayClient({
      credentialProvider: async () => grant(),
      fetch: send
    });
    await expect(
      good.chat.completions.create({ model: 'other', messages: [] })
    ).rejects.toMatchObject({ status: 400 });
    expect(send).not.toHaveBeenCalled();
  });
});
