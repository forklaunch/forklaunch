import { handlers, schemaValidator, string } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const resolverFactory = ci.scopedResolver(tokens.CodeSetProviderResolver);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

// admin:manage_codesets — the slug plan §3 originally sketched for this
// controller, still fits: only admins need visibility into which code-set
// provider is active and can look codes up directly against it.
const MANAGE_CODESETS_PERMISSIONS = new Set(['admin:manage_codesets']);

// Protected + JWT, not internal/HMAC — the per-organization feature gate
// (§5, plan §12 item 11) needs to know *which organization* is asking, so
// it can check that org's own CodeSetLicense rather than a global,
// process-wide setting. There's no way to know that from a service-to-
// service HMAC call the way there is from an authenticated session.
export const describeCodeSet = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'Describe Code Set',
    access: 'protected',
    summary:
      'Reports which procedure code-set provider is active for the caller\'s organization (mock vs. licensed CPT)',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      sessionSchema: {
        organizationId: string
      },
      allowedPermissions: MANAGE_CODESETS_PERMISSIONS
    },
    responses: {
      200: {
        codeSetType: string,
        licensed: schemaValidator.boolean
      }
    }
  },
  async (req, res) => {
    const organizationId = req.session?.organizationId;
    const codeSetProvider = await resolverFactory({ context: { tenantId: organizationId } }).resolve(organizationId);
    const descriptor = codeSetProvider.describe();
    openTelemetryCollector.debug('Describing active code set', {
      organizationId,
      ...descriptor
    });
    res.status(200).json(descriptor);
  }
);

export const lookupProcedureCode = handlers.get(
  schemaValidator,
  '/:code',
  {
    name: 'Lookup Procedure Code',
    access: 'protected',
    summary:
      "Looks up a procedure code against the caller's organization's currently active code-set provider",
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      sessionSchema: {
        organizationId: string
      },
      allowedPermissions: MANAGE_CODESETS_PERMISSIONS
    },
    params: {
      code: string
    },
    responses: {
      200: {
        code: string,
        description: string
      },
      404: string
    }
  },
  async (req, res) => {
    const { code } = req.params;
    const organizationId = req.session?.organizationId;
    openTelemetryCollector.debug('Looking up procedure code', {
      code,
      organizationId
    });
    const codeSetProvider = await resolverFactory({ context: { tenantId: organizationId } }).resolve(organizationId);
    const result = await codeSetProvider.lookupProcedureCode({ code });

    if (!result) {
      res.status(404).send(`Procedure code '${code}' not found`);
      return;
    }

    res.status(200).json(result);
  }
);
