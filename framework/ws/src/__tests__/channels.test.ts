import { describe, expect, it, vi, beforeEach } from 'vitest';
import { WSChannelManager } from '../channels';
import type { WSSession } from '../wsSession';
import type { WebSocket } from 'ws';

function makeSession(overrides: Partial<WSSession> = {}): WSSession {
  return {
    userId: 'user-1',
    tenantId: 'tenant-A',
    roles: ['admin'],
    permissions: ['orders:read', 'orders:write'],
    ...overrides
  };
}

function makeWs(
  readyState = 1 /* OPEN */
): WebSocket & { send: ReturnType<typeof vi.fn> } {
  return {
    readyState,
    send: vi.fn()
  } as WebSocket & { send: ReturnType<typeof vi.fn> };
}

describe('WSChannelManager', () => {
  let channels: WSChannelManager;

  beforeEach(() => {
    channels = new WSChannelManager();
  });

  it('subscribes a connection to a channel', () => {
    const ws = makeWs();
    const session = makeSession();

    channels.subscribe(ws, session, 'orders');

    expect(channels.getConnectionCount('orders')).toBe(1);
  });

  it('unsubscribes a connection from a channel', () => {
    const ws = makeWs();
    channels.subscribe(ws, makeSession(), 'orders');

    channels.unsubscribe(ws, 'orders');

    expect(channels.getConnectionCount('orders')).toBe(0);
  });

  it('removes a connection from all channels on disconnect', () => {
    const ws = makeWs();
    const session = makeSession();
    channels.subscribe(ws, session, 'orders');
    channels.subscribe(ws, session, 'notifications');

    channels.removeConnection(ws);

    expect(channels.getConnectionCount('orders')).toBe(0);
    expect(channels.getConnectionCount('notifications')).toBe(0);
  });

  describe('broadcast', () => {
    it('delivers event to same-tenant connections', () => {
      const ws1 = makeWs();
      const ws2 = makeWs();
      channels.subscribe(ws1, makeSession({ tenantId: 'tenant-A' }), 'orders');
      channels.subscribe(ws2, makeSession({ tenantId: 'tenant-A' }), 'orders');

      const count = channels.broadcast('orders', { type: 'new' }, 'tenant-A');

      expect(count).toBe(2);
      expect(ws1.send).toHaveBeenCalledOnce();
      expect(ws2.send).toHaveBeenCalledOnce();
    });

    it('does NOT deliver to connections from other tenants', () => {
      const ws1 = makeWs();
      const ws2 = makeWs();
      channels.subscribe(ws1, makeSession({ tenantId: 'tenant-A' }), 'orders');
      channels.subscribe(ws2, makeSession({ tenantId: 'tenant-B' }), 'orders');

      const count = channels.broadcast('orders', { type: 'new' }, 'tenant-A');

      expect(count).toBe(1);
      expect(ws1.send).toHaveBeenCalledOnce();
      expect(ws2.send).not.toHaveBeenCalled();
    });

    it('does NOT deliver to connections without required permissions', () => {
      const ws1 = makeWs();
      const ws2 = makeWs();
      channels.subscribe(
        ws1,
        makeSession({ permissions: ['orders:read'] }),
        'orders',
        ['orders:read']
      );
      channels.subscribe(ws2, makeSession({ permissions: [] }), 'orders', [
        'orders:read'
      ]);

      const count = channels.broadcast('orders', { type: 'new' }, 'tenant-A');

      expect(count).toBe(1);
      expect(ws1.send).toHaveBeenCalledOnce();
      expect(ws2.send).not.toHaveBeenCalled();
    });

    it('skips connections that are not OPEN', () => {
      const ws = makeWs(3 /* CLOSED */);
      channels.subscribe(ws, makeSession(), 'orders');

      const count = channels.broadcast('orders', { type: 'new' }, 'tenant-A');

      expect(count).toBe(0);
      expect(ws.send).not.toHaveBeenCalled();
    });

    it('returns 0 for non-existent channel', () => {
      expect(channels.broadcast('nonexistent', {}, 'tenant-A')).toBe(0);
    });

    it('delivers to all if no required permissions', () => {
      const ws1 = makeWs();
      const ws2 = makeWs();
      channels.subscribe(ws1, makeSession({ permissions: [] }), 'global');
      channels.subscribe(ws2, makeSession({ permissions: [] }), 'global');

      const count = channels.broadcast('global', { msg: 'hi' }, 'tenant-A');

      expect(count).toBe(2);
    });
  });

  describe('getConnectionCount', () => {
    it('returns count filtered by tenant', () => {
      channels.subscribe(makeWs(), makeSession({ tenantId: 'A' }), 'ch');
      channels.subscribe(makeWs(), makeSession({ tenantId: 'A' }), 'ch');
      channels.subscribe(makeWs(), makeSession({ tenantId: 'B' }), 'ch');

      expect(channels.getConnectionCount('ch', 'A')).toBe(2);
      expect(channels.getConnectionCount('ch', 'B')).toBe(1);
      expect(channels.getConnectionCount('ch')).toBe(3);
    });
  });
});
