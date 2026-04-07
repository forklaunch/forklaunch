export * from "./{{camel_case_name}}.controller";{{#is_database_enabled}}{{#is_iam_configured}}
export * from "./compliance.controller";{{/is_iam_configured}}{{/is_database_enabled}}
