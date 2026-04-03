export * from "./{{camel_case_name}}.controller";{{#is_database_enabled}}
export * from "./compliance.controller";{{/is_database_enabled}}
