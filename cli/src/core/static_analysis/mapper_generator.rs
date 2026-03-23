use super::{
    entity_analyzer::{EntityDefinition, EntityProperty, RelationType},
    schema_analyzer::{SchemaDefinition, SchemaProperty},
};
use convert_case::{Case, Casing};

pub struct MapperGenerator {
    schema: SchemaDefinition,
    entity: EntityDefinition,
    app_name: String,
    is_worker: bool,
}

impl MapperGenerator {
    pub fn new(
        schema: SchemaDefinition,
        entity: EntityDefinition,
        app_name: String,
        is_worker: bool,
    ) -> Self {
        Self {
            schema,
            entity,
            app_name,
            is_worker,
        }
    }

    pub fn generate_mapper_file(&self) -> String {
        let pascal_case_name = self.entity.name.replace("Record", "").replace("EventRecord", "");
        let camel_case_name = pascal_case_name.to_case(Case::Camel);
        // Entity const is PascalCase (e.g., "UserRecord")
        let entity_const_name = &self.entity.name;

        let imports = self.generate_imports(&pascal_case_name, &camel_case_name, &entity_const_name);
        let request_mapper = self.generate_request_mapper(&pascal_case_name, &entity_const_name);
        let response_mapper = self.generate_response_mapper(&pascal_case_name, &entity_const_name);

        format!(
            "{}\n\n{}\n\n{}",
            imports, request_mapper, response_mapper
        )
    }

    fn generate_imports(&self, pascal_case_name: &str, camel_case_name: &str, entity_const_name: &str) -> String {
        let entity_suffix = if self.is_worker { "EventRecord" } else { "Record" };
        let em_import = if !self.is_worker {
            "EntityManager, "
        } else {
            ""
        };

        format!(
            r#"import {{
  requestMapper,
  responseMapper
}} from '@forklaunch/core/mappers';
import {{ schemaValidator }} from '@{}/core';
import {{ {}InferEntity, wrap }} from '@mikro-orm/core';
import {{ {} }} from '../../persistence/entities/{}{}.entity';
import {{ {}RequestSchema, {}ResponseSchema }} from '../schemas/{}.schema';"#,
            self.app_name,
            em_import,
            entity_const_name,
            camel_case_name,
            entity_suffix,
            pascal_case_name,
            pascal_case_name,
            camel_case_name
        )
    }

    fn generate_request_mapper(&self, pascal_case_name: &str, entity_const_name: &str) -> String {
        let to_entity_body = self.generate_to_entity_body();
        let em_param = if !self.is_worker {
            ", em: EntityManager"
        } else {
            ""
        };

        format!(
            r#"// RequestMapper const that maps a request schema to an entity
export const {}RequestMapper = requestMapper({{
  schemaValidator,
  schema: {}RequestSchema,
  entity: {},
  mapperDefinition: {{
    toEntity: async (dto{}) => {{
      return em.create({}, {{
{}
      }});
    }}
  }}
}});"#,
            pascal_case_name,
            pascal_case_name,
            entity_const_name,
            em_param,
            entity_const_name,
            to_entity_body,
        )
    }

    fn generate_response_mapper(&self, pascal_case_name: &str, entity_const_name: &str) -> String {
        format!(
            r#"// ResponseMapper const that maps an entity to a response schema
export const {}ResponseMapper = responseMapper({{
  schemaValidator,
  schema: {}ResponseSchema,
  entity: {},
  mapperDefinition: {{
    toDto: async (entity: InferEntity<typeof {}>) => {{
      return wrap(entity).toPOJO();
    }}
  }}
}});"#,
            pascal_case_name,
            pascal_case_name,
            entity_const_name,
            entity_const_name
        )
    }

    fn generate_to_entity_body(&self) -> String {
        let mut lines = Vec::new();

        for schema_prop in &self.schema.properties {
            if let Some(entity_prop) = self.find_matching_entity_property(schema_prop) {
                let mapping = self.generate_property_mapping(schema_prop, entity_prop);
                lines.push(format!("        {}", mapping));
            } else {
                // Direct mapping if no entity property found (pass through to DTO)
                lines.push(format!("        {}: dto.{},", schema_prop.name, schema_prop.name));
            }
        }

        // Add worker-specific fields
        if self.is_worker {
            lines.push("        processed: false,".to_string());
            lines.push("        retryCount: 0,".to_string());
        }

        // Add auto-populated timestamp fields if entity extends a base class or has these fields
        // All entities that extend sqlBaseProperties, NosqlBaseProperties, etc. have these fields
        if self.entity.extends.is_some() || self.has_entity_property("createdAt") {
            lines.push("        createdAt: new Date(),".to_string());
        }
        if self.entity.extends.is_some() || self.has_entity_property("updatedAt") {
            lines.push("        updatedAt: new Date()".to_string());
        } else {
            if let Some(last) = lines.last_mut() {
                *last = last.trim_end_matches(',').to_string();
            }
        }

        lines.join("\n")
    }

    fn generate_property_mapping(
        &self,
        schema_prop: &SchemaProperty,
        entity_prop: &EntityProperty,
    ) -> String {
        match &entity_prop.relation_type {
            Some(RelationType::ManyToMany) => {
                // array of IDs to Collection
                // roles: await em.find(Role, { id: { $in: dto.roles } })
                let relation_entity = self.extract_relation_entity(&entity_prop.type_name);
                format!(
                    "{}: await em.find({}, {{ id: {{ $in: dto.{} }} }}),",
                    entity_prop.name, relation_entity, schema_prop.name
                )
            }
            Some(RelationType::ManyToOne) => {
                // Single ID to entity reference
                // organization: await em.findOne(Organization, { id: dto.organizationId })
                let relation_entity = &entity_prop.type_name;
                format!(
                    "{}: await em.findOne({}, {{ id: dto.{} }}),",
                    entity_prop.name, relation_entity, schema_prop.name
                )
            }
            Some(RelationType::OneToMany) | Some(RelationType::OneToOne) => {
                // Skip inverse side of relations in request mapping
                format!("// {}: skipped (inverse relation),", entity_prop.name)
            }
            None => {
                // Direct mapping
                format!("{}: dto.{},", entity_prop.name, schema_prop.name)
            }
        }
    }

    fn find_matching_entity_property(&self, schema_prop: &SchemaProperty) -> Option<&EntityProperty> {
        // First try exact name match
        if let Some(entity_prop) = self.entity.properties.iter().find(|ep| ep.name == schema_prop.name) {
            return Some(entity_prop);
        }

        // Try matching with ID suffix removed (e.g., organizationId -> organization)
        if schema_prop.name.ends_with("Id") {
            let base_name = schema_prop.name.trim_end_matches("Id");
            if let Some(entity_prop) = self.entity.properties.iter().find(|ep| ep.name == base_name) {
                return Some(entity_prop);
            }
        }

        // Try matching with plural/singular conversions for array types
        if schema_prop.is_array {
            // Try singular form (e.g., roleIds -> role)
            let singular_name = if schema_prop.name.ends_with("Ids") {
                schema_prop.name.trim_end_matches("Ids")
            } else if schema_prop.name.ends_with("s") {
                schema_prop.name.trim_end_matches("s")
            } else {
                &schema_prop.name
            };

            // Try singular match first
            if let Some(entity_prop) = self.entity.properties.iter().find(|ep| ep.name == singular_name) {
                return Some(entity_prop);
            }

            // Try plural form (e.g., roleIds -> roles)
            let plural_name = format!("{}s", singular_name);
            if let Some(entity_prop) = self.entity.properties.iter().find(|ep| ep.name == plural_name) {
                return Some(entity_prop);
            }
        }

        None
    }

    fn has_entity_property(&self, name: &str) -> bool {
        self.entity.properties.iter().any(|p| p.name == name)
    }

    fn extract_relation_entity(&self, type_name: &str) -> String {
        // Extract "Role" from "Collection<Role>"
        if type_name.starts_with("Collection<") && type_name.ends_with('>') {
            type_name[11..type_name.len() - 1].to_string()
        } else {
            type_name.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::static_analysis::entity_analyzer::{EntityProperty, RelationType};
    use crate::core::static_analysis::schema_analyzer::SchemaProperty;

    #[test]
    fn test_generate_simple_mapper() {
        let schema = SchemaDefinition {
            name: "UserRequestSchema".to_string(),
            properties: vec![
                SchemaProperty {
                    name: "name".to_string(),
                    type_name: "string".to_string(),
                    is_optional: false,
                    is_array: false,
                },
                SchemaProperty {
                    name: "email".to_string(),
                    type_name: "string".to_string(),
                    is_optional: false,
                    is_array: false,
                },
            ],
        };

        let entity = EntityDefinition {
            name: "UserRecord".to_string(),
            extends: Some("sqlBaseProperties".to_string()),
            properties: vec![
                EntityProperty {
                    name: "name".to_string(),
                    type_name: "string".to_string(),
                    is_nullable: false,
                    is_collection: false,
                    relation_type: None,
                },
                EntityProperty {
                    name: "email".to_string(),
                    type_name: "string".to_string(),
                    is_nullable: false,
                    is_collection: false,
                    relation_type: None,
                },
                EntityProperty {
                    name: "createdAt".to_string(),
                    type_name: "Date".to_string(),
                    is_nullable: false,
                    is_collection: false,
                    relation_type: None,
                },
            ],
        };

        let generator = MapperGenerator::new(schema, entity, "my-app".to_string(), false);
        let result = generator.generate_mapper_file();

        assert!(result.contains("requestMapper"));
        assert!(result.contains("responseMapper"));
        assert!(result.contains("UserRequestSchema"));
        assert!(result.contains("name: dto.name"));
        assert!(result.contains("email: dto.email"));
        assert!(result.contains("createdAt: new Date()"));
    }

    #[test]
    fn test_generate_mapper_with_relations() {
        let schema = SchemaDefinition {
            name: "UserRequestSchema".to_string(),
            properties: vec![
                SchemaProperty {
                    name: "name".to_string(),
                    type_name: "string".to_string(),
                    is_optional: false,
                    is_array: false,
                },
                SchemaProperty {
                    name: "organizationId".to_string(),
                    type_name: "string".to_string(),
                    is_optional: false,
                    is_array: false,
                },
                SchemaProperty {
                    name: "roleIds".to_string(),
                    type_name: "string".to_string(),
                    is_optional: false,
                    is_array: true,
                },
            ],
        };

        let entity = EntityDefinition {
            name: "UserRecord".to_string(),
            extends: Some("sqlBaseProperties".to_string()),
            properties: vec![
                EntityProperty {
                    name: "name".to_string(),
                    type_name: "string".to_string(),
                    is_nullable: false,
                    is_collection: false,
                    relation_type: None,
                },
                EntityProperty {
                    name: "organization".to_string(),
                    type_name: "Organization".to_string(),
                    is_nullable: false,
                    is_collection: false,
                    relation_type: Some(RelationType::ManyToOne),
                },
                EntityProperty {
                    name: "roles".to_string(),
                    type_name: "Collection<Role>".to_string(),
                    is_nullable: false,
                    is_collection: true,
                    relation_type: Some(RelationType::ManyToMany),
                },
            ],
        };

        let generator = MapperGenerator::new(schema, entity, "my-app".to_string(), false);
        let result = generator.generate_to_entity_body();

        eprintln!("Generated body:\n{}", result);
        assert!(result.contains("organization: await em.findOne(Organization"));
        assert!(result.contains("roles: await em.find(Role"));
    }
}
