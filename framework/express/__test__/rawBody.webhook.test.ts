import { safeStringify } from '@forklaunch/common';
import { createHmacToken, OpenTelemetryCollector } from '@forklaunch/core/http';
import { SchemaValidator, string } from '@forklaunch/validator/zod';
import { Server } from 'http';
import { forklaunchExpress, forklaunchRouter } from '../index';

const zodSchemaValidator = SchemaValidator();
const openTelemetryCollector = new OpenTelemetryCollector('test');

const PORT = 6480;
const HOST = `http://localhost:${PORT}`;

type RawCapture = { _rawBody?: unknown };

const echoRaw = (req: unknown) => {
  const raw = (req as RawCapture)._rawBody;
  return {
    isBuffer: Buffer.isBuffer(raw),
    raw: Buffer.isBuffer(raw) ? raw.toString('utf8') : String(raw)
  };
};

describe('raw body capture for webhook signature verification', () => {
  let server: Server;

  const app = forklaunchExpress(zodSchemaValidator, openTelemetryCollector, {
    // defect #2: application-level parser options must reach router parsing
    text: {
      type: ['text/plain', 'application/vnd.custom']
    }
  });
  const router = forklaunchRouter(
    '/webhooks',
    zodSchemaValidator,
    openTelemetryCollector
  );

  beforeAll(async () => {
    router.post(
      '/json',
      {
        name: 'JsonWebhook',
        access: 'public',
        summary: 'json body with raw capture',
        body: {
          json: { event: string }
        },
        responses: {
          200: {
            json: {
              bodyType: string,
              parsedEvent: string,
              isBuffer: string,
              raw: string
            }
          }
        }
      },
      (req, res) => {
        const { isBuffer, raw } = echoRaw(req);
        res.status(200).json({
          bodyType: typeof req.body,
          parsedEvent: req.body.event,
          isBuffer: String(isBuffer),
          raw
        });
      }
    );

    router.post(
      '/stripe-style',
      {
        name: 'StripeStyle',
        access: 'public',
        summary: 'text body declared as application/json (defect #1)',
        body: {
          text: string,
          contentType: 'application/json'
        },
        responses: {
          200: { json: { bodyType: string, raw: string } }
        }
      },
      (req, res) => {
        const { raw } = echoRaw(req);
        res.status(200).json({ bodyType: typeof req.body, raw });
      }
    );

    router.post(
      '/custom-text',
      {
        name: 'CustomText',
        access: 'public',
        summary: 'global text options widen accepted types (defect #2)',
        body: {
          text: string
        },
        responses: {
          200: { json: { bodyType: string, raw: string } }
        }
      },
      (req, res) => {
        const { raw } = echoRaw(req);
        res.status(200).json({ bodyType: typeof req.body, raw });
      }
    );

    app.use(router);
    await new Promise<void>((resolve) => {
      server = app.internal.listen(PORT, () => resolve());
    });
  });

  afterAll(() => {
    server?.close();
  });

  test('json route: body parsed AND _rawBody is the exact bytes', async () => {
    const payload = '{"event":  "invoice.paid"}'; // deliberate double space
    const response = await fetch(`${HOST}/webhooks/json`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: payload
    });
    expect(response.status).toBe(200);
    const result = await response.json();
    expect(result.bodyType).toBe('object');
    expect(result.parsedEvent).toBe('invoice.paid');
    expect(result.isBuffer).toBe('true');
    // raw bytes preserved exactly — including formatting JSON.parse discards
    expect(result.raw).toBe(payload);
  });

  test('text route with contentType application/json parses (defect #1)', async () => {
    const payload = '{"id": "evt_123"}';
    const response = await fetch(`${HOST}/webhooks/stripe-style`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: payload
    });
    expect(response.status).toBe(200);
    const result = await response.json();
    expect(result.bodyType).toBe('string');
    expect(result.raw).toBe(payload);
  });

  test('application-level text options widen accepted types (defect #2)', async () => {
    const payload = 'custom payload';
    const response = await fetch(`${HOST}/webhooks/custom-text`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/vnd.custom' },
      body: payload
    });
    expect(response.status).toBe(200);
    const result = await response.json();
    expect(result.bodyType).toBe('string');
    expect(result.raw).toBe(payload);
  });

  test('default text route still accepts text/plain', async () => {
    const response = await fetch(`${HOST}/webhooks/custom-text`, {
      method: 'POST',
      headers: { 'Content-Type': 'text/plain' },
      body: 'plain payload'
    });
    expect(response.status).toBe(200);
    expect((await response.json()).raw).toBe('plain payload');
  });

  test('hmac token over raw bytes matches token signed over the object', () => {
    const body = { name: 'John', nested: { a: 1 } };
    const wireBytes = Buffer.from(safeStringify(body), 'utf8');
    const shared = {
      method: 'POST',
      path: '/webhooks/json',
      timestamp: new Date('2026-01-01T00:00:00.000Z'),
      nonce: 'nonce',
      secretKey: 'secret'
    };
    const signedOverObject = createHmacToken({ ...shared, body });
    const signedOverRawBytes = createHmacToken({ ...shared, body: wireBytes });
    expect(signedOverRawBytes).toBe(signedOverObject);
  });
});
