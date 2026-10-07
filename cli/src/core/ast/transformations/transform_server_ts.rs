use std::path::Path;

use anyhow::{Context, Result};
use convert_case::{Case, Casing};
use oxc_allocator::Allocator;
use oxc_ast::ast::{Expression, SourceType, Statement};
use oxc_codegen::{Codegen, CodegenOptions};

use crate::{
    constants::error_failed_to_read_file,
    core::{
        ast::{
            injections::{
                inject_into_import_statement::inject_into_import_statement,
                inject_into_server_ts::inject_into_server_ts,
            },
            parse_ast_program::parse_ast_program,
        },
        rendered_template::RenderedTemplatesCache,
    },
};

pub(crate) fn transform_server_ts(
    rendered_templates_cache: &RenderedTemplatesCache,
    router_name: &str,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let server_path = base_path.join("server.ts");
    let template = rendered_templates_cache
        .get(&server_path)?
        .context(error_failed_to_read_file(&server_path))?;
    let server_source_text = template.content;
    let server_source_type = SourceType::from_path(&server_path)?;
    let router_name_camel_case = router_name.to_case(Case::Camel);

    let mut server_program = parse_ast_program(&allocator, &server_source_text, server_source_type);

    let use_injection_text = format!("app.use({router_name_camel_case}Router);",);
    let mut injection_program_ast =
        parse_ast_program(&allocator, &use_injection_text, SourceType::ts());
    inject_into_server_ts(
        &mut server_program,
        &mut injection_program_ast,
        |statements| {
            let mut maybe_splice_pos = None;
            statements.iter().enumerate().for_each(|(index, stmt)| {
                let expr = match stmt {
                    Statement::ExpressionStatement(expr) => expr,
                    _ => return,
                };

                let call = match &expr.expression {
                    Expression::CallExpression(call) => call,
                    _ => return,
                };

                let member = match &call.callee {
                    Expression::StaticMemberExpression(member) => member,
                    _ => return,
                };

                let id = match &member.object {
                    Expression::Identifier(id) => id,
                    _ => return,
                };

                if id.name == "app" && member.property.name == "use" {
                    maybe_splice_pos = Some(index + 1);
                }
            });
            maybe_splice_pos
        },
    )?;

    let forklaunch_routes_import_text = format!(
        "import {{ {router_name_camel_case}Router }} from './api/routes/{router_name_camel_case}.routes';",
    );
    let mut forklaunch_routes_import_injection = parse_ast_program(
        &allocator,
        &forklaunch_routes_import_text,
        server_source_type,
    );

    inject_into_import_statement(
        &mut server_program,
        &mut forklaunch_routes_import_injection,
        format!("./api/routes/{router_name_camel_case}.routes").as_str(),
        &server_source_text,
    )?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&server_program)
        .code)
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, write};

    use tempfile::TempDir;

    use super::*;
    use crate::core::rendered_template::RenderedTemplatesCache;

    fn create_test_server_ts() -> &'static str {
        r#"import { forklaunchExpress, schemaValidator } from '@forklaunch/blueprint-core';
import { getEnvVar } from '@forklaunch/common';
import dotenv from 'dotenv';
import { organizationRouter } from './api/routes/organization.routes';
import { permissionRouter } from './api/routes/permission.routes';
import { roleRouter } from './api/routes/role.routes';
import { userRouter } from './api/routes/user.routes';
import { createDependencyContainer } from './registrations';

//! bootstrap resources and config
const envFilePath = getEnvVar('DOTENV_FILE_PATH');
dotenv.config({ path: envFilePath });
export const { ci, tokens } = createDependencyContainer(envFilePath);
export type ScopeFactory = typeof ci.createScope;

//! resolves the openTelemetryCollector from the configuration
const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

//! creates an instance of forklaunchExpress
const app = forklaunchExpress(schemaValidator, openTelemetryCollector);

//! resolves the host, port, and version from the configuration
const host = ci.resolve(tokens.HOST);
const port = ci.resolve(tokens.PORT);
const version = ci.resolve(tokens.VERSION);
const docsPath = ci.resolve(tokens.DOCS_PATH);

//! mounts the routes to the app
app.use(organizationRouter);
app.use(permissionRouter);
app.use(roleRouter);
app.use(userRouter);

//! starts the server
app.listen(port, host, () => {
  openTelemetryCollector.info(
    `🎉 IAM Server is running at http://${host}:${port} 🎉.\nAn API reference can be accessed at http://${host}:${port}/api/${version}${docsPath}`
  );
});
"#
    }

    fn create_test_server_ts_with_watermark() -> &'static str {
        r#"/** 
 * Generated by ForkLaunch
 * File: server.ts
 * This is an auto-generated file. Modifications are encouraged but may inhibit automated upgrades.
 */

import { forklaunchExpress, schemaValidator } from '@forklaunch/blueprint-core';
import { getEnvVar } from '@forklaunch/common';
import dotenv from 'dotenv';
import { organizationRouter } from './api/routes/organization.routes';
import { permissionRouter } from './api/routes/permission.routes';
import { roleRouter } from './api/routes/role.routes';
import { userRouter } from './api/routes/user.routes';
import { createDependencyContainer } from './registrations';

//! bootstrap resources and config
const envFilePath = getEnvVar('DOTENV_FILE_PATH');
dotenv.config({ path: envFilePath });
export const { ci, tokens } = createDependencyContainer(envFilePath);
export type ScopeFactory = typeof ci.createScope;

//! resolves the openTelemetryCollector from the configuration
const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

//! creates an instance of forklaunchExpress
const app = forklaunchExpress(schemaValidator, openTelemetryCollector);

//! resolves the host, port, and version from the configuration
const host = ci.resolve(tokens.HOST);
const port = ci.resolve(tokens.PORT);
const version = ci.resolve(tokens.VERSION);
const docsPath = ci.resolve(tokens.DOCS_PATH);

//! mounts the routes to the app
app.use(organizationRouter);
app.use(permissionRouter);
app.use(roleRouter);
app.use(userRouter);

//! starts the server
app.listen(port, host, () => {
  openTelemetryCollector.info(
    `🎉 IAM Server is running at http://${host}:${port} 🎉.\nAn API reference can be accessed at http://${host}:${port}/api/${version}${docsPath}`
  );
});
"#
    }

    fn create_temp_project_structure(server_content: &str) -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path();

        create_dir_all(temp_path.join("api/routes"))
            .expect("Failed to create api/routes directory");

        write(temp_path.join("server.ts"), server_content).expect("Failed to write server.ts");

        temp_dir
    }

    #[test]
    fn test_transform_server_ts_successful_injection() {
        let server_content = create_test_server_ts();
        let temp_dir = create_temp_project_structure(server_content);
        let temp_path = temp_dir.path();
        let cache = RenderedTemplatesCache::new();

        let result = transform_server_ts(&cache, "userManagement", temp_path);

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        assert!(transformed_code.contains(
            "import { userManagementRouter } from \"./api/routes/userManagement.routes\";"
        ));

        assert!(transformed_code.contains("app.use(userManagementRouter);"));

        assert!(transformed_code.contains(
            "import { forklaunchExpress, schemaValidator } from \"@forklaunch/blueprint-core\";"
        ));
        assert!(
            transformed_code.contains(
                "const app = forklaunchExpress(schemaValidator, openTelemetryCollector);"
            )
        );
        assert!(transformed_code.contains("app.listen(port, host"));
    }

    #[test]
    fn test_transform_server_ts_with_kebab_case_router_name() {
        let server_content = create_test_server_ts();
        let temp_dir = create_temp_project_structure(server_content);
        let temp_path = temp_dir.path();
        let cache = RenderedTemplatesCache::new();

        let result = transform_server_ts(&cache, "order-processing", temp_path);

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        assert!(transformed_code.contains("app.use(orderProcessingRouter);"));

        assert!(transformed_code.contains("import { orderProcessingRouter }"));

        assert!(transformed_code.contains("\"./api/routes/orderProcessing.routes\""));
    }

    #[test]
    fn test_transform_server_ts_preserves_existing_imports_and_exports() {
        let server_content = r#"import { forklaunchExpress, schemaValidator } from '@forklaunch/blueprint-core';
import { getEnvVar } from '@forklaunch/common';
import dotenv from 'dotenv';
import { organizationRouter } from './api/routes/organization.routes';
import { permissionRouter } from './api/routes/permission.routes';
import { roleRouter } from './api/routes/role.routes';
import { userRouter } from './api/routes/user.routes';
import { createDependencyContainer } from './registrations';

//! bootstrap resources and config
const envFilePath = getEnvVar('DOTENV_FILE_PATH');
dotenv.config({ path: envFilePath });
export const { ci, tokens } = createDependencyContainer(envFilePath);
export type ScopeFactory = typeof ci.createScope;

//! resolves the openTelemetryCollector from the configuration
const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

//! creates an instance of forklaunchExpress
const app = forklaunchExpress(schemaValidator, openTelemetryCollector);

//! resolves the host, port, and version from the configuration
const host = ci.resolve(tokens.HOST);
const port = ci.resolve(tokens.PORT);
const version = ci.resolve(tokens.VERSION);
const docsPath = ci.resolve(tokens.DOCS_PATH);

//! mounts the routes to the app
app.use(organizationRouter);
app.use(permissionRouter);
app.use(roleRouter);
app.use(userRouter);

//! starts the server
app.listen(port, host, () => {
  openTelemetryCollector.info(
    `🎉 IAM Server is running at http://${host}:${port} 🎉.\nAn API reference can be accessed at http://${host}:${port}/api/${version}${docsPath}`
  );
});
"#;
        let temp_dir = create_temp_project_structure(server_content);
        let temp_path = temp_dir.path();
        let cache = RenderedTemplatesCache::new();

        let result = transform_server_ts(&cache, "newService", temp_path);

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        assert!(
            transformed_code
                .contains("import { newServiceRouter } from \"./api/routes/newService.routes\";")
        );
        assert!(transformed_code.contains("app.use(newServiceRouter);"));

        assert!(
            transformed_code.contains(
                "import { organizationRouter } from \"./api/routes/organization.routes\";"
            )
        );
        assert!(
            transformed_code
                .contains("import { permissionRouter } from \"./api/routes/permission.routes\";")
        );
        assert!(
            transformed_code.contains("import { roleRouter } from \"./api/routes/role.routes\";")
        );
        assert!(
            transformed_code.contains("import { userRouter } from \"./api/routes/user.routes\";")
        );
        assert!(transformed_code.contains("app.use(organizationRouter);"));
        assert!(transformed_code.contains("app.use(permissionRouter);"));
        assert!(transformed_code.contains("app.use(roleRouter);"));
        assert!(transformed_code.contains("app.use(userRouter);"));
    }

    #[test]
    fn test_transform_server_ts_with_missing_server_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path();
        let cache = RenderedTemplatesCache::new();

        let result = transform_server_ts(&cache, "testService", temp_path);

        assert!(result.is_err());
    }

    #[test]
    fn test_transform_server_ts_injection_order() {
        let server_content = create_test_server_ts();
        let temp_dir = create_temp_project_structure(server_content);
        let temp_path = temp_dir.path();
        let cache = RenderedTemplatesCache::new();

        let result = transform_server_ts(&cache, "testService", temp_path);

        assert!(result.is_ok());

        let transformed_code = result.unwrap();
        let lines: Vec<&str> = transformed_code.lines().collect();

        let import_line = lines
            .iter()
            .position(|&line| {
                line.contains(
                    "import { testServiceRouter } from \"./api/routes/testService.routes\";",
                )
            })
            .expect("Import injection not found");

        let app_use_line = lines
            .iter()
            .position(|&line| line.contains("app.use(testServiceRouter);"))
            .expect("App.use injection not found");

        assert!(
            import_line < app_use_line,
            "Import should be injected before app.use"
        );
    }

    #[test]
    fn test_transform_server_ts_with_watermark_issue() {
        let server_content = create_test_server_ts_with_watermark();
        let temp_dir = create_temp_project_structure(server_content);
        let temp_path = temp_dir.path();
        let cache = RenderedTemplatesCache::new();

        let result = transform_server_ts(&cache, "userManagement", temp_path);

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        assert!(transformed_code.contains(
            "import { userManagementRouter } from \"./api/routes/userManagement.routes\";"
        ));

        assert!(transformed_code.contains("app.use(userManagementRouter);"));

        assert!(transformed_code.contains("Generated by ForkLaunch"));
        assert!(transformed_code.contains("File: server.ts"));

        assert!(
            transformed_code.starts_with("/**")
                && transformed_code.contains("Generated by ForkLaunch")
        );
    }
}
