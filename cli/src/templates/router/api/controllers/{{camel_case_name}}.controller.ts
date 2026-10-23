import { handlers, {{#is_iam_configured}}bindRuntimeAuth, {{/is_iam_configured}}schemaValidator } from '@{{app_name}}/core';
import { {{camel_case_name}}Contract } from '../contracts/{{camel_case_name}}.contract';
import { ci, tokens } from '../../bootstrapper';

//! resolve the dependencies
//! scopeFactory creates a new dependency injection scope for the service
const scopeFactory = () => ci.createScope();
// serviceFactory returns a new service instance on demand
const serviceFactory = {{#is_worker}}(context?: Record<string, unknown>) =>
  scopeFactory().resolve(tokens.{{pascal_case_name}}Service, context);{{/is_worker}}{{^is_worker}}ci.scopedResolver(tokens.{{pascal_case_name}}Service);{{/is_worker}}
{{#is_iam_configured}}
//! resolve the JWKS public key URL
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);{{/is_iam_configured}}

  // GET endpoint handler that returns a simple message
export const {{camel_case_name}}Get = handlers.get(
  schemaValidator,
  {{camel_case_name}}Contract.get.path,
  {{#is_iam_configured}}bindRuntimeAuth({{camel_case_name}}Contract.get.details, { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL }){{/is_iam_configured}}{{^is_iam_configured}}{{camel_case_name}}Contract.get.details{{/is_iam_configured}},
  async (req, res) => {
    res.status(200).json({
      message: '{{pascal_case_name}}'{{#is_worker}},
      processed: false,
      retryCount: 0
      {{/is_worker}}
    });
  }
);

// POST endpoint handler that processes request body and returns response from service
export const {{camel_case_name}}Post = handlers.post(
  schemaValidator,
  {{camel_case_name}}Contract.post.path,
  {{#is_iam_configured}}bindRuntimeAuth({{camel_case_name}}Contract.post.details, { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL }){{/is_iam_configured}}{{^is_iam_configured}}{{camel_case_name}}Contract.post.details{{/is_iam_configured}},
  async (req, res) => {
    res
      .status(200)
      .json(
        // constructs a new service instance using the scopeFactory and calls the {{camel_case_name}}Post method
        await serviceFactory({{#is_worker}}
          // core 3 rejects '' as a tenant; with no organization the EM stays
          // unscoped, as it does for a service.
          { tenantId: req.session?.organizationId }{{/is_worker}}{{^is_worker}}
          { scope: scopeFactory() }{{/is_worker}}
        ).{{camel_case_name}}Post(req.body)
      );
  }
);
