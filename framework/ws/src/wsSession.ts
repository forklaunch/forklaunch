import type { IncomingMessage } from 'http';
import type { AuditLogger } from '@forklaunch/core/http';

/**
 * Authenticated WebSocket session — set on each connection after
 * successful handshake authentication.
 */
export interface WSSession {
  userId: string;
  tenantId: string;
  roles: string[];
  permissions: string[];
}

/**
 * Security options for the ForkLaunch WebSocket server.
 */
export interface WSSecurityOptions {
  /**
   * Authenticate an incoming WebSocket upgrade request.
   * Return a session if valid, or `null` to reject the connection.
   */
  authenticate: (req: IncomingMessage) => Promise<WSSession | null>;

  /**
   * Interval in milliseconds between session re-validation checks.
   * Set to 0 to disable periodic re-validation.
   * @default 300_000 (5 minutes)
   */
  revalidateIntervalMs?: number;

  /**
   * Audit logger instance for recording WS events.
   * If not provided, WS events are not audit-logged.
   */
  auditLogger?: AuditLogger;
}

/**
 * WebSocket close codes for security events.
 */
export const WSCloseCodes = {
  /** Authentication failed at handshake or re-validation. */
  UNAUTHORIZED: 4001,
  /** Tenant mismatch or permission denied. */
  FORBIDDEN: 4003
} as const;
