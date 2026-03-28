import { createHash } from 'crypto';
import { MetricsDefinition } from '../types/openTelemetryCollector.types';
import { OpenTelemetryCollector } from './openTelemetryCollector';

export type AuditEntry = {
  timestamp: string;
  userId: string | null;
  tenantId: string | null;
  route: string;
  method: string;
  bodyHash: string;
  status: number;
  duration: number;
  redactedFields: string[];
  eventType:
    | 'http'
    | 'ws'
    | 'auth_failure'
    | 'rate_limit'
    | 'rbac_deny'
    | 'super_admin_bypass';
};

export class AuditLogger {
  _otel: OpenTelemetryCollector<MetricsDefinition>;

  constructor(otel: OpenTelemetryCollector<MetricsDefinition>) {
    this._otel = otel;
  }

  append(entry: AuditEntry): void {
    try {
      this._otel.info(
        {
          'log.type': 'audit',
          'audit.timestamp': entry.timestamp,
          'audit.userId': entry.userId ?? '',
          'audit.tenantId': entry.tenantId ?? '',
          'audit.route': entry.route,
          'audit.method': entry.method,
          'audit.bodyHash': entry.bodyHash,
          'audit.status': entry.status,
          'audit.duration': entry.duration,
          'audit.redactedFields': entry.redactedFields.join(','),
          'audit.eventType': entry.eventType,
          _meta: true
        },
        `audit:${entry.eventType} ${entry.method} ${entry.route} ${entry.status}`
      );
    } catch (error: unknown) {
      console.error('Failed to emit audit log via OTEL:', error);
    }
  }

  static hashBody(body: string | Buffer | undefined | null): string {
    if (body === undefined || body === null || body === '') {
      return '';
    }
    return createHash('sha256').update(body).digest('hex');
  }
}
