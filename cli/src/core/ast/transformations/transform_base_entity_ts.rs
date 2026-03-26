use std::{fs::exists, path::Path};

use anyhow::{Context, Result};
use oxc_allocator::{Allocator, CloneIn, HashMap, Vec};
use oxc_ast::ast::{
    Declaration, ObjectPropertyKind, PropertyKey, SourceType, Statement,
};
use oxc_codegen::{Codegen, CodegenOptions};

use crate::{
    constants::{Database, error_failed_to_read_file},
    core::{
        ast::parse_ast_program::parse_ast_program,
        rendered_template::{RenderedTemplatesCache, TEMPLATES_DIR},
    },
};

/// Default property names that are part of the base properties templates
/// and should not be copied as user-defined properties.
const BASE_PROPERTY_NAMES: &[&str] = &["id", "_id", "createdAt", "updatedAt", "retentionAnonymizedAt"];

/// Default import sources that are part of the base properties templates
/// and should not be copied as user-defined imports.
/// Note: @forklaunch/core/persistence is NOT excluded here because users may
/// add extra specifiers (e.g., RetentionDuration). Duplicate `fp` specifiers
/// are handled by BASE_IMPORT_SPECIFIERS below.
const BASE_IMPORT_SOURCES: &[&str] = &[
    "@mikro-orm/core",
    "@mikro-orm/mongodb",
    "uuid",
];

/// Import specifiers from @forklaunch/core/persistence that already exist
/// in the base properties templates. If the user's source has ONLY these
/// specifiers from that path, the import is redundant and should be skipped.
const FORKLAUNCH_PERSISTENCE_SOURCE: &str = "@forklaunch/core/persistence";
const BASE_PERSISTENCE_SPECIFIERS: &[&str] = &["fp"];

pub(crate) fn transform_base_entity_ts(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
    database: &Database,
) -> Result<Option<String>> {
    let allocator = Allocator::default();

    let base_entity_file_name_to_create = match database {
        Database::MongoDB => "nosql.base.properties.ts",
        _ => "sql.base.properties.ts",
    };

    let base_entity_file_name_to_copy = match database {
        Database::MongoDB => "sql.base.properties.ts",
        _ => "nosql.base.properties.ts",
    };

    if exists(
        base_path
            .join("core")
            .join("persistence")
            .join(base_entity_file_name_to_create),
    )? {
        return Ok(None);
    }

    // Parse the existing base properties file to extract user-added properties
    let base_entity_to_copy_path = base_path
        .join("core")
        .join("persistence")
        .join(base_entity_file_name_to_copy);
    let template = rendered_templates_cache
        .get(&base_entity_to_copy_path)?
        .context(error_failed_to_read_file(&base_entity_to_copy_path))?;
    let base_entity_to_copy_text = template.content;

    let base_entity_to_copy_program = parse_ast_program(
        &allocator,
        &base_entity_to_copy_text,
        SourceType::from_path(&base_entity_to_copy_path)?,
    );

    // Extract user-defined imports (not part of the base template)
    let mut user_defined_imports = Vec::new_in(&allocator);
    for stmt in base_entity_to_copy_program.body.iter() {
        let import = match stmt {
            Statement::ImportDeclaration(import) => import,
            _ => continue,
        };

        let source = import.source.value.as_str();

        // Fully excluded sources — never copy
        if BASE_IMPORT_SOURCES.contains(&source) {
            continue;
        }

        // For @forklaunch/core/persistence: skip only if ALL specifiers are
        // base specifiers (e.g., just `fp`). If user added extra specifiers
        // (e.g., `RetentionDuration`), keep the import so those aren't lost.
        if source == FORKLAUNCH_PERSISTENCE_SOURCE {
            if let Some(specifiers) = &import.specifiers {
                let all_base = specifiers.iter().all(|s| {
                    match s {
                        oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(spec) => {
                            BASE_PERSISTENCE_SPECIFIERS.contains(&spec.local.name.as_str())
                        }
                        _ => false,
                    }
                });
                if all_base {
                    continue;
                }
            }
        }

        user_defined_imports.push(Statement::ImportDeclaration(import.clone_in(&allocator)));
    }

    // Extract user-defined properties from the exported object literal
    let mut user_defined_properties = HashMap::new_in(&allocator);
    for stmt in base_entity_to_copy_program.body.iter() {
        let export_decl = match stmt {
            Statement::ExportNamedDeclaration(export) => export,
            _ => continue,
        };

        let var_decl = match &export_decl.declaration {
            Some(Declaration::VariableDeclaration(var_decl)) => var_decl,
            _ => continue,
        };

        for declarator in &var_decl.declarations {
            if let Some(oxc_ast::ast::Expression::ObjectExpression(obj)) = &declarator.init {
                for prop in &obj.properties {
                    if let ObjectPropertyKind::ObjectProperty(obj_prop) = prop {
                        if let PropertyKey::StaticIdentifier(id) = &obj_prop.key {
                            if !BASE_PROPERTY_NAMES.contains(&id.name.as_str()) {
                                user_defined_properties.insert(
                                    id.name.clone(),
                                    ObjectPropertyKind::ObjectProperty(
                                        obj_prop.clone_in(&allocator),
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Parse the target template
    let base_entity_to_create_text = TEMPLATES_DIR
        .get_file(
            Path::new("project")
                .join("core")
                .join("persistence")
                .join(base_entity_file_name_to_create),
        )
        .unwrap()
        .contents_utf8()
        .unwrap();
    let mut base_entity_to_create_program =
        parse_ast_program(&allocator, base_entity_to_create_text, SourceType::ts());

    // Inject user-defined imports
    let mut last_import_index = 0;
    for (index, stmt) in base_entity_to_create_program.body.iter().enumerate() {
        if matches!(stmt, Statement::ImportDeclaration(_)) {
            last_import_index = index;
        }
    }

    for import in user_defined_imports.into_iter().rev() {
        base_entity_to_create_program
            .body
            .insert(last_import_index + 1, import);
    }

    // Inject user-defined properties into the target object literal
    for stmt in base_entity_to_create_program.body.iter_mut() {
        let export_decl = match stmt {
            Statement::ExportNamedDeclaration(export) => export,
            _ => continue,
        };

        let var_decl = match &mut export_decl.declaration {
            Some(Declaration::VariableDeclaration(var_decl)) => var_decl,
            _ => continue,
        };

        for declarator in var_decl.declarations.iter_mut() {
            if let Some(oxc_ast::ast::Expression::ObjectExpression(obj)) = &mut declarator.init {
                for prop in user_defined_properties.into_values() {
                    obj.properties.push(prop);
                }
                break;
            }
        }
        break;
    }

    let code = Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&base_entity_to_create_program)
        .code;

    Ok(Some(code))
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, write};

    use tempfile::TempDir;

    use super::*;

    fn create_temp_structure(sql_content: &str, nosql_content: Option<&str>) -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let persistence_dir = temp_dir.path().join("core").join("persistence");
        create_dir_all(&persistence_dir).expect("Failed to create persistence directory");

        write(
            persistence_dir.join("sql.base.properties.ts"),
            sql_content,
        )
        .expect("Failed to write sql.base.properties.ts");

        if let Some(nosql) = nosql_content {
            write(
                persistence_dir.join("nosql.base.properties.ts"),
                nosql,
            )
            .expect("Failed to write nosql.base.properties.ts");
        }

        temp_dir
    }

    #[test]
    fn test_returns_none_when_target_already_exists() {
        let sql_content = r#"import { p } from '@mikro-orm/core';
export const sqlBaseProperties = {
  id: p.uuid().primary(),
  createdAt: p.datetime(),
  updatedAt: p.datetime()
};"#;

        // Both files exist — switching to mongodb but nosql already exists
        let temp_dir = create_temp_structure(sql_content, Some("existing"));
        let cache = RenderedTemplatesCache::new();

        let result =
            transform_base_entity_ts(&cache, temp_dir.path(), &Database::MongoDB);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_creates_nosql_from_sql_without_user_properties() {
        let sql_content = r#"import { p } from '@mikro-orm/core';
import { v4 } from 'uuid';

export const sqlBaseProperties = {
  id: p.uuid().primary().onCreate(() => v4()),
  createdAt: p.datetime().onCreate(() => new Date()),
  updatedAt: p.datetime().onCreate(() => new Date()).onUpdate(() => new Date())
};"#;

        let temp_dir = create_temp_structure(sql_content, None);
        let cache = RenderedTemplatesCache::new();

        let result =
            transform_base_entity_ts(&cache, temp_dir.path(), &Database::MongoDB);

        assert!(result.is_ok());
        let code = result.unwrap().unwrap();

        // Should contain nosql base properties
        assert!(code.contains("nosqlBaseProperties"));
        assert!(code.contains("@mikro-orm/mongodb"));
        // Should not contain sql-specific properties
        assert!(!code.contains("v4()"));
    }

    #[test]
    fn test_preserves_user_added_properties_sql_to_nosql() {
        let sql_content = r#"import { p } from '@mikro-orm/core';
import { v4 } from 'uuid';

export const sqlBaseProperties = {
  id: p.uuid().primary().onCreate(() => v4()),
  createdAt: p.datetime().onCreate(() => new Date()),
  updatedAt: p.datetime().onCreate(() => new Date()).onUpdate(() => new Date()),
  tenantId: p.string(),
  isDeleted: p.boolean()
};"#;

        let temp_dir = create_temp_structure(sql_content, None);
        let cache = RenderedTemplatesCache::new();

        let result =
            transform_base_entity_ts(&cache, temp_dir.path(), &Database::MongoDB);

        assert!(result.is_ok());
        let code = result.unwrap().unwrap();

        assert!(code.contains("nosqlBaseProperties"));
        // User properties should be preserved
        assert!(code.contains("tenantId"));
        assert!(code.contains("isDeleted"));
        // Base properties should come from the nosql template, not copied
        assert!(code.contains("_id"));
    }

    #[test]
    fn test_preserves_user_added_imports() {
        let sql_content = r#"import { p } from '@mikro-orm/core';
import { v4 } from 'uuid';
import { TenantId } from '../types/tenant';

export const sqlBaseProperties = {
  id: p.uuid().primary().onCreate(() => v4()),
  createdAt: p.datetime().onCreate(() => new Date()),
  updatedAt: p.datetime().onCreate(() => new Date()).onUpdate(() => new Date()),
  tenantId: p.type(TenantId)
};"#;

        let temp_dir = create_temp_structure(sql_content, None);
        let cache = RenderedTemplatesCache::new();

        let result =
            transform_base_entity_ts(&cache, temp_dir.path(), &Database::MongoDB);

        assert!(result.is_ok());
        let code = result.unwrap().unwrap();

        // User import should be preserved
        assert!(code.contains("../types/tenant"));
        assert!(code.contains("TenantId"));
        // User property should be preserved
        assert!(code.contains("tenantId"));
    }

    #[test]
    fn test_creates_sql_from_nosql_with_user_properties() {
        let nosql_content = r#"import { p } from '@mikro-orm/core';
import { ObjectId } from '@mikro-orm/mongodb';

export const nosqlBaseProperties = {
  _id: p.type(ObjectId).primary(),
  id: p.string().serializedPrimaryKey().persist(false),
  createdAt: p.datetime().onCreate(() => new Date()),
  updatedAt: p.datetime().onCreate(() => new Date()).onUpdate(() => new Date()),
  region: p.string()
};"#;

        // Only nosql exists, creating sql
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let persistence_dir = temp_dir.path().join("core").join("persistence");
        create_dir_all(&persistence_dir).expect("Failed to create persistence directory");
        write(
            persistence_dir.join("nosql.base.properties.ts"),
            nosql_content,
        )
        .expect("Failed to write nosql.base.properties.ts");

        let cache = RenderedTemplatesCache::new();

        let result =
            transform_base_entity_ts(&cache, temp_dir.path(), &Database::PostgreSQL);

        assert!(result.is_ok());
        let code = result.unwrap().unwrap();

        assert!(code.contains("sqlBaseProperties"));
        assert!(code.contains("uuid"));
        // User property should be preserved
        assert!(code.contains("region"));
        // Nosql-specific properties should not leak
        assert!(!code.contains("ObjectId"));
        assert!(!code.contains("serializedPrimaryKey"));
    }
}
