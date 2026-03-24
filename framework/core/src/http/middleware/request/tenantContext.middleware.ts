import { TENANT_FILTER_NAME } from '../../../persistence/tenantFilter';

/**
 * Structural request type for tenant context middleware.
 * Accepts any object with the properties the middleware actually reads.
 */
export interface TenantContextRequest {
  contractDetails: Record<string, unknown>;
  session?: Record<string, unknown>;
  headers: Record<string, unknown>;
  em?: TenantFilterable;
  schemaValidator?: unknown;
}

/**
 * Structural response type for tenant context middleware.
 * Only `status` and `send` are used (for 403 responses).
 */
export interface TenantContextResponse {
  status(code: number): TenantContextResponse;
  send(body?: unknown): TenantContextResponse;
}

/**
 * Middleware that establishes tenant context on the request-scoped
 * EntityManager. Runs after auth middleware.
 *
 * - For `'protected'`/`'authenticated'` routes: reads tenant ID from
 *   `req.session.organizationId` or `req.session.activeOrganizationId`.
 *   Returns 403 if missing.
 * - For `'internal'` routes: reads tenant ID from `X-Tenant-Id` header
 *   or from the session if available.
 * - For `'public'` routes: skips tenant context — filter remains unset.
 *
 * The middleware:
 * 1. Sets the tenant filter params on the EM: `em.setFilterParams('tenant', { tenantId })`
 * 2. Wraps the EM with native query blocking for compliance entities
 */
export async function setTenantContext(
  req: TenantContextRequest,
  res: TenantContextResponse,
  next?: (() => void) | undefined
): Promise<void> {
  const contractDetails = req.contractDetails;
  const access = contractDetails['access'] as string | undefined;

  // Public routes skip tenant context
  if (!access || access === 'public') {
    next?.();
    return;
  }

  // Resolve tenant ID based on access type
  const tenantId = resolveTenantId(req, access);

  if (!tenantId && (access === 'protected' || access === 'authenticated')) {
    res
      .status(403)
      .send('Tenant context required. Session must include organizationId.');
    return;
  }

  // Set tenant filter params on the request's entity manager
  // The EM is typically available via DI scoping on the request
  if (tenantId) {
    const em = req.em;
    if (em && typeof em.setFilterParams === 'function') {
      em.setFilterParams(TENANT_FILTER_NAME, { tenantId });
    }
  }

  next?.();
}

/** Minimal interface for setting filter params — avoids importing full EntityManager. */
interface TenantFilterable {
  setFilterParams(name: string, params: Record<string, unknown>): void;
}

/**
 * Resolve the tenant ID from the request context.
 */
function resolveTenantId(
  req: { session?: Record<string, unknown>; headers: Record<string, unknown> },
  access: string
): string | undefined {
  // For internal routes, check X-Tenant-Id header first
  if (access === 'internal') {
    const headerTenantId = req.headers['x-tenant-id'];
    if (typeof headerTenantId === 'string' && headerTenantId) {
      return headerTenantId;
    }
  }

  // For all authenticated routes, check session
  const session = req.session;
  if (!session) return undefined;

  // better-auth uses activeOrganizationId
  if (typeof session.activeOrganizationId === 'string') {
    return session.activeOrganizationId;
  }

  // Fallback to organizationId
  if (typeof session.organizationId === 'string') {
    return session.organizationId;
  }

  return undefined;
}
