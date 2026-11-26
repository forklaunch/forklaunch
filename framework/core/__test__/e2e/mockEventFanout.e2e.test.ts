import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { createEmailClient } from '../../src/http/emailClient';
import { HMAC_KEY, type Harness, startHarness } from './harness';
import { verifyPlatformEvent } from '../../src/http';

/**
 * Two projects hold email locally (a service and a worker's server). The
 * gateway mock's MOCK_EVENTS_URL_EMAIL lists both, and each receives every
 * event, signed for it — adding the second never takes the first's route.
 */
function receiver() {
  const received: string[] = [];
  const server: Server = createServer((req, res) => {
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
        received.push(event.type);
        res.writeHead(200).end('ok');
      } catch {
        res.writeHead(401).end('refused');
      }
    });
  });
  return {
    received,
    server,
    async start() {
      await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
      return `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
    }
  };
}

const service = receiver();
const worker = receiver();
let h: Harness;

beforeAll(async () => {
  const urls = [await service.start(), await worker.start()];
  h = await startHarness({ MOCK_EVENTS_URL_EMAIL: urls.join(',') });
});
afterAll(async () => {
  await h?.stop();
  service.server.close();
  worker.server.close();
});

describe('gateway mock event fan-out (e2e)', () => {
  it('delivers each email event to every listed destination', async () => {
    await createEmailClient(h.clientOptions).send({
      to: 'pat@example.com',
      subject: 'Hello',
      text: 'Hi'
    });
    for (let i = 0; i < 50; i++) {
      if (service.received.length && worker.received.length) break;
      await new Promise((r) => setTimeout(r, 50));
    }
    expect(service.received).toEqual(['email.delivered']);
    expect(worker.received).toEqual(['email.delivered']);
  });
});
