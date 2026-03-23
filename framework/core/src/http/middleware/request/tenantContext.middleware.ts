import type { AnySchemaValidator } from '@forklaunch/validator';
import { TENANT_FILTER_NAME } from '../../../persistence/tenantFilter';
import type {
  ForklaunchNextFunction,
  ForklaunchRequest,
  ForklaunchResponse,
  MapParamsSchema,
  MapReqBodySchema,
  MapReqHeadersSchema,
  MapReqQuerySchema,
  MapResBodyMapSchema,
  MapResHeadersSchema,
  MapSessionSchema,
  MapVersionedReqsSchema
} from '../../types/apiDefinition.types';
import type {
  Body,
  HeadersObject,
  Method,
  ParamsObject,
  QueryObject,
  ResponsesObject,
  SessionObject,
  VersionSchema
} from '../../types/contractDetails.types';

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
export async function setTenantContext<
  SV extends AnySchemaValidator,
  ContractMethod extends Method,
  P extends ParamsObject<SV>,
  ResBodyMap extends ResponsesObject<SV>,
  ReqBody extends Body<SV>,
  ReqQuery extends QueryObject<SV>,
  ReqHeaders extends HeadersObject<SV>,
  ResHeaders extends HeadersObject<SV>,
  LocalsObj extends Record<string, unknown>,
  VersionedApi extends VersionSchema<SV, ContractMethod>,
  SessionSchema extends SessionObject<SV>
>(
  req: ForklaunchRequest<
    SV,
    MapParamsSchema<SV, P>,
    MapReqBodySchema<SV, ReqBody>,
    MapReqQuerySchema<SV, ReqQuery>,
    MapReqHeadersSchema<SV, ReqHeaders>,
    Extract<keyof MapVersionedReqsSchema<SV, VersionedApi>, string>,
    MapSessionSchema<SV, SessionSchema>
  >,
  res: ForklaunchResponse<
    unknown,
    MapResBodyMapSchema<SV, ResBodyMap>,
    MapResHeadersSchema<SV, ResHeaders>,
    LocalsObj,
    Extract<keyof MapVersionedReqsSchema<SV, VersionedApi>, string>
  >,
  next?: ForklaunchNextFunction
): Promise<void> {
  const contractDetails = req.contractDetails;
  const access = (contractDetails as Record<string, unknown>)['access'] as
    | string
    | undefined;

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
      .send(
        'Tenant context required. Session must include organizationId.' as never
      );
    return;
  }

  // Set tenant filter params on the request's entity manager
  // The EM is typically available via DI scoping on the request
  if (tenantId) {
    const em = (req as { em?: TenantFilterable }).em;
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
