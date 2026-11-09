import { {{pascal_case_name}}RequestSchema, {{pascal_case_name}}ResponseSchema } from '../../domain/schemas/{{camel_case_name}}.schema';

// Shared by runtime handlers and the build-time exporter. Keep this module free
// of registrations, environment access, database connections and server imports.
export const {{camel_case_name}}Contract = {
  basePath: '/{{kebab_case_name}}',
  get: {
    method: 'get',
    path: '/',
    operationId: '{{camel_case_name}}.{{camel_case_name}}Get',
    details: {
      name: '{{title_case_name}} Get',
      summary: 'Gets {{title_case_name}}',
      access: '{{#is_iam_configured}}protected{{/is_iam_configured}}{{^is_iam_configured}}public{{/is_iam_configured}}',
      {{#is_iam_configured}}auth: { scheme: 'jwt', allowedRoles: ['admin'] },{{/is_iam_configured}}
      responses: { 200: {{pascal_case_name}}ResponseSchema }
    }
  },
  post: {
    method: 'post',
    path: '/',
    operationId: '{{camel_case_name}}.{{camel_case_name}}Post',
    details: {
      name: '{{title_case_name}} Post',
      summary: 'Posts {{title_case_name}}',
      access: '{{#is_iam_configured}}protected{{/is_iam_configured}}{{^is_iam_configured}}public{{/is_iam_configured}}',
      {{#is_iam_configured}}auth: { scheme: 'jwt', allowedRoles: ['admin'] },{{/is_iam_configured}}
      body: {{pascal_case_name}}RequestSchema,
      responses: { 200: {{pascal_case_name}}ResponseSchema }
    }
  }
} as const;
