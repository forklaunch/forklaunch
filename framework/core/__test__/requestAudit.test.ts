import { afterEach, describe, expect, it, vi } from 'vitest';
import { AuditLogger } from '../src/http/telemetry/auditLogger';
import { OpenTelemetryCollector } from '../src/http/telemetry/openTelemetryCollector';
import {
  auditComplianceAccess,
  auditEventType,
  recordRequestAudit
} from '../src/http/telemetry/requestAudit';
import { MetricsDefinition } from '../src/http/types/openTelemetryCollector.types';

function otel() {
  const collector = new OpenTelemetryCollector<MetricsDefinition>(
    'test-request-audit',
    'silent'
  );
  vi.spyOn(collector, 'info');
  return collector;
}

const attributes = (collector: OpenTelemetryCollector<MetricsDefinition>) =>
  vi.mocked(collector.info).mock.calls.map(
    (call) => call[0] as unknown as Record<string, unknown>
  );

afterEach(() => {
  vi.unstubAllEnvs();
});

describe('request audit', () => {
  it('records who, which tenant, the route, status and duration, and only a hash of the body', () => {
    const collector = otel();
    recordRequestAudit(
      collector,
      {
        method: 'PATCH',
        originalPath: '/patients/:id',
        session: { sub: 'user-1', organizationId: 'org-1' },
        body: { ssn: '123-45-6789' }
      },
      200,
      12.4
    );
    const [entry] = attributes(collector);
    expect(entry).toMatchObject({
      'log.type': 'audit',
      'audit.userId': 'user-1',
      'audit.tenantId': 'org-1',
      'audit.route': '/patients/:id',
      'audit.method': 'PATCH',
      'audit.status': 200,
      'audit.duration': 12,
      'audit.eventType': 'http',
      'audit.bodyHash': AuditLogger.hashBody('{"ssn":"123-45-6789"}')
    });
    expect(JSON.stringify(entry)).not.toContain('123-45-6789');
  });

  it('names refused requests by their status', () => {
    expect(auditEventType(401)).toBe('auth_failure');
    expect(auditEventType(403)).toBe('rbac_deny');
    expect(auditEventType(429)).toBe('rate_limit');
    expect(auditEventType(500)).toBe('http');
  });

  it('prefers the active organization and records no user for an anonymous request', () => {
    const collector = otel();
    recordRequestAudit(
      collector,
      {
        method: 'GET',
        path: '/health',
        session: { activeOrganizationId: 'org-2', organizationId: 'org-1' }
      },
      401,
      1
    );
    expect(attributes(collector)[0]).toMatchObject({
      'audit.userId': '',
      'audit.tenantId': 'org-2',
      'audit.route': '/health',
      'audit.eventType': 'auth_failure'
    });
  });

  it('is off when FORKLAUNCH_AUDIT=off', () => {
    vi.stubEnv('FORKLAUNCH_AUDIT', 'off');
    const collector = otel();
    recordRequestAudit(collector, { method: 'GET', path: '/' }, 200, 1);
    expect(collector.info).not.toHaveBeenCalled();
  });
});

describe('compliant field read audit', () => {
  it('records each plaintext read once, with the field and tenant', () => {
    const collector = otel();
    const remove = auditComplianceAccess(collector);
    // A second registration does not double the records.
    auditComplianceAccess(collector);
    const listeners = (globalThis as Record<symbol, unknown>)[
      Symbol.for('forklaunch.compliance.listeners')
    ] as Set<(event: unknown) => void>;
    for (const listener of listeners)
      listener({ field: 'Patient.ssn', level: 'phi', tenantId: 'org-1' });
    remove();
    const records = attributes(collector).filter(
      (a) => a['audit.eventType'] === 'data_access'
    );
    expect(records).toHaveLength(1);
    expect(records[0]).toMatchObject({
      'audit.route': 'Patient.ssn',
      'audit.method': 'READ',
      'audit.tenantId': 'org-1',
      'audit.redactedFields': 'Patient.ssn'
    });
  });
});

describe('sensitive record change audit', () => {
  it('records each change with entity, id, action and sensitive field names', () => {
    const collector = otel();
    const remove = auditComplianceAccess(collector);
    const listeners = (globalThis as Record<symbol, unknown>)[
      Symbol.for('forklaunch.compliance.changeListeners')
    ] as Set<(event: unknown) => void>;
    for (const listener of listeners)
      listener({
        entity: 'Patient',
        id: 'p1',
        action: 'update',
        fields: ['status', 'diagnosis'],
        sensitiveFields: ['diagnosis'],
        tenantId: 'org-1'
      });
    remove();
    const records = attributes(collector).filter(
      (a) => a['audit.eventType'] === 'data_change'
    );
    expect(records).toHaveLength(1);
    expect(records[0]).toMatchObject({
      'audit.route': 'Patient#p1',
      'audit.method': 'UPDATE',
      'audit.tenantId': 'org-1',
      'audit.redactedFields': 'diagnosis'
    });
  });
});
