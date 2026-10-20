import { getEnvVar } from '@forklaunch/common';
import { onComplianceChange } from '../../persistence/complianceChanges';
import { onComplianceAccess } from '../../persistence/compliantField';
import { MetricsDefinition } from '../types/openTelemetryCollector.types';
import { AuditEntry, AuditLogger } from './auditLogger';
import { OpenTelemetryCollector } from './openTelemetryCollector';

/**
 * Default audit trail for every app built on ForkLaunch: one audit record per
 * request (who, tenant, method, route, status, duration and a hash of the
 * body, never its content) and one per read of a compliant field's plaintext.
 * Records carry `log.type: audit` so the platform can keep them apart from
 * application logs. On unless FORKLAUNCH_AUDIT=off.
 */
export function auditEnabled(): boolean {
  return getEnvVar('FORKLAUNCH_AUDIT') !== 'off';
}

const loggers = new WeakMap<object, AuditLogger>();
function loggerFor(otel: OpenTelemetryCollector<MetricsDefinition>) {
  let logger = loggers.get(otel);
  if (!logger) {
    logger = new AuditLogger(otel);
    loggers.set(otel, logger);
  }
  return logger;
}

/** The audit event a response status stands for. */
export function auditEventType(status: number): AuditEntry['eventType'] {
  if (status === 401) return 'auth_failure';
  if (status === 403) return 'rbac_deny';
  if (status === 429) return 'rate_limit';
  return 'http';
}

const text = (value: unknown) =>
  typeof value === 'string' && value ? value : null;

function bodyText(body: unknown): string | undefined {
  if (body == null || body === '') return undefined;
  if (typeof body === 'string' || Buffer.isBuffer(body)) return String(body);
  try {
    return JSON.stringify(body);
  } catch {
    return undefined;
  }
}

/** Append the audit record for one finished request. */
export function recordRequestAudit(
  otel: OpenTelemetryCollector<MetricsDefinition> | undefined,
  req: {
    method: string;
    originalPath?: string;
    path?: string;
    session?: unknown;
    body?: unknown;
  },
  status: number,
  durationMs: number
): void {
  if (!otel || !auditEnabled()) return;
  const session =
    req.session && typeof req.session === 'object'
      ? (req.session as Record<string, unknown>)
      : undefined;
  loggerFor(otel).append({
    timestamp: new Date().toISOString(),
    userId: text(session?.sub) ?? text(session?.userId),
    tenantId:
      text(session?.activeOrganizationId) ?? text(session?.organizationId),
    route: req.originalPath ?? req.path ?? 'unknown',
    method: req.method,
    bodyHash: AuditLogger.hashBody(bodyText(req.body)),
    status,
    duration: Math.max(0, Math.round(durationMs)),
    redactedFields: [],
    eventType: auditEventType(status)
  });
}

let accessAudit: (() => void) | undefined;

/**
 * Every `.deanon` read of a compliant field becomes an audit record (field,
 * level, tenant), and every create/update/delete of a record holding sensitive
 * fields a `data_change` record (entity#id, action, sensitive field names). It is emitted inside the request's trace, which ties it to
 * the request's own audit record and so to the user. Registered once per
 * process; returns a function that removes it.
 */
export function auditComplianceAccess(
  otel: OpenTelemetryCollector<MetricsDefinition>
): () => void {
  if (!accessAudit && auditEnabled()) {
    const logger = loggerFor(otel);
    const remove = onComplianceAccess((event) =>
      logger.append({
        timestamp: new Date().toISOString(),
        userId: null,
        tenantId: event.tenantId || null,
        route: event.field,
        method: 'READ',
        bodyHash: '',
        status: 0,
        duration: 0,
        redactedFields: [event.field],
        eventType: 'data_access'
      })
    );
    const removeChanges = onComplianceChange((event) =>
      logger.append({
        timestamp: new Date().toISOString(),
        userId: null,
        tenantId: event.tenantId,
        route: `${event.entity}#${event.id}`,
        method: event.action.toUpperCase(),
        bodyHash: '',
        status: 0,
        duration: 0,
        redactedFields: event.sensitiveFields,
        eventType: 'data_change'
      })
    );
    accessAudit = () => {
      remove();
      removeChanges();
      accessAudit = undefined;
    };
  }
  return accessAudit ?? (() => undefined);
}
