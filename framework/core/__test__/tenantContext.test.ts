import { describe, expect, it, vi } from 'vitest';
import {
  setTenantContext,
  type TenantContextRequest,
  type TenantContextResponse
} from '../src/http/middleware/request/tenantContext.middleware';
import { TENANT_FILTER_NAME } from '../src/persistence/tenantFilter';

type MockRequest = TenantContextRequest;

function makeReq(overrides: Partial<MockRequest> = {}): MockRequest {
  return {
    contractDetails: { access: 'protected' },
    session: { organizationId: 'org-123' },
    headers: {},
    em: { setFilterParams: vi.fn() },
    ...overrides
  };
}

function makeRes() {
  const res: TenantContextResponse = {
    status: vi.fn<(code: number) => TenantContextResponse>(),
    send: vi.fn<(body?: unknown) => TenantContextResponse>()
  };
  vi.mocked(res.status).mockReturnValue(res);
  return res;
}

function callMiddleware(
  req: MockRequest,
  res: TenantContextResponse,
  next: () => void
) {
  return setTenantContext(req, res, next);
}

describe('setTenantContext', () => {
  it('sets tenant filter params for protected routes', async () => {
    const req = makeReq();
    const res = makeRes();
    const next = vi.fn();

    await callMiddleware(req, res, next);

    expect(req.em?.setFilterParams).toHaveBeenCalledWith(TENANT_FILTER_NAME, {
      tenantId: 'org-123'
    });
    expect(next).toHaveBeenCalled();
  });

  it('reads activeOrganizationId from better-auth session', async () => {
    const req = makeReq({
      session: { activeOrganizationId: 'active-org-456' }
    });
    const res = makeRes();
    const next = vi.fn();

    await callMiddleware(req, res, next);

    expect(req.em?.setFilterParams).toHaveBeenCalledWith(TENANT_FILTER_NAME, {
      tenantId: 'active-org-456'
    });
  });

  it('skips tenant context for public routes', async () => {
    const req = makeReq({
      contractDetails: { access: 'public' },
      session: undefined
    });
    const res = makeRes();
    const next = vi.fn();

    await callMiddleware(req, res, next);

    expect(req.em?.setFilterParams).not.toHaveBeenCalled();
    expect(next).toHaveBeenCalled();
  });

  it('returns 403 for protected route without tenant ID', async () => {
    const req = makeReq({
      contractDetails: { access: 'protected' },
      session: {}
    });
    const res = makeRes();
    const next = vi.fn();

    await callMiddleware(req, res, next);

    expect(res.status).toHaveBeenCalledWith(403);
    expect(res.send).toHaveBeenCalled();
    expect(next).not.toHaveBeenCalled();
  });

  it('returns 403 for authenticated route without tenant ID', async () => {
    const req = makeReq({
      contractDetails: { access: 'authenticated' },
      session: {}
    });
    const res = makeRes();
    const next = vi.fn();

    await callMiddleware(req, res, next);

    expect(res.status).toHaveBeenCalledWith(403);
    expect(next).not.toHaveBeenCalled();
  });

  it('reads X-Tenant-Id header for internal routes', async () => {
    const req = makeReq({
      contractDetails: { access: 'internal' },
      session: undefined,
      headers: { 'x-tenant-id': 'header-org-789' }
    });
    const res = makeRes();
    const next = vi.fn();

    await callMiddleware(req, res, next);

    expect(req.em?.setFilterParams).toHaveBeenCalledWith(TENANT_FILTER_NAME, {
      tenantId: 'header-org-789'
    });
    expect(next).toHaveBeenCalled();
  });

  it('skips EM filter when no EM on request', async () => {
    const req = makeReq({ em: undefined });
    const res = makeRes();
    const next = vi.fn();

    await callMiddleware(req, res, next);

    // Should not throw, just skip EM setup
    expect(next).toHaveBeenCalled();
  });

  it('skips when access field is missing (backward compat)', async () => {
    const req = makeReq({
      contractDetails: {}
    });
    const res = makeRes();
    const next = vi.fn();

    await callMiddleware(req, res, next);

    expect(req.em?.setFilterParams).not.toHaveBeenCalled();
    expect(next).toHaveBeenCalled();
  });
});
