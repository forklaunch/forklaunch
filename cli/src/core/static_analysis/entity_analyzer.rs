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
    /// Parse a TypeScript entity file and extract entity definitions
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

        // Visit all class declarations looking for entities with @Entity decorator
        for stmt in &program.body {
            if let Statement::ExportNamedDeclaration(export_decl) = stmt {
                if let Some(Declaration::ClassDeclaration(class_decl)) = &export_decl.declaration {
                    if Self::has_entity_decorator(&class_decl.decorators) {
                        if let Some(entity) = Self::extract_entity_from_class(class_decl, &source) {
                            entities.push(entity);
                        }
                    }
                }
            }
        }

        Ok(entities)
    }

    fn has_entity_decorator(decorators: &[Decorator]) -> bool {
        decorators.iter().any(|dec| {
            if let Expression::CallExpression(call) = &dec.expression {
                if let Expression::Identifier(id) = &call.callee {
                    return id.name.as_str() == "Entity";
                }
            }
            false
        })
    }

    fn extract_entity_from_class(class_decl: &Class, source: &str) -> Option<EntityDefinition> {
        let name = class_decl.id.as_ref()?.name.as_str().to_string();

        let extends = class_decl.super_class.as_ref().and_then(|super_expr| {
            if let Expression::Identifier(id) = super_expr {
                Some(id.name.as_str().to_string())
            } else {
                None
            }
        });

        let mut properties = Vec::new();

        for element in &class_decl.body.body {
            if let ClassElement::PropertyDefinition(prop_def) = element {
                if let Some(entity_prop) = Self::extract_property_from_definition(prop_def, source)
                {
                    properties.push(entity_prop);
                }
            }
        }

        Some(EntityDefinition {
            name,
            extends,
            properties,
        })
    }

    fn extract_property_from_definition(
        prop_def: &PropertyDefinition,
        _source: &str,
    ) -> Option<EntityProperty> {
        let name = match &prop_def.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str().to_string(),
            _ => return None,
        };

        // Analyze decorators to determine if this is a relation
        let (relation_type, is_nullable) = Self::analyze_decorators(&prop_def.decorators);

        let (type_name, is_collection) = if let Some(type_annotation) = &prop_def.type_annotation {
            Self::extract_type_info(&type_annotation.type_annotation)
        } else if let Some(value) = &prop_def.value {
            // Try to infer from initializer (e.g., new Collection<Role>(this))
            Self::extract_type_from_initializer(value)
        } else {
            return None;
        };

        Some(EntityProperty {
            name,
            type_name,
            is_nullable,
            is_collection,
            relation_type,
        })
    }

    fn analyze_decorators(decorators: &[Decorator]) -> (Option<RelationType>, bool) {
        let mut relation_type = None;
        let mut is_nullable = false;

        for decorator in decorators {
            if let Expression::CallExpression(call) = &decorator.expression {
                if let Expression::Identifier(id) = &call.callee {
                    let decorator_name = id.name.as_str();

                    // Detect relation decorators
                    relation_type = match decorator_name {
                        "ManyToOne" => Some(RelationType::ManyToOne),
                        "OneToMany" => Some(RelationType::OneToMany),
                        "ManyToMany" => Some(RelationType::ManyToMany),
                        "OneToOne" => Some(RelationType::OneToOne),
                        _ => relation_type,
                    };

                    // Check for nullable option in @Property decorator
                    if decorator_name == "Property" {
                        if let Some(Argument::ObjectExpression(obj)) = call.arguments.first() {
                            for prop in &obj.properties {
                                if let ObjectPropertyKind::ObjectProperty(obj_prop) = prop {
                                    if let PropertyKey::StaticIdentifier(key) = &obj_prop.key {
                                        if key.name.as_str() == "nullable" {
                                            if let Expression::BooleanLiteral(lit) = &obj_prop.value
                                            {
                                                is_nullable = lit.value;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        (relation_type, is_nullable)
    }

    fn extract_type_info(type_ann: &TSType) -> (String, bool) {
        match type_ann {
            TSType::TSTypeReference(type_ref) => {
                if let TSTypeName::IdentifierReference(id) = &type_ref.type_name {
                    let type_name = id.name.as_str();

                    // Check if it's a Collection type
                    if type_name == "Collection" {
                        if let Some(type_params) = &type_ref.type_arguments {
                            if let Some(first_param) = type_params.params.first() {
                                if let TSType::TSTypeReference(inner_ref) = first_param {
                                    if let TSTypeName::IdentifierReference(inner_id) =
                                        &inner_ref.type_name
                                    {
                                        return (
                                            format!("Collection<{}>", inner_id.name.as_str()),
                                            true,
                                        );
                                    }
                                }
                            }
                        }
                        return ("Collection".to_string(), true);
                    }

                    return (type_name.to_string(), false);
                }
                ("unknown".to_string(), false)
            }
            TSType::TSStringKeyword(_) => ("string".to_string(), false),
            TSType::TSNumberKeyword(_) => ("number".to_string(), false),
            TSType::TSBooleanKeyword(_) => ("boolean".to_string(), false),
            TSType::TSUnionType(union) => {
                // Handle optional types like string | undefined
                let has_undefined = union
                    .types
                    .iter()
                    .any(|t| matches!(t, TSType::TSUndefinedKeyword(_)));
                if has_undefined && union.types.len() == 2 {
                    // Extract the non-undefined type
                    for t in &union.types {
                        if !matches!(t, TSType::TSUndefinedKeyword(_)) {
                            let (type_name, is_coll) = Self::extract_type_info(t);
                            return (type_name, is_coll);
                        }
                    }
                }
                ("unknown".to_string(), false)
            }
            _ => ("unknown".to_string(), false),
        }
    }

    fn extract_type_from_initializer(value: &Expression) -> (String, bool) {
        // Handle new Collection<Role>(this) expressions
        if let Expression::NewExpression(new_expr) = value {
            if let Expression::Identifier(id) = &new_expr.callee {
                if id.name.as_str() == "Collection" {
                    // Extract the type argument (Role from Collection<Role>)
                    if let Some(type_args) = &new_expr.type_arguments {
                        if let Some(first_arg) = type_args.params.first() {
                            if let TSType::TSTypeReference(type_ref) = first_arg {
                                if let TSTypeName::IdentifierReference(id) = &type_ref.type_name {
                                    let type_name = format!("Collection<{}>", id.name.as_str());
                                    return (type_name, true);
                                }
                            }
                        }
                    }
                }
            }
        }
        ("unknown".to_string(), false)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::write;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_parse_entity_with_relations() {
        let dir = tempdir().unwrap();
        let entity_path = dir.path().join("user.entity.ts");

        write(
            &entity_path,
            r#"
import { Entity, Property, ManyToOne, ManyToMany, Collection } from '@mikro-orm/core';
import { SqlBaseEntity } from './base.entity';
import { Organization } from './organization.entity';
import { Role } from './role.entity';

@Entity()
export class User extends SqlBaseEntity {
  @Property()
  name!: string;

  @Property()
  email!: string;

  @Property({ nullable: true })
  age?: number;

  @ManyToOne(() => Organization)
  organization!: Organization;

  @ManyToMany(() => Role)
  roles = new Collection<Role>(this);

  @Property()
  createdAt!: Date;
}
"#,
        )
        .unwrap();

        let entities = EntityAnalyzer::parse_entity_file(&entity_path).unwrap();

        assert_eq!(entities.len(), 1);

        let user_entity = &entities[0];
        assert_eq!(user_entity.name, "User");
        assert_eq!(user_entity.extends, Some("SqlBaseEntity".to_string()));

        eprintln!("Found {} properties:", user_entity.properties.len());
        for prop in &user_entity.properties {
            eprintln!(
                "  - {} : {} (relation: {:?})",
                prop.name, prop.type_name, prop.relation_type
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

        // Check roles (ManyToMany relation with Collection)
        let roles_prop = user_entity
            .properties
            .iter()
            .find(|p| p.name == "roles")
            .unwrap();
        assert!(roles_prop.type_name.contains("Collection"));
        assert!(roles_prop.is_collection);
        assert_eq!(roles_prop.relation_type, Some(RelationType::ManyToMany));
    }
}
