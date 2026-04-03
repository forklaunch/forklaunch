use std::{fs::read_to_string, path::Path};

use anyhow::{Context, Result};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::{Parser, ParserReturn};
use oxc_span::SourceType;

#[derive(Debug, Clone, PartialEq)]
pub enum RelationType {
    ManyToOne,
    OneToMany,
    ManyToMany,
    OneToOne,
}

#[derive(Debug, Clone)]
pub struct EntityProperty {
    pub name: String,
    pub type_name: String,
    #[allow(dead_code)]
    pub is_nullable: bool,
    #[allow(dead_code)]
    pub is_collection: bool,
    pub relation_type: Option<RelationType>,
}

#[derive(Debug, Clone)]
pub struct EntityDefinition {
    pub name: String,
    #[allow(dead_code)]
    pub extends: Option<String>,
    pub properties: Vec<EntityProperty>,
}

pub struct EntityAnalyzer;

impl EntityAnalyzer {
    /// Parse a TypeScript entity file and extract entity definitions.
    /// Supports the v7 defineEntity() format:
    ///
    /// ```typescript
    /// export const User = defineEntity({
    ///   name: 'User',
    ///   properties: {
    ///     ...sqlBaseProperties,
    ///     email: p.string().unique(),
    ///     organization: () => p.manyToOne(Organization).nullable(),
    ///   },
    /// });
    /// ```
    pub fn parse_entity_file(path: &Path) -> Result<Vec<EntityDefinition>> {
        let source = read_to_string(path)
            .with_context(|| format!("Failed to read entity file: {}", path.display()))?;

        let allocator = Allocator::default();
        let source_type = SourceType::from_path(path)
            .unwrap_or_default()
            .with_typescript(true);

        let ParserReturn {
            program, errors, ..
        } = Parser::new(&allocator, &source, source_type).parse();

        if !errors.is_empty() {
            anyhow::bail!("Failed to parse entity file: {:?}", errors);
        }

        let mut entities = Vec::new();

        for stmt in &program.body {
            if let Statement::ExportNamedDeclaration(export_decl) = stmt {
                if let Some(Declaration::VariableDeclaration(var_decl)) = &export_decl.declaration {
                    for declarator in &var_decl.declarations {
                        if let Some(init) = &declarator.init {
                            if Self::is_define_entity_call(init) {
                                if let Some(entity) = Self::extract_entity_from_define_entity(init)
                                {
                                    entities.push(entity);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(entities)
    }

    /// Check if an expression is a defineEntity() call
    fn is_define_entity_call(expr: &Expression) -> bool {
        if let Expression::CallExpression(call) = expr {
            if let Expression::Identifier(id) = &call.callee {
                return id.name.as_str() == "defineEntity";
            }
        }
        false
    }

    /// Extract entity definition from a defineEntity({...}) call
    fn extract_entity_from_define_entity(expr: &Expression) -> Option<EntityDefinition> {
        let call = match expr {
            Expression::CallExpression(call) => call,
            _ => return None,
        };

        let config_obj = match call.arguments.first() {
            Some(Argument::ObjectExpression(obj)) => obj,
            _ => return None,
        };

        let mut name = String::new();
        let mut properties = Vec::new();

        for prop in &config_obj.properties {
            if let ObjectPropertyKind::ObjectProperty(obj_prop) = prop {
                let key = match &obj_prop.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    _ => continue,
                };

                match key {
                    "name" => {
                        if let Expression::StringLiteral(lit) = &obj_prop.value {
                            name = lit.value.as_str().to_string();
                        }
                    }
                    "properties" => {
                        if let Expression::ObjectExpression(props_obj) = &obj_prop.value {
                            properties = Self::extract_properties_from_object(props_obj);
                        }
                    }
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            return None;
        }

        Some(EntityDefinition {
            name,
            extends: None, // class-free entities have no class hierarchy
            properties,
        })
    }

    /// Extract entity properties from the properties object in defineEntity
    fn extract_properties_from_object(obj: &ObjectExpression) -> Vec<EntityProperty> {
        let mut properties = Vec::new();

        for prop in &obj.properties {
            match prop {
                // Skip spread elements (e.g., ...sqlBaseProperties)
                ObjectPropertyKind::SpreadProperty(_) => continue,
                ObjectPropertyKind::ObjectProperty(obj_prop) => {
                    let name = match &obj_prop.key {
                        PropertyKey::StaticIdentifier(id) => id.name.as_str().to_string(),
                        _ => continue,
                    };

                    // The value is either:
                    // 1. A property builder chain: p.string().unique()
                    // 2. An arrow function wrapping a relation: () => p.manyToOne(Organization)
                    let chain_expr = Self::unwrap_arrow_function(&obj_prop.value);
                    let parsed = Self::parse_property_builder_chain(chain_expr);

                    properties.push(EntityProperty {
                        name,
                        type_name: parsed.type_name,
                        is_nullable: parsed.is_nullable,
                        is_collection: parsed.is_collection,
                        relation_type: parsed.relation_type,
                    });
                }
            }
        }

        properties
    }

    /// If the expression is an arrow function, unwrap to its body expression
    fn unwrap_arrow_function<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
        if let Expression::ArrowFunctionExpression(arrow) = expr {
            // expression: true means it's a concise body (no braces)
            if arrow.expression {
                if let Some(Statement::ExpressionStatement(expr_stmt)) =
                    arrow.body.statements.first()
                {
                    return &expr_stmt.expression;
                }
            }
            // For arrow functions with block body, try to extract the return expression
            if let Some(Statement::ReturnStatement(ret)) = arrow.body.statements.first() {
                if let Some(arg) = &ret.argument {
                    return arg;
                }
            }
        }
        expr
    }

    /// Parse a property builder chain like p.string().unique().nullable()
    /// or p.manyToOne(Organization).nullable()
    ///
    /// Walks the chain from outermost to innermost:
    ///   CallExpr(.nullable) -> MemberExpr -> CallExpr(p.string) -> MemberExpr -> Identifier(p)
    fn parse_property_builder_chain(expr: &Expression) -> ParsedProperty {
        let mut result = ParsedProperty {
            type_name: "unknown".to_string(),
            is_nullable: false,
            is_collection: false,
            relation_type: None,
        };

        Self::walk_chain(expr, &mut result);
        result
    }

    /// Recursively walk a method chain, accumulating modifiers
    fn walk_chain(expr: &Expression, result: &mut ParsedProperty) {
        if let Expression::CallExpression(call) = expr {
            if let Some(member) = call.callee.as_member_expression() {
                if let MemberExpression::StaticMemberExpression(static_member) = member {
                    let method_name = static_member.property.name.as_str();
                    let object = &static_member.object;

                    if Self::is_p_identifier(object) {
                        // This is the base: p.string(), p.manyToOne(X), etc.
                        Self::resolve_base_type(method_name, call, result);
                    } else {
                        // This is a modifier: .nullable(), .unique(), etc.
                        Self::apply_modifier(method_name, result);
                        // Recurse into the inner expression
                        Self::walk_chain(object, result);
                    }
                }
            }
        }
    }

    /// Check if an expression is the `p` identifier (the property builder namespace)
    fn is_p_identifier(expr: &Expression) -> bool {
        matches!(expr, Expression::Identifier(id) if id.name.as_str() == "p")
    }

    /// Resolve the base type from a p.xxx() call
    fn resolve_base_type(method: &str, call: &CallExpression, result: &mut ParsedProperty) {
        match method {
            // Scalar types
            "string" | "text" => result.type_name = "string".to_string(),
            "integer" | "number" | "smallint" | "tinyint" | "mediumint" | "float" | "double" => {
                result.type_name = "number".to_string()
            }
            "boolean" => result.type_name = "boolean".to_string(),
            "datetime" | "date" => result.type_name = "Date".to_string(),
            "uuid" => result.type_name = "string".to_string(),
            "bigint" => result.type_name = "bigint".to_string(),
            "json" => result.type_name = "unknown".to_string(),
            "blob" => result.type_name = "Buffer".to_string(),
            "decimal" => result.type_name = "string".to_string(),
            "time" => result.type_name = "string".to_string(),
            "enum" => result.type_name = "enum".to_string(),
            "type" => result.type_name = "unknown".to_string(),
            "array" => result.type_name = "unknown".to_string(),

            // Relation types - extract the target entity from the first argument
            "manyToOne" => {
                result.relation_type = Some(RelationType::ManyToOne);
                result.type_name = Self::extract_relation_target(call);
            }
            "oneToMany" => {
                result.relation_type = Some(RelationType::OneToMany);
                result.type_name = Self::extract_relation_target(call);
                result.is_collection = true;
            }
            "manyToMany" => {
                result.relation_type = Some(RelationType::ManyToMany);
                result.type_name =
                    format!("Collection<{}>", Self::extract_relation_target(call));
                result.is_collection = true;
            }
            "oneToOne" => {
                result.relation_type = Some(RelationType::OneToOne);
                result.type_name = Self::extract_relation_target(call);
            }

            _ => {}
        }
    }

    /// Extract the target entity name from a relation call: p.manyToOne(Organization)
    fn extract_relation_target(call: &CallExpression) -> String {
        if let Some(Argument::Identifier(id)) = call.arguments.first() {
            return id.name.as_str().to_string();
        }
        "unknown".to_string()
    }

    /// Apply a chain modifier (.nullable(), .unique(), .primary(), etc.)
    fn apply_modifier(method: &str, result: &mut ParsedProperty) {
        match method {
            "nullable" => result.is_nullable = true,
            "array" => result.is_collection = true,
            // Other modifiers like unique, primary, default, etc. are ignored by the analyzer
            _ => {}
        }
    }
}

// Inner struct for parse_property_builder_chain
struct ParsedProperty {
    type_name: String,
    is_nullable: bool,
    is_collection: bool,
    relation_type: Option<RelationType>,
}

#[cfg(test)]
mod tests {
    use std::fs::write;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_parse_define_entity_with_relations() {
        let dir = tempdir().unwrap();
        let entity_path = dir.path().join("user.entity.ts");

        write(
            &entity_path,
            r#"
import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@app/core';
import { Organization } from './organization.entity';
import { Role } from './role.entity';

export const User = defineEntity({
  name: 'User',
  properties: {
    ...sqlBaseProperties,
    name: p.string(),
    email: p.string().unique(),
    age: p.number().nullable(),
    organization: () => p.manyToOne(Organization),
    roles: () => p.manyToMany(Role),
    createdAt: p.datetime(),
  },
});

export type IUser = InferEntity<typeof User>;
"#,
        )
        .unwrap();

        let entities = EntityAnalyzer::parse_entity_file(&entity_path).unwrap();

        assert_eq!(entities.len(), 1);

        let user_entity = &entities[0];
        assert_eq!(user_entity.name, "User");
        assert_eq!(user_entity.extends, None); // class-free, no extends

        eprintln!("Found {} properties:", user_entity.properties.len());
        for prop in &user_entity.properties {
            eprintln!(
                "  - {} : {} (relation: {:?}, nullable: {})",
                prop.name, prop.type_name, prop.relation_type, prop.is_nullable
            );
        }

        // Check name property
        let name_prop = user_entity
            .properties
            .iter()
            .find(|p| p.name == "name")
            .unwrap();
        assert_eq!(name_prop.type_name, "string");
        assert!(!name_prop.is_nullable);
        assert!(name_prop.relation_type.is_none());

        // Check email property (unique, but analyzer doesn't track unique)
        let email_prop = user_entity
            .properties
            .iter()
            .find(|p| p.name == "email")
            .unwrap();
        assert_eq!(email_prop.type_name, "string");
        assert!(!email_prop.is_nullable);

        // Check age property (nullable)
        let age_prop = user_entity
            .properties
            .iter()
            .find(|p| p.name == "age")
            .unwrap();
        assert_eq!(age_prop.type_name, "number");
        assert!(age_prop.is_nullable);

        // Check organization (ManyToOne relation)
        let org_prop = user_entity
            .properties
            .iter()
            .find(|p| p.name == "organization")
            .unwrap();
        assert_eq!(org_prop.type_name, "Organization");
        assert_eq!(org_prop.relation_type, Some(RelationType::ManyToOne));

        // Check roles (ManyToMany relation)
        let roles_prop = user_entity
            .properties
            .iter()
            .find(|p| p.name == "roles")
            .unwrap();
        assert!(roles_prop.type_name.contains("Collection"));
        assert!(roles_prop.is_collection);
        assert_eq!(roles_prop.relation_type, Some(RelationType::ManyToMany));

        // Check createdAt
        let created_prop = user_entity
            .properties
            .iter()
            .find(|p| p.name == "createdAt")
            .unwrap();
        assert_eq!(created_prop.type_name, "Date");
    }

    #[test]
    fn test_parse_simple_entity() {
        let dir = tempdir().unwrap();
        let entity_path = dir.path().join("permission.entity.ts");

        write(
            &entity_path,
            r#"
import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@app/core';

export const Permission = defineEntity({
  name: 'Permission',
  properties: {
    ...sqlBaseProperties,
    slug: p.string(),
  },
});

export type IPermission = InferEntity<typeof Permission>;
"#,
        )
        .unwrap();

        let entities = EntityAnalyzer::parse_entity_file(&entity_path).unwrap();
        assert_eq!(entities.len(), 1);

        let permission = &entities[0];
        assert_eq!(permission.name, "Permission");
        assert_eq!(permission.properties.len(), 1);
        assert_eq!(permission.properties[0].name, "slug");
        assert_eq!(permission.properties[0].type_name, "string");
    }

    #[test]
    fn test_parse_entity_with_nullable_relation() {
        let dir = tempdir().unwrap();
        let entity_path = dir.path().join("user.entity.ts");

        write(
            &entity_path,
            r#"
import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@app/core';
import { Organization } from './organization.entity';

export const User = defineEntity({
  name: 'User',
  properties: {
    ...sqlBaseProperties,
    email: p.string(),
    organization: () => p.manyToOne(Organization).nullable(),
  },
});

export type IUser = InferEntity<typeof User>;
"#,
        )
        .unwrap();

        let entities = EntityAnalyzer::parse_entity_file(&entity_path).unwrap();
        let user = &entities[0];

        let org_prop = user
            .properties
            .iter()
            .find(|p| p.name == "organization")
            .unwrap();
        assert_eq!(org_prop.relation_type, Some(RelationType::ManyToOne));
        assert!(org_prop.is_nullable);
        assert_eq!(org_prop.type_name, "Organization");
    }
}
