import { WebStandardStreamableHTTPServerTransport } from '@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js';
import { AsyncLocalStorage } from 'async_hooks';
import { randomUUID } from 'crypto';
import { FastMCP, FastMCPSession } from 'fastmcp';
import http from 'http';

type McpAuthenticate = (
  request: http.IncomingMessage
) => Promise<Record<string, unknown> | undefined>;

type McpSessionEntry = {
  transport: WebStandardStreamableHTTPServerTransport;
  session: FastMCPSession<Record<string, unknown> | undefined>;
  lastSeen: number;
};

/**
 * `stateless` (the default) serves every request with a fresh MCP session, so
 * any worker or task behind a load balancer can answer any request.
 * `stateful` keeps sessions in this process's memory; every request in a
 * session must reach the same process.
 */
export type McpSessionMode = 'stateless' | 'stateful';

export const DEFAULT_MCP_SESSION_IDLE_TIMEOUT_MS = 15 * 60 * 1000;

const mcpTransportHeaders = new AsyncLocalStorage<Headers>();

/**
 * Headers of the MCP HTTP request currently being served, if any. Tools use
 * these to forward the caller's credentials and trace context to the routes
 * they call.
 */
export function getMcpTransportHeaders(): Headers | undefined {
  return mcpTransportHeaders.getStore();
}

/**
 * Serves an MCP server over Streamable HTTP from inside the application's own
 * HTTP server, so MCP shares the API's port (and its load balancer listener)
 * instead of needing a second public port.
 *
 * Adapters convert their native request into a fetch `Request` and write the
 * returned `Response` back. `authRequest` is what the `authenticate` hook
 * receives; adapters that are not backed by Node's `http.IncomingMessage` pass
 * a header-compatible stand-in.
 */
export function createMcpHttpHandler(
  mcpServer: FastMCP<Record<string, unknown> | undefined>,
  options?: {
    authenticate?: McpAuthenticate;
    sessions?: McpSessionMode;
    /**
     * Stateful mode only: sessions with no request for this long are closed.
     */
    sessionIdleTimeoutMs?: number;
    /**
     * Number of cluster workers sharing the application port.
     */
    workerCount?: number;
  }
) {
  const mode = options?.sessions ?? 'stateless';
  if (mode === 'stateful' && (options?.workerCount ?? 1) > 1) {
    throw new Error(
      "Stateful MCP sessions live in one process's memory and cannot be shared across workers. Use mcp.sessions: 'stateless' or a single worker."
    );
  }
  const idleTimeoutMs =
    options?.sessionIdleTimeoutMs ?? DEFAULT_MCP_SESSION_IDLE_TIMEOUT_MS;
  const sessions = new Map<string, McpSessionEntry>();

  const jsonRpcError = (
    status: number,
    code: number,
    message: string,
    headers?: Record<string, string>
  ) =>
    new Response(
      JSON.stringify({ jsonrpc: '2.0', error: { code, message }, id: null }),
      {
        status,
        headers: { 'Content-Type': 'application/json', ...headers }
      }
    );

  let sweepTimer: NodeJS.Timeout | undefined;
  if (mode === 'stateful') {
    sweepTimer = setInterval(
      () => {
        const cutoff = Date.now() - idleTimeoutMs;
        for (const [id, entry] of sessions) {
          if (entry.lastSeen < cutoff) {
            sessions.delete(id);
            entry.session.close().catch(() => undefined);
          }
        }
      },
      Math.min(idleTimeoutMs, 60_000)
    );
    sweepTimer.unref();
  }

  async function authenticateRequest(
    authRequest: http.IncomingMessage
  ): Promise<
    { ok: true; auth: Record<string, unknown> | undefined } | { ok: false }
  > {
    if (!options?.authenticate) {
      return { ok: true, auth: undefined };
    }
    try {
      const auth = await options.authenticate(authRequest);
      return auth ? { ok: true, auth } : { ok: false };
    } catch {
      return { ok: false };
    }
  }

  async function handleStateless(
    request: Request,
    auth: Record<string, unknown> | undefined
  ): Promise<Response> {
    // Without sessions there is no server-initiated stream to open or
    // session to delete.
    if (request.method !== 'POST') {
      return jsonRpcError(405, -32000, 'Method not allowed', {
        Allow: 'POST'
      });
    }

    const transport = new WebStandardStreamableHTTPServerTransport({
      sessionIdGenerator: undefined
    });
    const session = await mcpServer.connect(transport, auth);
    let closed = false;
    const closeSession = () => {
      if (!closed) {
        closed = true;
        session.close().catch(() => undefined);
      }
    };

    try {
      const response = await mcpTransportHeaders.run(request.headers, () =>
        transport.handleRequest(request)
      );
      return closeWhenBodyEnds(response, closeSession);
    } catch (error) {
      closeSession();
      throw error;
    }
  }

  async function handleStateful(
    request: Request,
    auth: Record<string, unknown> | undefined
  ): Promise<Response> {
    const sessionId = request.headers.get('mcp-session-id');
    if (sessionId) {
      const entry = sessions.get(sessionId);
      if (!entry) {
        return jsonRpcError(404, -32001, 'Session not found');
      }
      entry.lastSeen = Date.now();
      if (auth) {
        entry.session.updateAuth(auth);
      }
      return mcpTransportHeaders.run(request.headers, () =>
        entry.transport.handleRequest(request)
      );
    }

    // Without a session id the only valid request is `initialize`; the
    // transport rejects anything else, and we drop it if no session started.
    const transport = new WebStandardStreamableHTTPServerTransport({
      sessionIdGenerator: () => randomUUID(),
      onsessioninitialized: (id) => {
        sessions.set(id, { transport, session, lastSeen: Date.now() });
      },
      onsessionclosed: (id) => {
        sessions.delete(id);
      }
    });
    const session = await mcpServer.connect(transport, auth);
    transport.onclose = ((onclose) => () => {
      if (transport.sessionId) {
        sessions.delete(transport.sessionId);
      }
      onclose?.();
    })(transport.onclose);

    const response = await mcpTransportHeaders.run(request.headers, () =>
      transport.handleRequest(request)
    );
    if (!transport.sessionId) {
      await session.close();
    }
    return response;
  }

  async function handle(
    request: Request,
    authRequest: http.IncomingMessage
  ): Promise<Response> {
    const authResult = await authenticateRequest(authRequest);
    if (!authResult.ok) {
      return jsonRpcError(401, -32001, 'Unauthorized', {
        'WWW-Authenticate': 'Bearer'
      });
    }

    return mode === 'stateless'
      ? handleStateless(request, authResult.auth)
      : handleStateful(request, authResult.auth);
  }

  async function close() {
    if (sweepTimer) {
      clearInterval(sweepTimer);
    }
    const entries = [...sessions.values()];
    sessions.clear();
    await Promise.all(entries.map(({ session }) => session.close()));
  }

  return { handle, close, sessions };
}

/**
 * Returns `response` with a body that calls `done` once it has been fully
 * read, failed, or been cancelled by the client.
 */
function closeWhenBodyEnds(response: Response, done: () => void): Response {
  if (!response.body) {
    done();
    return response;
  }

  const reader = response.body.getReader();
  const body = new ReadableStream<Uint8Array>({
    async pull(controller) {
      try {
        const { done: finished, value } = await reader.read();
        if (finished) {
          controller.close();
          done();
        } else {
          controller.enqueue(value);
        }
      } catch (error) {
        controller.error(error);
        done();
      }
    },
    cancel(reason) {
      done();
      return reader.cancel(reason);
    }
  });

  return new Response(body, {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers
  });
}

/**
 * Whether MCP should run on its own listener rather than on the application
 * port. Mirrors the Rust runtime: shared by default, separate only when asked
 * for (`MCP_LISTENER=separate`) or when an explicit MCP port is configured.
 */
export function isSeparateMcpListener(configuredPort?: number) {
  return (
    process.env.MCP_LISTENER === 'separate' ||
    configuredPort != null ||
    Boolean(Number(process.env.MCP_PORT))
  );
}
