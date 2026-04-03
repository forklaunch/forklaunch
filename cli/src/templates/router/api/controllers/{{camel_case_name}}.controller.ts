import { handlers, {{#is_iam_configured}}ROLES, {{/is_iam_configured}}schemaValidator } from '@{{app_name}}/core';
{{#with_mappers}}
import { {{pascal_case_name}}RequestMapper, {{pascal_case_name}}ResponseMapper } from '../../domain/mappers/{{camel_case_name}}.mappers';
{{/with_mappers}}
{{^with_mappers}}
import { {{pascal_case_name}}RequestSchema, {{pascal_case_name}}ResponseSchema } from '../../domain/schemas/{{camel_case_name}}.schema';
{{/with_mappers}}
import { ci, tokens } from '../../bootstrapper';

//! resolve the dependencies
//! scopeFactory creates a new dependency injection scope for the service
const scopeFactory = () => ci.createScope();
// serviceFactory returns a new service instance on demand
const serviceFactory = {{#is_worker}}(context?: Record<string, unknown>) =>
  scopeFactory().resolve(tokens.{{pascal_case_name}}Service, context);{{/is_worker}}{{^is_worker}}ci.scopedResolver(tokens.{{pascal_case_name}}Service);{{/is_worker}}
// openTelemetryCollector for collecting logs and metrics with appropriate context
const openTelemetryCollector = ci.resolve(tokens.{{otel_token}});{{#is_iam_configured}}
//! resolve the JWKS public key URL
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);{{/is_iam_configured}}

  // GET endpoint handler that returns a simple message
export const {{camel_case_name}}Get = handlers.get(
  schemaValidator,
  '/',
  {
    name: '{{title_case_name}} Get',
    summary: 'Gets {{title_case_name}}',
    {{#is_iam_configured}}access: 'protected',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedRoles: new Set([ROLES.ADMIN])
    },{{/is_iam_configured}}{{^is_iam_configured}}access: 'public',{{/is_iam_configured}}
    responses: {
      {{#with_mappers}}
      // specifies the success response schema using Mapper constructs
      200: {{pascal_case_name}}ResponseMapper.schema
      {{/with_mappers}}
      {{^with_mappers}}
      200: {{pascal_case_name}}ResponseSchema
      {{/with_mappers}}
    }
  },
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
  '/',
  {
    name: '{{title_case_name}} Post',
    summary: 'Posts {{title_case_name}}',
    {{#is_iam_configured}}access: 'protected',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedRoles: new Set([ROLES.ADMIN])
    },{{/is_iam_configured}}{{^is_iam_configured}}access: 'public',{{/is_iam_configured}}
    {{#with_mappers}}
    // specifies the request body schema using Mapper constructs
    body: {{pascal_case_name}}RequestMapper.schema,
    {{/with_mappers}}
    {{^with_mappers}}
    body: {{pascal_case_name}}RequestSchema,
    {{/with_mappers}}
    responses: {
      {{#with_mappers}}
      // specifies the success response schema using Mapper constructs
      200: {{pascal_case_name}}ResponseMapper.schema
      {{/with_mappers}}
      {{^with_mappers}}
      200: {{pascal_case_name}}ResponseSchema
      {{/with_mappers}}
    }
  },
  async (req, res) => {
    res
      .status(200)
      .json(
        // constructs a new service instance using the scopeFactory and calls the {{camel_case_name}}Post method
        await serviceFactory({{#is_worker}}
          { tenantId: (req.session as Record<string, unknown>)?.organizationId ?? '' }{{/is_worker}}{{^is_worker}}
          { scope: scopeFactory() }{{/is_worker}}
        ).{{camel_case_name}}Post(req.body)
      );
  }
);
