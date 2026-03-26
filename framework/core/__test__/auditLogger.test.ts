import { createHash } from 'crypto';
import { describe, expect, it, vi } from 'vitest';
import { AuditEntry, AuditLogger } from '../src/http/telemetry/auditLogger';
import { OpenTelemetryCollector } from '../src/http/telemetry/openTelemetryCollector';
import { MetricsDefinition } from '../src/http/types/openTelemetryCollector.types';

function createMockOtel(): OpenTelemetryCollector<MetricsDefinition> {
  const otel = new OpenTelemetryCollector('test-audit', 'silent');
  vi.spyOn(otel, 'info');
  return otel;
}

function createSampleEntry(overrides?: Partial<AuditEntry>): AuditEntry {
  return {
    timestamp: '2026-03-23T00:00:00.000Z',
    userId: 'user-123',
    tenantId: 'tenant-456',
    route: '/api/v1/resource',
    method: 'POST',
    bodyHash: AuditLogger.hashBody('{"key":"value"}'),
    status: 200,
    duration: 42,
    redactedFields: ['ssn', 'creditCard'],
    eventType: 'http',
    ...overrides
  };
}

describe('AuditLogger', () => {
  describe('append', () => {
    it('calls the OTEL collector info method', () => {
      const otel = createMockOtel();
      const auditLogger = new AuditLogger(otel);
      const entry = createSampleEntry();

      auditLogger.append(entry);

      expect(otel.info).toHaveBeenCalledOnce();
    });

    it('passes audit attributes including log.type', () => {
      const otel = createMockOtel();
      const auditLogger = new AuditLogger(otel);
      const entry = createSampleEntry();

      auditLogger.append(entry);

      expect(otel.info).toHaveBeenCalledWith(
        expect.objectContaining({
          'log.type': 'audit',
          'audit.timestamp': entry.timestamp,
          'audit.userId': entry.userId,
          'audit.tenantId': entry.tenantId,
          'audit.route': entry.route,
          'audit.method': entry.method,
          'audit.bodyHash': entry.bodyHash,
          'audit.status': entry.status,
          'audit.duration': entry.duration,
          'audit.redactedFields': 'ssn,creditCard',
          'audit.eventType': entry.eventType
        }),
        expect.stringContaining('audit:http POST /api/v1/resource 200')
      );
    });

    it('includes all required fields in the entry', () => {
      const entry = createSampleEntry();
      const requiredKeys: (keyof AuditEntry)[] = [
        'timestamp',
        'userId',
        'tenantId',
        'route',
        'method',
        'bodyHash',
        'status',
        'duration',
        'redactedFields',
        'eventType'
      ];
      for (const key of requiredKeys) {
        expect(entry).toHaveProperty(key);
      }
    });

    it('handles null userId and tenantId', () => {
      const otel = createMockOtel();
      const auditLogger = new AuditLogger(otel);
      const entry = createSampleEntry({ userId: null, tenantId: null });

      auditLogger.append(entry);

      expect(otel.info).toHaveBeenCalledWith(
        expect.objectContaining({
          'audit.userId': '',
          'audit.tenantId': ''
        }),
        expect.any(String)
      );
    });

    it('does not throw when OTEL emission fails', () => {
      const otel = createMockOtel();
      vi.spyOn(otel, 'info').mockImplementation(() => {
        throw new Error('OTEL transport failure');
      });
      const auditLogger = new AuditLogger(otel);
      const entry = createSampleEntry();

      expect(() => auditLogger.append(entry)).not.toThrow();
    });
  });

  describe('hashBody', () => {
    it('produces consistent SHA-256 hashes for the same input', () => {
      const body = '{"key":"value"}';
      const hash1 = AuditLogger.hashBody(body);
      const hash2 = AuditLogger.hashBody(body);
      expect(hash1).toBe(hash2);
    });

    it('produces correct SHA-256 hex hash', () => {
      const body = 'hello world';
      const expected = createHash('sha256').update(body).digest('hex');
      expect(AuditLogger.hashBody(body)).toBe(expected);
    });

    it('returns empty string for null', () => {
      expect(AuditLogger.hashBody(null)).toBe('');
    });

    it('returns empty string for undefined', () => {
      expect(AuditLogger.hashBody(undefined)).toBe('');
    });

    it('returns empty string for empty string', () => {
      expect(AuditLogger.hashBody('')).toBe('');
    });

    it('handles Buffer input', () => {
      const buf = Buffer.from('binary data');
      const expected = createHash('sha256').update(buf).digest('hex');
      expect(AuditLogger.hashBody(buf)).toBe(expected);
    });
  });
});
