import type { AnySchemaValidator } from '@forklaunch/validator';
import type { EventSchema } from '@forklaunch/core/ws';
import type { IncomingMessage } from 'http';
import type { Duplex } from 'stream';
import { WebSocket } from 'ws';
import { WSChannelManager } from './channels';
import { ForklaunchWebSocketServer } from './webSocketServer';
import type { WSSecurityOptions, WSSession } from './wsSession';
import { WSCloseCodes } from './wsSession';

const DEFAULT_REVALIDATE_MS = 300_000; // 5 minutes

/**
 * Map to track session and re-validation timers per connection.
 */
interface ConnectionMeta {
  session: WSSession;
  revalidateTimer?: ReturnType<typeof setInterval>;
  request: IncomingMessage;
}

/**
 * Secure ForkLaunch WebSocket server with built-in:
 * - Authentication at handshake (reject before any data is transmitted)
 * - Periodic session re-validation (configurable interval)
 * - Tenant-scoped channel system with per-broadcast permission checks
 * - Audit logging for all WS events
 *
 * @example
 * ```typescript
 * const wss = new SecureForklaunchWebSocketServer(
 *   validator,
 *   schemas,
 *   { noServer: true },
 *   {
 *     authenticate: async (req) => {
 *       const token = new URL(req.url!, 'ws://localhost').searchParams.get('token');
 *       if (!token) return null;
 *       const payload = await verifyJwt(token);
 *       return {
 *         userId: payload.sub,
 *         tenantId: payload.organizationId,
 *         roles: payload.roles,
 *         permissions: payload.permissions
 *       };
 *     },
 *     revalidateIntervalMs: 300_000,
 *     auditLogger
 *   }
 * );
 *
 * httpServer.on('upgrade', (req, socket, head) => {
 *   wss.handleSecureUpgrade(req, socket, head);
 * });
 * ```
 */
export class SecureForklaunchWebSocketServer<
  SV extends AnySchemaValidator,
  const ES extends EventSchema<SV>
> extends ForklaunchWebSocketServer<SV, ES> {
  private readonly securityOptions: WSSecurityOptions;
  private readonly connectionMeta = new Map<WebSocket, ConnectionMeta>();
  readonly channels: WSChannelManager;

  constructor(
    schemaValidator: SV,
    eventSchemas: ES,
    options: ConstructorParameters<typeof ForklaunchWebSocketServer>[2],
    security: WSSecurityOptions,
    callback?: () => void
  ) {
    super(schemaValidator, eventSchemas, options, callback);
    this.securityOptions = security;
    this.channels = new WSChannelManager(security.auditLogger);
  }

  /**
   * Handle an HTTP upgrade request with authentication.
   * If authentication fails, the socket is destroyed before any WS data is sent.
   * If authentication succeeds, the connection is established and tagged with the session.
   */
  async handleSecureUpgrade(
    request: IncomingMessage,
    socket: Duplex,
    head: Buffer
  ): Promise<void> {
    const session = await this.securityOptions.authenticate(request);

    if (!session) {
      // Reject — destroy socket before any WS frames
      this.auditAuthFailure(request);
      socket.write('HTTP/1.1 401 Unauthorized\r\n\r\n');
      socket.destroy();
      return;
    }

    // Authenticated — complete the upgrade
    this.handleUpgrade(request, socket, head, (ws) => {
      this.onAuthenticated(ws, session, request);
      this.emit('connection', ws, request);
    });
  }

  /**
   * Get the session for a connected WebSocket.
   */
  getSession(ws: WebSocket): WSSession | undefined {
    return this.connectionMeta.get(ws)?.session;
  }

  /**
   * Called after successful authentication and upgrade.
   */
  private onAuthenticated(
    ws: WebSocket,
    session: WSSession,
    request: IncomingMessage
  ): void {
    // Store session on the connection
    const meta: ConnectionMeta = { session, request };
    this.connectionMeta.set(ws, meta);

    // Audit log the connection
    this.auditEvent('ws', session, 'CONNECT', request);

    // Set up periodic re-validation
    const intervalMs =
      this.securityOptions.revalidateIntervalMs ?? DEFAULT_REVALIDATE_MS;
    if (intervalMs > 0) {
      meta.revalidateTimer = setInterval(() => {
        this.revalidateSession(ws, meta);
      }, intervalMs);
    }

    // Clean up on close
    ws.on('close', () => {
      this.auditEvent('ws', session, 'DISCONNECT', request);
      if (meta.revalidateTimer) {
        clearInterval(meta.revalidateTimer);
      }
      this.channels.removeConnection(ws);
      this.connectionMeta.delete(ws);
    });

    // Audit log messages
    ws.on('message', () => {
      this.auditEvent('ws', session, 'MESSAGE', request);
    });
  }

  /**
   * Periodically re-validates the session. If the session is no longer
   * valid, the connection is closed with code 4001.
   */
  private async revalidateSession(
    ws: WebSocket,
    meta: ConnectionMeta
  ): Promise<void> {
    try {
      const session = await this.securityOptions.authenticate(meta.request);
      if (!session) {
        this.auditEvent(
          'auth_failure',
          meta.session,
          'REVALIDATE_FAIL',
          meta.request
        );
        ws.close(WSCloseCodes.UNAUTHORIZED, 'Session expired');
        return;
      }
      // Update session in case roles/permissions changed
      meta.session = session;
    } catch {
      // Auth function failed — close connection
      ws.close(WSCloseCodes.UNAUTHORIZED, 'Session validation failed');
    }
  }

  /**
   * Emit an audit log entry for a WS event.
   */
  private auditEvent(
    eventType: 'ws' | 'auth_failure',
    session: WSSession,
    method: string,
    request: IncomingMessage
  ): void {
    const logger = this.securityOptions.auditLogger;
    if (!logger) return;

    logger.append({
      timestamp: new Date().toISOString(),
      userId: session.userId,
      tenantId: session.tenantId,
      route: `ws:${request.url ?? '/'}`,
      method,
      bodyHash: '',
      status: eventType === 'auth_failure' ? 401 : 200,
      duration: 0,
      redactedFields: [],
      eventType
    });
  }

  /**
   * Audit log a failed authentication attempt (before connection established).
   */
  private auditAuthFailure(request: IncomingMessage): void {
    const logger = this.securityOptions.auditLogger;
    if (!logger) return;

    logger.append({
      timestamp: new Date().toISOString(),
      userId: null,
      tenantId: null,
      route: `ws:${request.url ?? '/'}`,
      method: 'UPGRADE',
      bodyHash: '',
      status: 401,
      duration: 0,
      redactedFields: [],
      eventType: 'auth_failure'
    });
  }
}
