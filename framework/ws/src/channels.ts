import type { AuditLogger } from '@forklaunch/core/http';
import type { WebSocket } from 'ws';
import type { WSSession } from './wsSession';

/**
 * A tracked WebSocket connection with its authenticated session.
 */
interface TrackedConnection {
  ws: WebSocket;
  session: WSSession;
}

/**
 * Channel subscription with required permissions for receiving events.
 */
interface ChannelSubscription {
  requiredPermissions: string[];
  connections: Map<WebSocket, TrackedConnection>;
}

/**
 * Manages tenant-scoped WebSocket channels with per-broadcast permission checks.
 *
 * Channels are implicitly tenant-scoped: a broadcast from tenant A never
 * reaches connections belonging to tenant B. Within a tenant, each recipient's
 * permissions are checked against the channel's required permissions before
 * delivery.
 *
 * @example
 * ```typescript
 * const channels = new WSChannelManager();
 *
 * // Subscribe a connection to a channel
 * channels.subscribe(ws, session, 'orders', ['orders:read']);
 *
 * // Broadcast to channel — only reaches same-tenant connections with permission
 * channels.broadcast('orders', { type: 'order-created', orderId: '123' }, session.tenantId);
 * ```
 */
export class WSChannelManager {
  private readonly channels = new Map<string, ChannelSubscription>();
  private readonly auditLogger?: AuditLogger;

  constructor(auditLogger?: AuditLogger) {
    this.auditLogger = auditLogger;
  }

  /**
   * Subscribe a connection to a channel.
   *
   * @param ws - The WebSocket connection
   * @param session - The authenticated session for this connection
   * @param channel - The channel name to subscribe to
   * @param requiredPermissions - Permissions needed to receive events on this channel
   */
  subscribe(
    ws: WebSocket,
    session: WSSession,
    channel: string,
    requiredPermissions: string[] = []
  ): void {
    let sub = this.channels.get(channel);
    if (!sub) {
      sub = { requiredPermissions, connections: new Map() };
      this.channels.set(channel, sub);
    } else if (!arraysEqual(sub.requiredPermissions, requiredPermissions)) {
      throw new Error(
        `Channel '${channel}' already exists with different requiredPermissions: ` +
          `[${sub.requiredPermissions.join(', ')}] vs [${requiredPermissions.join(', ')}]`
      );
    }
    sub.connections.set(ws, { ws, session });
  }

  /**
   * Unsubscribe a connection from a channel.
   */
  unsubscribe(ws: WebSocket, channel: string): void {
    const sub = this.channels.get(channel);
    if (!sub) return;
    sub.connections.delete(ws);
    if (sub.connections.size === 0) {
      this.channels.delete(channel);
    }
  }

  /**
   * Remove a connection from ALL channels (on disconnect).
   */
  removeConnection(ws: WebSocket): void {
    for (const [channelName, sub] of this.channels) {
      sub.connections.delete(ws);
      if (sub.connections.size === 0) {
        this.channels.delete(channelName);
      }
    }
  }

  /**
   * Broadcast an event to all authorized connections in a channel.
   *
   * Tenant isolation: only connections with the same `tenantId` as the sender
   * receive the event. Within the tenant, each connection's permissions are
   * checked against the channel's `requiredPermissions`.
   *
   * @param channel - The channel name
   * @param event - The event data to broadcast
   * @param senderTenantId - The tenant ID of the sender
   * @returns The number of connections that received the event
   */
  broadcast(channel: string, event: unknown, senderTenantId: string): number {
    const sub = this.channels.get(channel);
    if (!sub) return 0;

    let delivered = 0;
    const payload = typeof event === 'string' ? event : JSON.stringify(event);

    for (const [, tracked] of sub.connections) {
      // Tenant isolation: skip connections from other tenants
      if (tracked.session.tenantId !== senderTenantId) {
        continue;
      }

      // Permission check: skip connections without required permissions
      if (!hasRequiredPermissions(tracked.session, sub.requiredPermissions)) {
        continue;
      }

      // Deliver if connection is open
      if (tracked.ws.readyState === 1 /* WebSocket.OPEN */) {
        tracked.ws.send(payload);
        delivered++;
      }
    }

    // Audit log the broadcast
    if (this.auditLogger) {
      this.auditLogger.append({
        timestamp: new Date().toISOString(),
        userId: null,
        tenantId: senderTenantId,
        route: `ws:channel:${channel}`,
        method: 'BROADCAST',
        bodyHash: '',
        status: 200,
        duration: 0,
        redactedFields: [],
        eventType: 'ws'
      });
    }

    return delivered;
  }

  /**
   * Get the number of connections in a channel (optionally filtered by tenant).
   */
  getConnectionCount(channel: string, tenantId?: string): number {
    const sub = this.channels.get(channel);
    if (!sub) return 0;

    if (!tenantId) return sub.connections.size;

    let count = 0;
    for (const [, tracked] of sub.connections) {
      if (tracked.session.tenantId === tenantId) count++;
    }
    return count;
  }
}

/**
 * Check if a session has all required permissions.
 */
function arraysEqual(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  const sorted1 = [...a].sort();
  const sorted2 = [...b].sort();
  return sorted1.every((v, i) => v === sorted2[i]);
}

function hasRequiredPermissions(
  session: WSSession,
  requiredPermissions: string[]
): boolean {
  if (requiredPermissions.length === 0) return true;
  return requiredPermissions.every((perm) =>
    session.permissions.includes(perm)
  );
}
