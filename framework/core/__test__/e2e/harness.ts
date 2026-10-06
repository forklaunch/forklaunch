import { type ChildProcess, spawn } from 'node:child_process';
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import path from 'node:path';
import {
  type PlatformEvent,
  verifyPlatformEvent
} from '../../src/http/instanceGateway';

/**
 * End-to-end harness for managed features: the real gateway mock
 * (bin/gateway-mock/server.mjs) over real HTTP, plus an app-side receiver
 * that verifies platform events the way a generated service does.
 *
 *   const h = await startHarness();
 *   const client = createXClient(h.clientOptions);
 *   ...
 *   await h.triggerEvent('payments', 'checkout.session.completed', {...});
 *   expect(h.received('payments')).toHaveLength(1);
 *   await h.stop();
 */

export const INSTANCE_ID = 'e2e-instance';
export const HMAC_KEY = 'e2e-instance-key';

export interface Harness {
  gatewayUrl: string;
  clientOptions: { gatewayUrl: string; instanceId: string; hmacKey: string };
  /** Events the app received and verified, per feature. */
  received(feature?: string): PlatformEvent[];
  /** Deliveries the app refused (bad signature, replay, …). */
  rejected(): string[];
  /** Ask the mock to deliver an event to the app now. */
  triggerEvent(
    feature: string,
    type: string,
    data?: Record<string, unknown>
  ): Promise<{ status: number | null; error: string | null }>;
  /** Gateway calls the mock received, for a feature. */
  requests(feature?: string): Promise<
    { feature: string; method: string; path: string; body?: unknown }[]
  >;
  reset(): Promise<void>;
  stop(): Promise<void>;
}

export async function startHarness(
  env: Record<string, string> = {}
): Promise<Harness> {
  const received: PlatformEvent[] = [];
  const rejected: string[] = [];
  const app: Server = createServer((req, res) => {
    let raw = '';
    req.setEncoding('utf8');
    req.on('data', (c) => (raw += c));
    req.on('end', () => {
      try {
        const event = verifyPlatformEvent(
          {
            method: req.method ?? 'POST',
            path: new URL(req.url ?? '/', 'http://app').pathname,
            headers: req.headers,
            body: raw
          },
          { hmacKey: HMAC_KEY }
        );
        received.push(event);
        res.writeHead(200).end('ok');
      } catch (error) {
        rejected.push(String((error as Error).message));
        res.writeHead(401).end('refused');
      }
    });
  });
  await new Promise<void>((r) => app.listen(0, '127.0.0.1', r));
  const appUrl = `http://127.0.0.1:${(app.address() as AddressInfo).port}`;

  const port = 20000 + Math.floor(Math.random() * 20000);
  const gatewayUrl = `http://127.0.0.1:${port}`;
  const mock: ChildProcess = spawn(
    process.execPath,
    [path.join(__dirname, '..', '..', 'bin', 'gateway-mock', 'server.mjs')],
    {
      env: {
        ...process.env,
        PORT: String(port),
        MOCK_INSTANCE_HMAC_KEY: HMAC_KEY,
        MOCK_EVENTS_URL: appUrl,
        ...env
      },
      stdio: 'ignore'
    }
  );
  let up = false;
  for (let i = 0; i < 100 && !up; i++) {
    try {
      up = (await fetch(`${gatewayUrl}/health`)).ok;
    } catch {
      await new Promise((r) => setTimeout(r, 100));
    }
  }
  if (!up) {
    mock.kill();
    app.close();
    throw new Error('gateway mock did not start');
  }

  return {
    gatewayUrl,
    clientOptions: { gatewayUrl, instanceId: INSTANCE_ID, hmacKey: HMAC_KEY },
    received: (feature) =>
      feature ? received.filter((e) => e.feature === feature) : [...received],
    rejected: () => [...rejected],
    async triggerEvent(feature, type, data = {}) {
      const response = await fetch(`${gatewayUrl}/__mock/events/${feature}`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ type, data })
      });
      return (await response.json()) as {
        status: number | null;
        error: string | null;
      };
    },
    async requests(feature) {
      const q = feature ? `?feature=${feature}` : '';
      return (await (
        await fetch(`${gatewayUrl}/__mock/requests${q}`)
      ).json()) as never;
    },
    async reset() {
      received.length = 0;
      rejected.length = 0;
      await fetch(`${gatewayUrl}/__mock/reset`, { method: 'POST' });
    },
    async stop() {
      mock.kill();
      await new Promise<void>((r) => app.close(() => r()));
    }
  };
}
