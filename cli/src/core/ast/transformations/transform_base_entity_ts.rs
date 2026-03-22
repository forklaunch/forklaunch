use std::{fs::exists, path::Path};

use anyhow::{Context, Result};
use oxc_allocator::{Allocator, CloneIn, HashMap, Vec};
use oxc_ast::ast::{ClassElement, PropertyKey, SourceType, Statement};
use oxc_codegen::{Codegen, CodegenOptions};

use crate::{
    constants::{Database, error_failed_to_read_file},
    core::{
        ast::parse_ast_program::parse_ast_program,
        rendered_template::{RenderedTemplatesCache, TEMPLATES_DIR},
    },
};

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

    let mut user_defined_imports = Vec::new_in(&allocator);
    for stmt in base_entity_to_copy_program.body.iter() {
        let import = match stmt {
            Statement::ImportDeclaration(import) => import,
            _ => continue,
        };

        if ![
            "@forklaunch/core/persistence",
            "@mikro-orm/core",
            "@mikro-orm/mongodb",
        ]
        .contains(&import.source.value.as_str())
        {
            user_defined_imports.push(Statement::ImportDeclaration(import.clone_in(&allocator)));
        }
    }

    let mut user_defined_properties = HashMap::new_in(&allocator);
    for stmt in base_entity_to_copy_program.body.iter() {
        let statement = match stmt {
            Statement::ClassDeclaration(class) => class,
            _ => continue,
        };

        for body_stmt in statement.body.body.iter() {
            let property = match body_stmt {
                ClassElement::PropertyDefinition(property) => property,
                _ => continue,
            };

            match &property.key {
                PropertyKey::Identifier(identifier) => {
                    if !["id", "_id"].contains(&identifier.name.as_str()) {
                        user_defined_properties.insert(
                            identifier.name.clone(),
                            ClassElement::PropertyDefinition(property.clone_in(&allocator)),
                        );
                    }
                }
                _ => {}
            };
        }
    }

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
        parse_ast_program(&allocator, &base_entity_to_create_text, SourceType::ts());

    let mut last_import_index = 0;
    for (index, stmt) in base_entity_to_create_program.body.iter().enumerate() {
        match stmt {
            Statement::ImportDeclaration(import) => import,
            _ => continue,
        };

        last_import_index = index;
    }

    for import in user_defined_imports.into_iter().rev() {
        base_entity_to_create_program
            .body
            .insert(last_import_index, import);
    }

    for stmt in base_entity_to_create_program.body.iter_mut() {
        let statement = match stmt {
            Statement::ClassDeclaration(class) => class,
            _ => continue,
        };

        let mut preserved_properties = Vec::new_in(&allocator);
        for body_stmt in statement.body.body.iter() {
            let property = match body_stmt {
                ClassElement::PropertyDefinition(property) => property,
                _ => continue,
            };

            match &property.key {
                PropertyKey::Identifier(identifier) => {
                    if ["id", "_id"].contains(&identifier.name.as_str())
                        || !&user_defined_properties.contains_key(&identifier.name)
                    {
                        preserved_properties.push(ClassElement::PropertyDefinition(
                            property.clone_in(&allocator),
                        ));
                    }
                }
                _ => {}
            };
        }

        preserved_properties.extend(user_defined_properties.into_values());
        statement.body.body = preserved_properties;
        break;
    }

    let code = Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&base_entity_to_create_program)
        .code;

    let code = if database == &Database::MongoDB {
        code.replace("id:", "id!:")
    } else {
        code
    };

    Ok(Some(code))
}
