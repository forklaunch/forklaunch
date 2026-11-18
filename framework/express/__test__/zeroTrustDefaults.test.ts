import {
  generateHmacAuthHeaders,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { SchemaValidator, string } from '@forklaunch/validator/zod';
import { Server } from 'http';
import { AddressInfo } from 'net';
import { forklaunchExpress, forklaunchRouter } from '../index';

/**
 * The secure defaults, through a real server: CORS when the app names no
 * origins, and replay protection on an HMAC-signed internal route.
 */
const zodSchemaValidator = SchemaValidator();
const openTelemetryCollector = new OpenTelemetryCollector('test', 'silent');

const SECRET = 'internal-service-secret';

function buildApp(options: Parameters<typeof forklaunchExpress>[2] = {}) {
  const app = forklaunchExpress(zodSchemaValidator, openTelemetryCollector, {
    mcp: false,
    ...options
  });
  const router = forklaunchRouter(
    '/internal',
    zodSchemaValidator,
    openTelemetryCollector
  );
  router.get(
    '/ping',
    {
      name: 'Ping',
      access: 'public',
      summary: 'public route',
      responses: { 200: string }
    },
    (_req, res) => {
      res.status(200).send('pong');
    }
  );
  router.post(
    '/charge',
    {
      name: 'Charge',
      access: 'internal',
      summary: 'service-to-service route',
      auth: { hmac: { secretKeys: { billing: SECRET } } },
      body: { json: { amount: string } },
      responses: { 200: string }
    },
    (_req, res) => {
      res.status(200).send('charged');
    }
  );
  app.use(router);
  return app;
}

async function listen(app: ReturnType<typeof buildApp>) {
  const server: Server = await new Promise((resolve) => {
    const listening = app.internal.listen(0, '127.0.0.1', () =>
      resolve(listening)
    );
  });
  const { port } = server.address() as AddressInfo;
  return { server, host: `http://127.0.0.1:${port}` };
}

describe('CORS default (no cors option)', () => {
  const servers: Server[] = [];
  afterAll(() => {
    for (const server of servers) server.close();
  });

  async function start(
    env: Record<string, string | undefined>,
    options?: Parameters<typeof forklaunchExpress>[2]
  ) {
    const previous: Record<string, string | undefined> = {};
    for (const [key, value] of Object.entries(env)) {
      previous[key] = process.env[key];
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    try {
      const { server, host } = await listen(buildApp(options));
      servers.push(server);
      return host;
    } finally {
      for (const [key, value] of Object.entries(previous)) {
        if (value === undefined) delete process.env[key];
        else process.env[key] = value;
      }
    }
  }

  const allowOrigin = async (
    host: string,
    origin: string,
    method: 'GET' | 'OPTIONS' = 'GET'
  ) => {
    const response = await fetch(`${host}/internal/ping`, {
      method,
      headers: {
        origin,
        ...(method === 'OPTIONS'
          ? { 'access-control-request-method': 'GET' }
          : {})
      }
    });
    await response.text();
    return response.headers.get('access-control-allow-origin');
  };

  test('nothing configured: the wildcard is not returned; only localhost is allowed outside production', async () => {
    const host = await start({ CORS_ORIGINS: undefined });
    expect(await allowOrigin(host, 'https://app.example.com')).toBeNull();
    expect(
      await allowOrigin(host, 'https://app.example.com', 'OPTIONS')
    ).toBeNull();
    expect(await allowOrigin(host, 'http://localhost:3000')).toBe(
      'http://localhost:3000'
    );
    // The route itself still answers; CORS only decides what browsers may read.
    const response = await fetch(`${host}/internal/ping`);
    expect(await response.text()).toBe('pong');
    expect(response.headers.get('access-control-allow-origin')).toBeNull();
  });

  test('nothing configured in production: no origin is allowed', async () => {
    const host = await start({
      CORS_ORIGINS: undefined,
      NODE_ENV: 'production'
    });
    for (const origin of ['https://app.example.com', 'http://localhost:3000']) {
      expect(await allowOrigin(host, origin)).toBeNull();
      expect(await allowOrigin(host, origin, 'OPTIONS')).toBeNull();
    }
  });

  test('CORS_ORIGINS: exactly the listed origins are allowed', async () => {
    const host = await start({
      CORS_ORIGINS: 'https://app.example.com,https://admin.example.com',
      NODE_ENV: 'production'
    });
    expect(await allowOrigin(host, 'https://app.example.com')).toBe(
      'https://app.example.com'
    );
    expect(await allowOrigin(host, 'https://admin.example.com', 'OPTIONS')).toBe(
      'https://admin.example.com'
    );
    expect(await allowOrigin(host, 'https://evil.example.com')).toBeNull();
    expect(await allowOrigin(host, 'http://localhost:3000')).toBeNull();
  });

  test('an explicit cors option is used as given', async () => {
    const host = await start(
      { CORS_ORIGINS: 'https://ignored.example.com', NODE_ENV: 'production' },
      { cors: { origin: 'https://configured.example.com', credentials: true } }
    );
    const response = await fetch(`${host}/internal/ping`, {
      headers: { origin: 'https://configured.example.com' }
    });
    await response.text();
    expect(response.headers.get('access-control-allow-origin')).toBe(
      'https://configured.example.com'
    );
    expect(response.headers.get('access-control-allow-credentials')).toBe(
      'true'
    );
  });
});

describe('HMAC internal route: replay protection', () => {
  let server: Server;
  let host: string;

  beforeAll(async () => {
    ({ server, host } = await listen(buildApp()));
  });
  afterAll(() => {
    server?.close();
  });

  const body = JSON.stringify({ amount: '100' });
  const signed = () =>
    generateHmacAuthHeaders({
      secretKey: SECRET,
      method: 'POST',
      path: '/internal/charge',
      body,
      keyId: 'billing'
    }).authorization;
  const charge = (authorization: string, sentBody = body) =>
    fetch(`${host}/internal/charge`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization },
      body: sentBody
    });

  test('a fresh signed request passes; sending the same request again is rejected', async () => {
    const authorization = signed();
    const first = await charge(authorization);
    expect(first.status).toBe(200);
    expect(await first.text()).toBe('charged');

    const replay = await charge(authorization);
    expect(replay.status).toBe(403);
    await replay.text();

    // A retry that signs again gets a new nonce and passes.
    const retry = await charge(signed());
    expect(retry.status).toBe(200);
    await retry.text();
  });

  test('a tampered body, a tampered signature and a stale timestamp are rejected', async () => {
    const tamperedBody = await charge(
      signed(),
      JSON.stringify({ amount: '999999' })
    );
    expect(tamperedBody.status).toBe(403);
    await tamperedBody.text();

    const authorization = signed();
    const signature = authorization.match(/signature=(\S+)$/)?.[1] as string;
    const forged = authorization.replace(
      /signature=\S+$/,
      `signature=${signature[0] === 'A' ? 'B' : 'A'}${signature.slice(1)}`
    );
    const tamperedSignature = await charge(forged);
    expect(tamperedSignature.status).toBe(403);
    await tamperedSignature.text();

    const stale = await charge(
      authorization.replace(
        /ts=\S+/,
        `ts=${new Date(Date.now() - 10 * 60_000).toISOString()}`
      )
    );
    expect(stale.status).toBe(403);
    await stale.text();

    const malformed = await charge(
      authorization.replace(/ts=\S+/, 'ts=yesterday')
    );
    expect(malformed.status).toBe(401);
    await malformed.text();
  });
});
