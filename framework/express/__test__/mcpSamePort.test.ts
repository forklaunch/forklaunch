import {
  createMcpHttpHandler,
  ExpressLikeApplicationOptions,
  OpenTelemetryCollector,
  SessionObject
} from '@forklaunch/core/http';
import { SchemaValidator, string } from '@forklaunch/validator/zod';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';
import { FastMCP } from 'fastmcp';
import http from 'http';
import { forklaunchExpress, forklaunchRouter } from '../index';

const zodSchemaValidator = SchemaValidator();
const openTelemetryCollector = new OpenTelemetryCollector('test');

const BASE_PORT = 6491;

type McpOptions = Exclude<
  ExpressLikeApplicationOptions<
    ReturnType<typeof SchemaValidator>,
    SessionObject<ReturnType<typeof SchemaValidator>>
  >['mcp'],
  false | undefined
>;

function buildApp(mcp?: McpOptions) {
  const app = forklaunchExpress(zodSchemaValidator, openTelemetryCollector, {
    mcp
  });
  const router = forklaunchRouter(
    '/greetings',
    zodSchemaValidator,
    openTelemetryCollector
  );
  router.get(
    '/:name',
    {
      name: 'GetGreeting',
      access: 'public',
      summary: 'Greets a name',
      params: { name: string },
      responses: { 200: { json: { greeting: string } } }
    },
    (req, res) => {
      res.status(200).json({ greeting: `hello ${req.params.name}` });
    }
  );
  router.get(
    '/echo/headers',
    {
      name: 'EchoHeaders',
      access: 'public',
      summary: 'Echoes the forwarded caller headers',
      responses: {
        200: { json: { authorization: string, correlationId: string } }
      }
    },
    (req, res) => {
      res.status(200).json({
        authorization: String(req.headers.authorization ?? ''),
        correlationId: String(req.headers['x-correlation-id'] ?? '')
      });
    }
  );
  app.use(router);
  return app;
}

async function start(
  app: ReturnType<typeof buildApp>,
  port: number
): Promise<() => Promise<void>> {
  const server = app.listen(port, 'localhost');
  await new Promise<void>((resolve) => server.once('listening', resolve));
  return () =>
    new Promise<void>((resolve) => {
      server.closeAllConnections();
      server.close(() => resolve());
    });
}

/** Sends each request to the next upstream in turn, like a load balancer
 * without sticky sessions in front of several tasks. */
async function startRoundRobinProxy(
  port: number,
  upstreamPorts: number[]
): Promise<() => Promise<void>> {
  let next = 0;
  const server = http.createServer((req, res) => {
    const target = upstreamPorts[next++ % upstreamPorts.length];
    const upstream = http.request(
      {
        host: 'localhost',
        port: target,
        path: req.url,
        method: req.method,
        headers: req.headers
      },
      (upstreamResponse) => {
        res.writeHead(
          upstreamResponse.statusCode ?? 502,
          upstreamResponse.headers
        );
        upstreamResponse.pipe(res);
      }
    );
    upstream.on('error', () => {
      res.writeHead(502);
      res.end();
    });
    req.pipe(upstream);
  });
  server.listen(port, 'localhost');
  await new Promise<void>((resolve) => server.once('listening', resolve));
  return () =>
    new Promise<void>((resolve) => {
      server.closeAllConnections();
      server.close(() => resolve());
    });
}

async function connect(port: number, headers?: Record<string, string>) {
  const client = new Client({ name: 'test-client', version: '1.0.0' });
  const transport = new StreamableHTTPClientTransport(
    new URL(`http://localhost:${port}/mcp`),
    headers ? { requestInit: { headers } } : undefined
  );
  await client.connect(transport);
  return { client, transport };
}

const initializeBody = JSON.stringify({
  jsonrpc: '2.0',
  id: 1,
  method: 'initialize',
  params: {
    protocolVersion: '2025-06-18',
    capabilities: {},
    clientInfo: { name: 'raw', version: '1' }
  }
});
const mcpHeaders = {
  'Content-Type': 'application/json',
  Accept: 'application/json, text/event-stream'
};

describe('MCP served on the application port', () => {
  const PORT = BASE_PORT;
  let stop: () => Promise<void>;

  beforeAll(async () => {
    stop = await start(buildApp(), PORT);
  });

  afterAll(async () => {
    await stop();
  });

  it('serves the API, health and MCP on the same port', async () => {
    const api = await fetch(`http://localhost:${PORT}/greetings/ada`);
    expect(api.status).toBe(200);
    expect(await api.json()).toEqual({ greeting: 'hello ada' });

    const health = await fetch(`http://localhost:${PORT}/health`);
    expect(health.status).toBe(200);

    const { client } = await connect(PORT);
    const { tools } = await client.listTools();
    expect(tools.map((tool) => tool.name)).toContain('GetGreeting');

    const result = await client.callTool({
      name: 'GetGreeting',
      arguments: { params: { name: 'grace' } }
    });
    expect(result.isError).toBeFalsy();
    expect(JSON.stringify(result.content)).toContain('hello grace');
    await client.close();
  });

  it('does not open a second listener on PORT + 2000', async () => {
    await expect(
      fetch(`http://localhost:${PORT + 2000}/mcp`, {
        method: 'POST',
        headers: mcpHeaders,
        body: initializeBody
      })
    ).rejects.toThrow();
  });

  it('is stateless by default: no session id, any request stands alone', async () => {
    const initialize = await fetch(`http://localhost:${PORT}/mcp`, {
      method: 'POST',
      headers: mcpHeaders,
      body: initializeBody
    });
    expect(initialize.status).toBe(200);
    expect(initialize.headers.get('mcp-session-id')).toBeNull();
    await initialize.text();

    const list = await fetch(`http://localhost:${PORT}/mcp`, {
      method: 'POST',
      headers: { ...mcpHeaders, 'mcp-protocol-version': '2025-06-18' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/list' })
    });
    expect(list.status).toBe(200);
    expect(await list.text()).toContain('GetGreeting');

    const stream = await fetch(`http://localhost:${PORT}/mcp`, {
      method: 'GET',
      headers: { Accept: 'text/event-stream' }
    });
    expect(stream.status).toBe(405);
  });

  it('forwards the caller credentials and correlation id to routes', async () => {
    const { client } = await connect(PORT, {
      Authorization: 'Bearer caller-token',
      'x-correlation-id': 'corr-123'
    });
    const result = await client.callTool({
      name: 'EchoHeaders',
      arguments: {}
    });
    expect(result.isError).toBeFalsy();
    expect(JSON.parse((result.content as { text: string }[])[0].text)).toEqual({
      authorization: 'Bearer caller-token',
      correlationId: 'corr-123'
    });
    await client.close();
  });
});

describe('MCP behind a load balancer without sticky sessions', () => {
  const [A, B, PROXY, STATEFUL_A, STATEFUL_B, STATEFUL_PROXY] = [
    2, 4, 6, 8, 10, 12
  ].map((offset) => BASE_PORT + offset);
  const stops: (() => Promise<void>)[] = [];

  beforeAll(async () => {
    stops.push(
      await start(buildApp(), A),
      await start(buildApp(), B),
      await startRoundRobinProxy(PROXY, [A, B]),
      await start(buildApp({ sessions: 'stateful' }), STATEFUL_A),
      await start(buildApp({ sessions: 'stateful' }), STATEFUL_B),
      await startRoundRobinProxy(STATEFUL_PROXY, [STATEFUL_A, STATEFUL_B])
    );
  });

  afterAll(async () => {
    await Promise.all(stops.map((stop) => stop()));
  });

  it('works across instances in the default stateless mode', async () => {
    const { client } = await connect(PROXY);
    for (const name of ['ada', 'grace', 'linus', 'barbara']) {
      const result = await client.callTool({
        name: 'GetGreeting',
        arguments: { params: { name } }
      });
      expect(JSON.stringify(result.content)).toContain(`hello ${name}`);
    }
    await client.close();
  });

  it('breaks across instances in stateful mode', async () => {
    await expect(
      (async () => {
        const { client } = await connect(STATEFUL_PROXY);
        await client.listTools();
        await client.listTools();
      })()
    ).rejects.toThrow();
  });
});

describe('MCP stateful sessions', () => {
  const PORT = BASE_PORT + 14;
  let stop: () => Promise<void>;

  beforeAll(async () => {
    stop = await start(
      buildApp({ sessions: 'stateful', sessionIdleTimeoutMs: 300 }),
      PORT
    );
  });

  afterAll(async () => {
    await stop();
  });

  it('keeps sessions separate and rejects unknown session ids', async () => {
    const a = await connect(PORT);
    const b = await connect(PORT);
    expect(a.transport.sessionId).toBeDefined();
    expect(a.transport.sessionId).not.toBe(b.transport.sessionId);

    const unknown = await fetch(`http://localhost:${PORT}/mcp`, {
      method: 'POST',
      headers: { ...mcpHeaders, 'mcp-session-id': 'not-a-session' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/list' })
    });
    expect(unknown.status).toBe(404);

    await a.transport.terminateSession();
    const afterDelete = await fetch(`http://localhost:${PORT}/mcp`, {
      method: 'POST',
      headers: {
        ...mcpHeaders,
        'mcp-session-id': a.transport.sessionId ?? 'missing',
        'mcp-protocol-version': '2025-06-18'
      },
      body: JSON.stringify({ jsonrpc: '2.0', id: 3, method: 'tools/list' })
    });
    expect(afterDelete.status).toBe(404);

    const stillWorks = await b.client.listTools();
    expect(stillWorks.tools.length).toBeGreaterThan(0);
    await a.client.close();
    await b.client.close();
  });

  it('rejects requests other than initialize without a session', async () => {
    const response = await fetch(`http://localhost:${PORT}/mcp`, {
      method: 'POST',
      headers: mcpHeaders,
      body: JSON.stringify({ jsonrpc: '2.0', id: 4, method: 'tools/list' })
    });
    expect(response.status).toBe(400);
  });

  it('closes sessions that sit idle past the timeout', async () => {
    const { client, transport } = await connect(PORT);
    expect((await client.listTools()).tools.length).toBeGreaterThan(0);

    await new Promise((resolve) => setTimeout(resolve, 1000));

    const expired = await fetch(`http://localhost:${PORT}/mcp`, {
      method: 'POST',
      headers: {
        ...mcpHeaders,
        'mcp-session-id': transport.sessionId ?? 'missing',
        'mcp-protocol-version': '2025-06-18'
      },
      body: JSON.stringify({ jsonrpc: '2.0', id: 5, method: 'tools/list' })
    });
    expect(expired.status).toBe(404);
    await client.close().catch(() => undefined);
  });

  it('refuses stateful sessions with more than one worker', () => {
    expect(() =>
      createMcpHttpHandler(new FastMCP({ name: 'x', version: '1.0.0' }), {
        sessions: 'stateful',
        workerCount: 2
      })
    ).toThrow(/cannot be shared across workers/);
  });
});

describe('MCP on the application port with authenticate', () => {
  const PORT = BASE_PORT + 16;
  let stop: () => Promise<void>;

  beforeAll(async () => {
    stop = await start(
      buildApp({
        authenticate: async (request) =>
          request.headers.authorization === 'Bearer good-token'
            ? { user: 'ada' }
            : undefined
      }),
      PORT
    );
  });

  afterAll(async () => {
    await stop();
  });

  it('returns 401 without a valid token', async () => {
    const response = await fetch(`http://localhost:${PORT}/mcp`, {
      method: 'POST',
      headers: mcpHeaders,
      body: initializeBody
    });
    expect(response.status).toBe(401);
    expect(response.headers.get('www-authenticate')).toBe('Bearer');

    await expect(
      connect(PORT, { Authorization: 'Bearer bad-token' })
    ).rejects.toThrow();
  });

  it('serves tools with a valid token and passes it to routes', async () => {
    const { client } = await connect(PORT, {
      Authorization: 'Bearer good-token'
    });
    const { tools } = await client.listTools();
    expect(tools.map((tool) => tool.name)).toContain('GetGreeting');

    const result = await client.callTool({
      name: 'EchoHeaders',
      arguments: {}
    });
    expect(
      JSON.parse((result.content as { text: string }[])[0].text).authorization
    ).toBe('Bearer good-token');
    await client.close();
  });
});
