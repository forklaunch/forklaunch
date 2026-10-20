import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { SchemaValidator, string } from '@forklaunch/validator/zod';
import type { Server } from 'node:http';
import { forklaunchExpress, forklaunchRouter } from '../index';

// Exercise the actual content parser and final error handler, not a mocked
// status resolver. A malformed client request must never become server failure.
describe('HTTP error status preservation', () => {
  let server: Server;
  let origin: string;
  let handlerCalls = 0;
  const telemetry = new OpenTelemetryCollector(
    'http-error-status',
    'error',
    {}
  );
  const validator = SchemaValidator();
  const app = forklaunchExpress(validator, telemetry, {
    json: { limit: '64b' }
  });
  const router = forklaunchRouter('/input', validator, telemetry);

  beforeAll(async () => {
    router.post(
      '/json',
      {
        name: 'ParserStatus',
        summary: 'Verify parser status',
        access: 'public',
        body: { json: { task: string } },
        responses: { 200: { json: { accepted: string } } }
      },
      (_req, res) => {
        handlerCalls++;
        res.status(200).json({ accepted: 'yes' });
      }
    );
    app.use(router);
    app.internal.get('/failure/:status', (req, res, next) => {
      if (req.params.status === 'preset') res.status(409);
      next(
        Object.assign(new Error('PRIVATE_ERROR_DETAIL'), {
          status:
            req.params.status === 'string' ? '400' : Number(req.params.status)
        })
      );
    });
    await new Promise<void>((resolve) => {
      server = app.listen(0, '127.0.0.1', resolve);
    });
    const address = server.address();
    if (!address || typeof address === 'string')
      throw new Error('No test address');
    origin = `http://127.0.0.1:${address.port}`;
  });
  afterAll(async () => {
    if (!server) return;
    await new Promise<void>((resolve, reject) =>
      server.close((error) => (error ? reject(error) : resolve()))
    );
  });

  test.each([
    ['null', 'application/json', 400],
    ['{"task":"PRIVATE_BODY"', 'application/json', 400],
    [JSON.stringify({ task: 'x'.repeat(128) }), 'application/json', 413],
    ['{"task":"ok"}', 'application/json; charset=unsupported', 415]
  ])(
    'rejects parser input with its client status: %s',
    async (body, contentType, expected) => {
      const before = handlerCalls;
      const response = await fetch(`${origin}/input/json`, {
        method: 'POST',
        headers: { 'content-type': contentType },
        body
      });
      expect(response.status).toBe(expected);
      expect(handlerCalls).toBe(before);
      const text = await response.text();
      expect(text).not.toContain('PRIVATE_BODY');
      expect(text).not.toContain('SyntaxError');
      expect(text).toContain('Request could not be processed');
    }
  );

  test('valid JSON still reaches the handler', async () => {
    const response = await fetch(`${origin}/input/json`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ task: 'ok' })
    });
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ accepted: 'yes' });
  });

  test.each([
    ['503', 503],
    ['preset', 409],
    ['0', 500],
    ['600', 500],
    ['string', 500],
    ['missing', 500]
  ])(
    'preserves valid HTTP errors and sanitizes invalid status %s',
    async (status, expected) => {
      const response = await fetch(`${origin}/failure/${status}`);
      expect(response.status).toBe(expected);
      const text = await response.text();
      expect(text).not.toContain('PRIVATE_ERROR_DETAIL');
      expect(text).not.toContain('Error:');
    }
  );
});
