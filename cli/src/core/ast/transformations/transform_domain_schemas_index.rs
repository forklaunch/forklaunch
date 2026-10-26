use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result};
use convert_case::{Case, Casing};
use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::{
    Argument, CallExpression, Expression, ImportDeclarationSpecifier, ObjectExpression,
    ObjectPropertyKind, Program, SourceType, Statement,
};
use oxc_codegen::{Codegen, CodegenOptions};
use regex::Regex;

use crate::{
    constants::error_failed_to_read_file,
    core::{ast::parse_ast_program::parse_ast_program, rendered_template::RenderedTemplatesCache},
};

pub(crate) fn transform_domain_schemas_index_ts(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
    ejectable_dependencies: &Vec<String>,
) -> Result<String> {
    let schema_index_ts_path = base_path.join("domain").join("schemas").join("index.ts");
    let template = rendered_templates_cache
        .get(&schema_index_ts_path)?
        .context(error_failed_to_read_file(&schema_index_ts_path))?;
    let schema_index_ts_source_text = &template.content;

    let allocator = Allocator::default();

    let mut program = parse_ast_program(&allocator, &schema_index_ts_source_text, SourceType::ts());

    let mut import_replacements = HashMap::new();
    collect_import_replacements(&program, ejectable_dependencies, &mut import_replacements)?;
    remove_schema_validator_generics(&allocator, &mut program)?;

    let codegen_options = CodegenOptions::default();
    let mut result = Codegen::new().with_options(codegen_options).build(&program);

    result.code = apply_import_replacements(&result.code, &import_replacements)?;

    Ok(result.code)
}

fn collect_import_replacements(
    program: &Program,
    ejectable_dependencies: &Vec<String>,
    import_replacements: &mut HashMap<String, Vec<(String, String)>>,
) -> Result<()> {
    let implementation_regex = Regex::new(r"@forklaunch/implementation-([^/]+)/schemas")?;

    for stmt in program.body.iter() {
        if let Statement::ImportDeclaration(import_decl) = stmt {
            let implementation_package = import_decl.source.value.as_str().unwrap_or_default();
            if implementation_regex.is_match(implementation_package) {
                if !ejectable_dependencies.contains(&implementation_package.to_string()) {
                    continue;
                }

                let mut replacements = Vec::new();

                if let Some(specifiers) = &import_decl.specifiers {
                    for specifier in specifiers.iter() {
                        if let ImportDeclarationSpecifier::ImportSpecifier(import_spec) = specifier
                        {
                            let imported_name = import_spec.imported.name().to_string();

                            if let Some(service_name) =
                                extract_service_name_from_type(&imported_name)
                            {
                                let file_name =
                                    format!("./{}.schema", service_name.to_case(Case::Camel));
                                replacements.push((imported_name, file_name));
                            }
                        }
                    }
                }

                if !replacements.is_empty() {
                    import_replacements.insert(implementation_package.to_string(), replacements);
                }
            }
        }
    }

    Ok(())
}

fn apply_import_replacements(
    code: &str,
    import_replacements: &HashMap<String, Vec<(String, String)>>,
) -> Result<String> {
    let mut result = code.to_string();

    for (implementation_package, replacements) in import_replacements {
        let escaped_package = regex::escape(implementation_package);
        let import_pattern = format!(
            "import\\s+\\{{\\s*([^}}]+)\\s*\\}}\\s+from\\s+['\"]{}['\"].*?;?",
            escaped_package
        );
        let import_regex = Regex::new(&import_pattern)?;

        if let Some(import_match) = import_regex.find(&result) {
            result = result.replace(import_match.as_str(), "");

            let insertion_point = find_import_insertion_point(&result);
            let mut new_imports = String::new();

            for (specifier_name, file_path) in replacements {
                new_imports.push_str(&format!(
                    "import {{ {} }} from '{}';\n",
                    specifier_name, file_path
                ));
            }

            result.insert_str(insertion_point, &new_imports);
        }
    }

    Ok(result)
}

fn find_import_insertion_point(code: &str) -> usize {
    let lines: Vec<&str> = code.lines().collect();
    let mut insertion_line = 0;

    for (i, line) in lines.iter().enumerate() {
        if line.contains("@forklaunch/core/mappers") {
            insertion_line = i + 1;
            break;
        }
    }

    let mut byte_position = 0;
    for (i, line) in lines.iter().enumerate() {
        if i == insertion_line {
            break;
        }
        byte_position += line.len() + 1;
    }

    byte_position
}

fn remove_schema_validator_generics<'a>(
    allocator: &'a Allocator,
    program: &mut Program<'a>,
) -> Result<()> {
    for stmt in program.body.iter_mut() {
        if let Statement::VariableDeclaration(var_decl) = stmt {
            for declarator in var_decl.declarations.iter_mut() {
                if let Some(Expression::CallExpression(call_expr)) = &mut declarator.init {
                    if is_map_service_schemas_call(call_expr) {
                        if let Some(Argument::ObjectExpression(obj_expr)) =
                            call_expr.arguments.get_mut(0)
                        {
                            remove_generics_from_object_properties(allocator, obj_expr);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn is_map_service_schemas_call(call_expr: &CallExpression) -> bool {
    if let Expression::Identifier(ident) = &call_expr.callee {
        ident.name == "mapServiceSchemas"
    } else {
        false
    }
}

fn remove_generics_from_object_properties<'a>(
    allocator: &'a Allocator,
    obj_expr: &mut ObjectExpression<'a>,
) {
    for property in obj_expr.properties.iter_mut() {
        if let ObjectPropertyKind::ObjectProperty(obj_prop) = property {
            match &mut obj_prop.value {
                Expression::CallExpression(call_expr) => {
                    call_expr.type_arguments = None;
                }
                Expression::TSInstantiationExpression(ts_inst) => {
                    obj_prop.value = ts_inst.expression.clone_in(allocator);
                }
                _ => {}
            }
        }
    }
}

fn extract_service_name_from_type(type_name: &str) -> Option<String> {
    let service_schemas_suffix = "ServiceSchemas";
    if !type_name.ends_with(service_schemas_suffix) {
        return None;
    }

    let without_suffix = &type_name[..type_name.len() - service_schemas_suffix.len()];
    let prefixes = ["Stripe", "Base", ""];

    for prefix in &prefixes {
        if without_suffix.starts_with(prefix) {
            let service_name = &without_suffix[prefix.len()..];
            if !service_name.is_empty() {
                return Some(service_name.to_string());
            }
        }
    }

    Some(without_suffix.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, write};

    use tempfile::TempDir;

    use super::*;
    use crate::core::rendered_template::RenderedTemplatesCache;

    fn create_temp_schema_file(content: &str) -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("test-project");
        let schema_dir = project_path.join("domain").join("schemas");
        create_dir_all(&schema_dir).unwrap();

        write(schema_dir.join("index.ts"), content).unwrap();
        temp_dir
    }

    #[test]
    fn test_transform_stripe_billing_schemas() {
        let input_content = r#"import { SchemaValidator, schemaValidator } from '@forklaunch/blueprint-core';
import { mapServiceSchemas } from '@forklaunch/core/mappers';
import {
  StripeBillingPortalServiceSchemas,
  StripeCheckoutSessionServiceSchemas,
  StripePaymentLinkServiceSchemas,
  StripePlanServiceSchemas,
  StripeSubscriptionServiceSchemas
} from '@forklaunch/implementation-billing-stripe/schemas';

const schemas = mapServiceSchemas(
  {
    BillingPortalSchemas: StripeBillingPortalServiceSchemas<SchemaValidator>,
    CheckoutSessionSchemas:
      StripeCheckoutSessionServiceSchemas<SchemaValidator>,
    PaymentLinkSchemas: StripePaymentLinkServiceSchemas<SchemaValidator>,
    PlanSchemas: StripePlanServiceSchemas<SchemaValidator>,
    SubscriptionSchemas: StripeSubscriptionServiceSchemas<SchemaValidator>
  },
  {
    validator: schemaValidator
  }
);

export const {
  BillingPortalSchemas,
  CheckoutSessionSchemas,
  PaymentLinkSchemas,
  PlanSchemas,
  SubscriptionSchemas
} = schemas;"#;

        let temp_dir = create_temp_schema_file(input_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_domain_schemas_index_ts(
            &cache,
            &project_path,
            &vec!["@forklaunch/implementation-billing-stripe/schemas".to_string()],
        );

        assert!(result.is_ok());
        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("<SchemaValidator>"));
        assert!(!transformed_code.contains("@forklaunch/implementation-billing-stripe/schemas"));
        assert!(transformed_code.contains("from './billingPortal.schema'"));
        assert!(transformed_code.contains("from './checkoutSession.schema'"));
        assert!(transformed_code.contains("from './paymentLink.schema'"));
        assert!(transformed_code.contains("from './plan.schema'"));
        assert!(transformed_code.contains("from './subscription.schema'"));
        assert!(transformed_code.contains("from \"@forklaunch/blueprint-core\""));
        assert!(transformed_code.contains("from \"@forklaunch/core/mappers\""));
    }

    #[test]
    fn test_transform_base_iam_schemas() {
        let input_content = r#"import { SchemaValidator, schemaValidator } from '@forklaunch/blueprint-core';
import { mapServiceSchemas } from '@forklaunch/core/mappers';
import {
  BaseUserServiceSchemas,
  BaseAuthServiceSchemas,
  BaseSessionServiceSchemas
} from '@forklaunch/implementation-iam-base/schemas';

const schemas = mapServiceSchemas(
  {
    UserSchemas: BaseUserServiceSchemas<SchemaValidator>,
    AuthSchemas: BaseAuthServiceSchemas<SchemaValidator>,
    SessionSchemas: BaseSessionServiceSchemas<SchemaValidator>
  },
  {
    validator: schemaValidator
  }
);

export const {
  UserSchemas,
  AuthSchemas,
  SessionSchemas
} = schemas;"#;

        let temp_dir = create_temp_schema_file(input_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_domain_schemas_index_ts(
            &cache,
            &project_path,
            &vec!["@forklaunch/implementation-iam-base/schemas".to_string()],
        );

        assert!(result.is_ok());
        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("<SchemaValidator>"));
        assert!(transformed_code.contains("from './user.schema'"));
        assert!(transformed_code.contains("from './auth.schema'"));
        assert!(transformed_code.contains("from './session.schema'"));
        assert!(!transformed_code.contains("@forklaunch/implementation-iam-base/schemas"));
    }

    #[test]
    fn test_transform_with_custom_text() {
        let input_content = r#"import { SchemaValidator, schemaValidator } from '@forklaunch/blueprint-core';
import { mapServiceSchemas } from '@forklaunch/core/mappers';
import {
  StripeProductServiceSchemas
} from '@forklaunch/implementation-billing-stripe/schemas';

const schemas = mapServiceSchemas(
  {
    ProductSchemas: StripeProductServiceSchemas<SchemaValidator>
  },
  {
    validator: schemaValidator
  }
);

export const { ProductSchemas } = schemas;"#;

        let temp_dir = TempDir::new().unwrap();
        let mut cache = RenderedTemplatesCache::new();
        cache.insert(
            temp_dir
                .path()
                .join("domain")
                .join("schemas")
                .join("index.ts")
                .to_string_lossy()
                .to_string(),
            crate::core::rendered_template::RenderedTemplate {
                path: temp_dir
                    .path()
                    .join("domain")
                    .join("schemas")
                    .join("index.ts"),
                content: input_content.to_string(),
                context: None,
            },
        );

        let result = transform_domain_schemas_index_ts(
            &cache,
            &temp_dir.path(),
            &vec!["@forklaunch/implementation-billing-stripe/schemas".to_string()],
        );

        assert!(result.is_ok());
        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("<SchemaValidator>"));
        assert!(transformed_code.contains("from './product.schema'"));
        assert!(!transformed_code.contains("@forklaunch/implementation-billing-stripe/schemas"));
    }

    #[test]
    fn test_extract_service_name_from_type() {
        assert_eq!(
            extract_service_name_from_type("StripeBillingPortalServiceSchemas"),
            Some("BillingPortal".to_string())
        );

        assert_eq!(
            extract_service_name_from_type("BaseUserServiceSchemas"),
            Some("User".to_string())
        );

        assert_eq!(
            extract_service_name_from_type("StripeCheckoutSessionServiceSchemas"),
            Some("CheckoutSession".to_string())
        );

        assert_eq!(
            extract_service_name_from_type("PaymentLinkServiceSchemas"),
            Some("PaymentLink".to_string())
        );

        assert_eq!(extract_service_name_from_type("InvalidTypeName"), None);

        assert_eq!(
            extract_service_name_from_type("ServiceSchemas"),
            Some("".to_string())
        );
    }

    #[test]
    fn test_complex_schema_transformation() {
        let input_content = r#"import { SchemaValidator, schemaValidator } from '@forklaunch/blueprint-core';
import { mapServiceSchemas } from '@forklaunch/core/mappers';
import { someOtherImport } from 'other-package';
import {
  StripeCustomerServiceSchemas,
  StripeInvoiceServiceSchemas
} from '@forklaunch/implementation-billing-stripe/schemas';
import {
  BaseNotificationServiceSchemas
} from '@forklaunch/implementation-notification-base/schemas';

const schemas = mapServiceSchemas(
  {
    CustomerSchemas: StripeCustomerServiceSchemas<SchemaValidator>,
    InvoiceSchemas: StripeInvoiceServiceSchemas<SchemaValidator>,
    NotificationSchemas: BaseNotificationServiceSchemas<SchemaValidator>
  },
  {
    validator: schemaValidator
  }
);

export const {
  CustomerSchemas,
  InvoiceSchemas,
  NotificationSchemas
} = schemas;"#;

        let temp_dir = create_temp_schema_file(input_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_domain_schemas_index_ts(
            &cache,
            &project_path,
            &vec![
                "@forklaunch/implementation-billing-stripe/schemas".to_string(),
                "@forklaunch/implementation-notification-base/schemas".to_string(),
            ],
        );

        assert!(result.is_ok());
        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("<SchemaValidator>"));
        assert!(transformed_code.contains("from './customer.schema'"));
        assert!(transformed_code.contains("from './invoice.schema'"));
        assert!(transformed_code.contains("from './notification.schema'"));
        assert!(!transformed_code.contains("@forklaunch/implementation-billing-stripe/schemas"));
        assert!(!transformed_code.contains("@forklaunch/implementation-notification-base/schemas"));
        assert!(transformed_code.contains("from \"@forklaunch/blueprint-core\""));
        assert!(transformed_code.contains("from \"@forklaunch/core/mappers\""));
        assert!(transformed_code.contains("from \"other-package\""));
    }

    #[test]
    fn test_no_implementation_imports() {
        let input_content = r#"import { SchemaValidator, schemaValidator } from '@forklaunch/blueprint-core';
import { mapServiceSchemas } from '@forklaunch/core/mappers';
import { CustomSchema } from './custom.schema';

const schemas = mapServiceSchemas(
  {
    CustomSchemas: CustomSchema
  },
  {
    validator: schemaValidator
  }
);

export const { CustomSchemas } = schemas;"#;

        let temp_dir = create_temp_schema_file(input_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_domain_schemas_index_ts(&cache, &project_path, &vec![]);

        assert!(result.is_ok());
        let transformed_code = result.unwrap();

        assert!(transformed_code.contains("from \"@forklaunch/blueprint-core\""));
        assert!(transformed_code.contains("from \"@forklaunch/core/mappers\""));
        assert!(transformed_code.contains("from \"./custom.schema\""));
    }

    #[test]
    fn test_ejectable_dependencies_whitelist() {
        let input_content = r#"import { SchemaValidator, schemaValidator } from '@forklaunch/blueprint-core';
import { mapServiceSchemas } from '@forklaunch/core/mappers';
import {
  StripeBillingPortalServiceSchemas,
  StripeCheckoutSessionServiceSchemas
} from '@forklaunch/implementation-billing-stripe/schemas';
import {
  BaseUserServiceSchemas,
  BaseAuthServiceSchemas
} from '@forklaunch/implementation-iam-base/schemas';

const schemas = mapServiceSchemas(
  {
    BillingPortalSchemas: StripeBillingPortalServiceSchemas<SchemaValidator>,
    CheckoutSessionSchemas: StripeCheckoutSessionServiceSchemas<SchemaValidator>,
    UserSchemas: BaseUserServiceSchemas<SchemaValidator>,
    AuthSchemas: BaseAuthServiceSchemas<SchemaValidator>
  },
  {
    validator: schemaValidator
  }
);

export const {
  BillingPortalSchemas,
  CheckoutSessionSchemas,
  UserSchemas,
  AuthSchemas
} = schemas;"#;

        let temp_dir = create_temp_schema_file(input_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_domain_schemas_index_ts(
            &cache,
            &project_path,
            &vec!["@forklaunch/implementation-billing-stripe/schemas".to_string()],
        );

        assert!(result.is_ok());
        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("@forklaunch/implementation-billing-stripe/schemas"));
        assert!(transformed_code.contains("from './billingPortal.schema'"));
        assert!(transformed_code.contains("from './checkoutSession.schema'"));
        assert!(transformed_code.contains("@forklaunch/implementation-iam-base/schemas"));
        assert!(!transformed_code.contains("from './user.schema'"));
        assert!(!transformed_code.contains("from './auth.schema'"));
        assert!(!transformed_code.contains("<SchemaValidator>"));
    }
}
