use std::{fs::read_to_string, path::Path};

use anyhow::{Context, Result};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::{Parser, ParserReturn};
use oxc_span::SourceType;

#[derive(Debug, Clone)]
pub struct SchemaProperty {
    pub name: String,
    #[allow(dead_code)]
    pub type_name: String,
    #[allow(dead_code)]
    pub is_optional: bool,
    pub is_array: bool,
}

#[derive(Debug, Clone)]
pub struct SchemaDefinition {
    pub name: String,
    pub properties: Vec<SchemaProperty>,
}

pub struct SchemaAnalyzer;

impl SchemaAnalyzer {
    /// Parse a TypeScript schema file and extract schema definitions
    pub fn parse_schema_file(path: &Path) -> Result<Vec<SchemaDefinition>> {
        let source = read_to_string(path)
            .with_context(|| format!("Failed to read schema file: {}", path.display()))?;

        let allocator = Allocator::default();
        let source_type = SourceType::from_path(path).unwrap_or_default();

        let ParserReturn {
            program, errors, ..
        } = Parser::new(&allocator, &source, source_type).parse();

        if !errors.is_empty() {
            anyhow::bail!("Failed to parse schema file: {:?}", errors);
        }

        let mut schemas = Vec::new();

        // Visit all variable declarations looking for schema exports
        for stmt in &program.body {
            if let Statement::ExportNamedDeclaration(export_decl) = stmt {
                if let Some(Declaration::VariableDeclaration(var_decl)) = &export_decl.declaration {
                    for declarator in &var_decl.declarations {
                        if let Some(schema) =
                            Self::extract_schema_from_declarator(declarator, &source)
                        {
                            schemas.push(schema);
                        }
                    }
                }
            }
        }

        Ok(schemas)
    }

    fn extract_schema_from_declarator(
        declarator: &VariableDeclarator,
        source: &str,
    ) -> Option<SchemaDefinition> {
        let name = match &declarator.id.kind {
            BindingPatternKind::BindingIdentifier(id) => id.name.as_str().to_string(),
            _ => return None,
        };

        // Only process if it looks like a schema (ends with "Schema")
        if !name.ends_with("Schema") {
            return None;
        }

        let properties = if let Some(init) = &declarator.init {
            Self::extract_properties_from_expression(init, source)
        } else {
            Vec::new()
        };

        Some(SchemaDefinition { name, properties })
    }

    fn extract_properties_from_expression(expr: &Expression, source: &str) -> Vec<SchemaProperty> {
        match expr {
            Expression::ObjectExpression(obj_expr) => {
                let mut properties = Vec::new();

                for prop in &obj_expr.properties {
                    if let ObjectPropertyKind::ObjectProperty(obj_prop) = prop {
                        if let Some(schema_prop) =
                            Self::extract_property_from_object_property(obj_prop, source)
                        {
                            properties.push(schema_prop);
                        }
                    }
                }

                properties
            }
            Expression::CallExpression(call_expr) => {
                // Handle cases like object({ ... }) or record(...)
                if let Some(arg) = call_expr.arguments.first() {
                    if let Argument::ObjectExpression(obj_expr) = arg {
                        return Self::extract_properties_from_object_expression(obj_expr, source);
                    }
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn extract_properties_from_object_expression(
        obj_expr: &ObjectExpression,
        source: &str,
    ) -> Vec<SchemaProperty> {
        let mut properties = Vec::new();

        for prop in &obj_expr.properties {
            if let ObjectPropertyKind::ObjectProperty(obj_prop) = prop {
                if let Some(schema_prop) =
                    Self::extract_property_from_object_property(obj_prop, source)
                {
                    properties.push(schema_prop);
                }
            }
        }

        properties
    }

    fn extract_property_from_object_property(
        obj_prop: &ObjectProperty,
        source: &str,
    ) -> Option<SchemaProperty> {
        let name = match &obj_prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str().to_string(),
            PropertyKey::StringLiteral(lit) => lit.value.as_str().to_string(),
            _ => return None,
        };

        let (type_name, is_optional, is_array) =
            Self::analyze_property_value(&obj_prop.value, source);

        Some(SchemaProperty {
            name,
            type_name,
            is_optional,
            is_array,
        })
    }

    fn analyze_property_value(expr: &Expression, _source: &str) -> (String, bool, bool) {
        match expr {
            // Direct type references: string, number, date, boolean
            Expression::Identifier(id) => {
                let type_name = id.name.as_str();
                (type_name.to_string(), false, false)
            }

            // Function calls: optional(string), array(string), etc.
            Expression::CallExpression(call_expr) => {
                if let Expression::Identifier(callee) = &call_expr.callee {
                    let function_name = callee.name.as_str();

                    match function_name {
                        "optional" => {
                            // optional(string) -> (string, true, false)
                            // optional(array(string)) -> (string, true, true)
                            if let Some(arg) = call_expr.arguments.first() {
                                let (inner_type, _, is_array) = match arg {
                                    Argument::Identifier(id) => {
                                        (id.name.as_str().to_string(), false, false)
                                    }
                                    Argument::CallExpression(call) => {
                                        // optional(array(string))
                                        if let Expression::Identifier(callee) = &call.callee {
                                            if callee.name.as_str() == "array" {
                                                if let Some(inner_arg) = call.arguments.first() {
                                                    let inner_type = match inner_arg {
                                                        Argument::Identifier(inner_id) => {
                                                            inner_id.name.as_str().to_string()
                                                        }
                                                        _ => "unknown".to_string(),
                                                    };
                                                    return (inner_type, true, true);
                                                }
                                            }
                                        }
                                        ("unknown".to_string(), false, false)
                                    }
                                    _ => ("unknown".to_string(), false, false),
                                };
                                return (inner_type, true, is_array);
                            }
                            ("unknown".to_string(), true, false)
                        }
                        "array" => {
                            // array(string) -> (string, false, true)
                            if let Some(arg) = call_expr.arguments.first() {
                                let inner_type = match arg {
                                    Argument::Identifier(id) => id.name.as_str().to_string(),
                                    Argument::CallExpression(call) => {
                                        // array(optional(string))
                                        if let Expression::Identifier(callee) = &call.callee {
                                            if callee.name.as_str() == "optional" {
                                                if let Some(Argument::Identifier(inner)) =
                                                    call.arguments.first()
                                                {
                                                    return (
                                                        inner.name.as_str().to_string(),
                                                        true,
                                                        true,
                                                    );
                                                }
                                            }
                                        }
                                        "unknown".to_string()
                                    }
                                    _ => "unknown".to_string(),
                                };
                                return (inner_type, false, true);
                            }
                            ("unknown".to_string(), false, true)
                        }
                        "record" => {
                            // record(string, unknown) -> Map type
                            ("Record".to_string(), false, false)
                        }
                        _ => (function_name.to_string(), false, false),
                    }
                } else {
                    ("unknown".to_string(), false, false)
                }
            }

            _ => ("unknown".to_string(), false, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::write;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_parse_simple_schema() {
        let dir = tempdir().unwrap();
        let schema_path = dir.path().join("user.schema.ts");

        write(
            &schema_path,
            r#"
import { string, number, optional, array } from '@forklaunch/validator';

export const UserRequestSchema = {
  name: string,
  email: string,
  age: optional(number),
  roles: array(string)
};

export const UserResponseSchema = {
  id: string,
  name: string,
  email: string,
  createdAt: date
};
"#,
        )
        .unwrap();

        let schemas = SchemaAnalyzer::parse_schema_file(&schema_path).unwrap();

        assert_eq!(schemas.len(), 2);

        let request_schema = schemas
            .iter()
            .find(|s| s.name == "UserRequestSchema")
            .unwrap();
        assert_eq!(request_schema.properties.len(), 4);

        let name_prop = request_schema
            .properties
            .iter()
            .find(|p| p.name == "name")
            .unwrap();
        assert_eq!(name_prop.type_name, "string");
        assert!(!name_prop.is_optional);
        assert!(!name_prop.is_array);

        let age_prop = request_schema
            .properties
            .iter()
            .find(|p| p.name == "age")
            .unwrap();
        assert_eq!(age_prop.type_name, "number");
        assert!(age_prop.is_optional);

        let roles_prop = request_schema
            .properties
            .iter()
            .find(|p| p.name == "roles")
            .unwrap();
        assert_eq!(roles_prop.type_name, "string");
        assert!(roles_prop.is_array);
    }

    #[test]
    fn test_parse_optional_array_schema() {
        let dir = tempdir().unwrap();
        let schema_path = dir.path().join("test.schema.ts");

        write(
            &schema_path,
            r#"
import { string, optional, array } from '@forklaunch/validator';

export const TestSchema = {
  tags: optional(array(string)),
  categories: optional(array(string))
};
"#,
        )
        .unwrap();

        let schemas = SchemaAnalyzer::parse_schema_file(&schema_path).unwrap();

        assert_eq!(schemas.len(), 1);

        let test_schema = schemas.first().unwrap();
        assert_eq!(test_schema.name, "TestSchema");
        assert_eq!(test_schema.properties.len(), 2);

        let tags_prop = test_schema
            .properties
            .iter()
            .find(|p| p.name == "tags")
            .unwrap();
        assert_eq!(tags_prop.type_name, "string");
        assert!(tags_prop.is_optional);
        assert!(tags_prop.is_array);

        let categories_prop = test_schema
            .properties
            .iter()
            .find(|p| p.name == "categories")
            .unwrap();
        assert_eq!(categories_prop.type_name, "string");
        assert!(categories_prop.is_optional);
        assert!(categories_prop.is_array);
    }
}
