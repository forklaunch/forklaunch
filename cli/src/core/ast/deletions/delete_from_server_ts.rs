use anyhow::{Result, bail};
use convert_case::{Case, Casing};
use oxc_allocator::Allocator;
use oxc_ast::ast::{Argument, Expression, Program, Statement};
use oxc_codegen::{Codegen, CodegenOptions};

use super::delete_import_statement::delete_import_statement;

pub(crate) fn delete_from_server_ts<'a, F>(
    server_program_ast: &mut Program<'a>,
    server_ts_injection_pos: F,
) -> Result<()>
where
    F: Fn(&oxc_allocator::Vec<'a, Statement>) -> Option<usize>,
{
    if let Some(pos) = server_ts_injection_pos(&server_program_ast.body) {
        server_program_ast.body.remove(pos);
        return Ok(());
    }
    // Match the generated async startup shape supported by router injection.
    for statement in &mut server_program_ast.body {
        if let Statement::FunctionDeclaration(function) = statement
            && let Some(body) = function.body.as_mut()
            && let Some(pos) = server_ts_injection_pos(&body.statements)
        {
            body.statements.remove(pos);
            return Ok(());
        }
    }
    bail!("Failed to delete from server.ts")
}

pub(crate) fn delete_from_server_ts_router<'a>(
    allocator: &'a Allocator,
    server_program: &mut Program<'a>,
    router_name: &str,
) -> Result<String> {
    let router_name_camel_case = router_name.to_case(Case::Camel);

    delete_from_server_ts(server_program, |statements| {
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

            let arg = match call.arguments.first() {
                Some(Argument::Identifier(arg)) => Some(arg),
                _ => None,
            };

            if id.name == "app" && member.property.name == "use" {
                if let Some(arg) = arg {
                    if &arg.name == format!("{router_name_camel_case}Router").as_str() {
                        maybe_splice_pos = Some(index);
                    }
                }
            }
        });
        maybe_splice_pos
    })?;

    let _ = delete_import_statement(
        allocator,
        server_program,
        format!("./api/routes/{router_name_camel_case}.routes").as_str(),
    )?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&server_program)
        .code)
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_ast::ast::SourceType;
    use oxc_codegen::{Codegen, CodegenOptions};

    use super::*;
    use crate::core::ast::parse_ast_program::parse_ast_program;

    #[test]
    fn deletes_router_inside_async_startup_and_preserves_unrelated_calls() {
        let allocator = Allocator::default();
        let source = r#"import { profilesRouter } from './api/routes/profiles.routes';
async function startServer() {
  const app = createApp();
  app.use();
  app.use(otherRouter);
  app.use(profilesRouter);
  app.listen();
}
startServer();"#;
        let mut program = parse_ast_program(&allocator, source, SourceType::ts());
        let output = delete_from_server_ts_router(&allocator, &mut program, "profiles").unwrap();
        assert!(!output.contains("profilesRouter"));
        assert!(output.contains("app.use(otherRouter)"));
        assert!(output.contains("app.use()"));
        assert!(output.contains("app.listen()"));
        assert!(output.contains("async function startServer()"));
    }

    fn create_test_server_ts_with_multiple_routers() -> &'static str {
        r#"import express from 'express';
import { ci } from './registrations';
import { openTelemetryCollector } from '@forklaunch/core';
import { UserRouter } from './api/routes/user.routes';
import { OrderRouter } from './api/routes/order.routes';
import { ProductRouter } from './api/routes/product.routes';

const app = express();

const port = process.env.PORT || 3000;
const UserServiceFactory = ci.scopedResolver(tokens.UserService);
const OrderServiceFactory = ci.scopedResolver(tokens.OrderService);
const ProductServiceFactory = ci.scopedResolver(tokens.ProductService);

app.use(userRouter);
app.use(orderRouter);
app.use(productRouter);

app.listen(port, () => {});
"#
    }

    fn create_test_server_ts_single_router() -> &'static str {
        r#"import express from 'express';
import { ci } from './registrations';
import { openTelemetryCollector } from '@forklaunch/core';
import { UserRouter } from './api/routes/user.routes';

const app = express();

const port = process.env.PORT || 3000;
const UserServiceFactory = ci.scopedResolver(tokens.UserService);

app.use(userRouter);

app.listen(port, () => {});
"#
    }

    #[test]
    fn test_delete_from_server_ts_router_successful_deletion() {
        let allocator = Allocator::default();
        let server_content = create_test_server_ts_with_multiple_routers();
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result = delete_from_server_ts_router(&allocator, &mut server_program, "order");

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("app.use(orderRouter);"));
        assert!(
            !transformed_code
                .contains("import { OrderRouter } from \"./api/routes/order.routes\";")
        );

        assert!(
            transformed_code
                .contains("const UserServiceFactory = ci.scopedResolver(tokens.UserService);")
        );
        assert!(
            transformed_code.contains(
                "const ProductServiceFactory = ci.scopedResolver(tokens.ProductService);"
            )
        );
        assert!(transformed_code.contains("app.use(userRouter);"));
        assert!(transformed_code.contains("app.use(productRouter);"));
        assert!(
            transformed_code.contains("import { UserRouter } from \"./api/routes/user.routes\";")
        );
        assert!(
            transformed_code
                .contains("import { ProductRouter } from \"./api/routes/product.routes\";")
        );

        assert!(transformed_code.contains("import express from \"express\";"));
        assert!(transformed_code.contains("const app = express();"));
        assert!(transformed_code.contains("app.listen(port"));
    }

    #[test]
    fn test_delete_from_server_ts_router_with_pascal_case_name() {
        let allocator = Allocator::default();
        let server_content = r#"import express from 'express';
import { ci } from './registrations';
import { openTelemetryCollector } from '@forklaunch/core';
import { UserManagementRouter } from './api/routes/userManagement.routes';

const app = express();

const port = process.env.PORT || 3000;

const UserManagementServiceFactory = ci.scopedResolver(tokens.UserManagementService);

app.use(userManagementRouter);
app.listen(port, () => {});
"#;
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result =
            delete_from_server_ts_router(&allocator, &mut server_program, "userManagement");

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("app.use(userManagementRouter);"));
        assert!(!transformed_code.contains("import { UserManagementRouter }"));

        assert!(transformed_code.contains("import express from \"express\";"));
        assert!(transformed_code.contains("const app = express();"));
        assert!(transformed_code.contains("app.listen(port"));
    }

    #[test]
    fn test_delete_from_server_ts_router_with_kebab_case_name() {
        let allocator = Allocator::default();
        let server_content = r#"import express from 'express';
import { ci } from './registrations';
import { openTelemetryCollector } from '@forklaunch/core';
import { OrderProcessingRouter } from './api/routes/orderProcessing.routes';

const app = express();

const port = process.env.PORT || 3000;

const OrderProcessingServiceFactory = ci.scopedResolver(tokens.OrderProcessingService);

app.use(orderProcessingRouter);

app.listen(port, () => {});
"#;
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result =
            delete_from_server_ts_router(&allocator, &mut server_program, "order-processing");

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("app.use(orderProcessingRouter);"));
        assert!(!transformed_code.contains("import { OrderProcessingRouter }"));

        assert!(transformed_code.contains("import express from \"express\";"));
        assert!(transformed_code.contains("const app = express();"));
        assert!(transformed_code.contains("app.listen(port"));
    }

    #[test]
    fn test_delete_from_server_ts_router_last_router() {
        let allocator = Allocator::default();
        let server_content = create_test_server_ts_single_router();
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result = delete_from_server_ts_router(&allocator, &mut server_program, "user");

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("app.use(userRouter);"));
        assert!(!transformed_code.contains("import { UserRouter }"));

        assert!(transformed_code.contains("import express from \"express\";"));
        assert!(transformed_code.contains("const app = express();"));
        assert!(transformed_code.contains("app.listen(port"));
    }

    #[test]
    fn test_delete_from_server_ts_router_nonexistent_router() {
        let allocator = Allocator::default();
        let server_content = create_test_server_ts_with_multiple_routers();
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result = delete_from_server_ts_router(&allocator, &mut server_program, "nonexistent");

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to delete from server.ts")
        );
    }

    #[test]
    fn test_delete_from_server_ts_router_preserves_order() {
        let allocator = Allocator::default();
        let server_content = create_test_server_ts_with_multiple_routers();
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result = delete_from_server_ts_router(&allocator, &mut server_program, "order");

        assert!(result.is_ok());

        let transformed_code = result.unwrap();
        let lines: Vec<&str> = transformed_code.lines().collect();

        let user_factory_line = lines.iter().position(|&line| {
            line.contains("const UserServiceFactory = ci.scopedResolver(tokens.UserService);")
        });
        let product_factory_line = lines.iter().position(|&line| {
            line.contains("const ProductServiceFactory = ci.scopedResolver(tokens.ProductService);")
        });
        let user_routes_line = lines
            .iter()
            .position(|&line| line.contains("app.use(userRouter);"));
        let product_routes_line = lines
            .iter()
            .position(|&line| line.contains("app.use(productRouter);"));

        assert!(user_factory_line.is_some());
        assert!(product_factory_line.is_some());
        assert!(user_routes_line.is_some());
        assert!(product_routes_line.is_some());

        assert!(user_factory_line.unwrap() < product_factory_line.unwrap());
        assert!(user_routes_line.unwrap() < product_routes_line.unwrap());
    }

    #[test]
    fn test_delete_from_server_ts_router_with_middleware() {
        let allocator = Allocator::default();
        let server_content = r#"import express from 'express';
import { ci } from './registrations';
import { openTelemetryCollector } from '@forklaunch/core';
import { UserRouter } from './api/routes/user.routes';
import { OrderRouter } from './api/routes/order.routes';

const app = express();

const port = process.env.PORT || 3000;
const UserServiceFactory = ci.scopedResolver(tokens.UserService);
const OrderServiceFactory = ci.scopedResolver(tokens.OrderService);

const userRouter = UserRouter(() => ci.createScope(), UserServiceFactory, openTelemetryCollector);
const orderRouter = OrderRouter(() => ci.createScope(), OrderServiceFactory, openTelemetryCollector);

// Middleware
app.use(express.json());
app.use(express.urlencoded({ extended: true }));

app.use(userRouter);
app.use(orderRouter);

app.listen(port, () => {});
"#;
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result = delete_from_server_ts_router(&allocator, &mut server_program, "order");

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("app.use(orderRouter);"));
        assert!(!transformed_code.contains("import { OrderRouter }"));

        assert!(transformed_code.contains("app.use(userRouter);"));
        assert!(transformed_code.contains("app.use(express.json());"));
        assert!(transformed_code.contains("app.use(express.urlencoded({ extended: true }));"));
    }

    #[test]
    fn test_delete_from_server_ts_router_with_comments() {
        let allocator = Allocator::default();
        let server_content = r#"import express from 'express';
import { ci } from './registrations';
import { openTelemetryCollector } from '@forklaunch/core';
import { UserRoutes } from './api/routes/user.routes';
import { OrderRoutes } from './api/routes/order.routes';

const app = express();

const port = process.env.PORT || 3000;

// User service factory
const UserServiceFactory = ci.scopedResolver(tokens.UserService);

// Order service factory
const OrderServiceFactory = ci.scopedResolver(tokens.OrderService);

// Export routes
export const userRouter = UserRoutes(() => ci.createScope(), UserServiceFactory, openTelemetryCollector);
export const orderRouter = OrderRoutes(() => ci.createScope(), OrderServiceFactory, openTelemetryCollector);

// Use routes
app.use(userRouter);
app.use(orderRouter);

app.listen(port, () => {});
"#;
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result = delete_from_server_ts_router(&allocator, &mut server_program, "order");

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("app.use(orderRouter);"));

        assert!(transformed_code.contains("app.use(userRouter);"));
    }

    #[test]
    fn test_delete_from_server_ts_router_with_multiple_uses() {
        let allocator = Allocator::default();
        let server_content = r#"import express from 'express';
import { ci } from './registrations';
import { openTelemetryCollector } from '@forklaunch/core';
import { UserRouter } from './api/routes/user.routes';

const app = express();

const port = process.env.PORT || 3000;
const UserServiceFactory = ci.scopedResolver(tokens.UserService);

app.use(userRouter);
app.use('/api', userRouter);
app.use('/v1', userRouter);

app.listen(port, () => {});
"#;
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result = delete_from_server_ts_router(&allocator, &mut server_program, "user");

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        assert!(!transformed_code.contains("app.use(userRouter);"));
        assert!(!transformed_code.contains("app.use('/api', userRouter);"));
        assert!(!transformed_code.contains("app.use('/v1', userRouter);"));
        assert!(!transformed_code.contains("import { UserRouter }"));

        assert!(transformed_code.contains("import express from \"express\";"));
        assert!(transformed_code.contains("const app = express();"));
        assert!(transformed_code.contains("app.listen(port"));
    }

    #[test]
    fn test_delete_from_server_ts_generic_function() {
        let allocator = Allocator::default();
        let server_content = r#"import express from 'express';

const app = express();

const firstStatement = "first";
const secondStatement = "second";
const thirdStatement = "third";

app.listen(3000, () => {});
"#;
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result = delete_from_server_ts(&mut server_program, |statements| {
            statements.iter().enumerate().find_map(|(index, stmt)| {
                if let Statement::VariableDeclaration(expr) = stmt {
                    if let Some(id_name) = expr.declarations[0].id.get_identifier_name() {
                        if id_name == "secondStatement" {
                            return Some(index);
                        }
                    }
                }
                None
            })
        });

        assert!(result.is_ok());

        let transformed_code = Codegen::new()
            .with_options(CodegenOptions::default())
            .build(&server_program)
            .code;

        assert!(!transformed_code.contains("const secondStatement"));

        assert!(transformed_code.contains("const firstStatement"));
        assert!(transformed_code.contains("const thirdStatement"));
    }

    #[test]
    fn test_delete_from_server_ts_no_match() {
        let allocator = Allocator::default();
        let server_content = r#"import express from 'express';

const app = express();

const statement = "test";

app.listen(3000, () => {});
"#;
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result = delete_from_server_ts(&mut server_program, |_statements| None);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to delete from server.ts")
        );
    }

    #[test]
    fn test_delete_from_server_ts_valid_deletion() {
        let allocator = Allocator::default();
        let server_content = r#"import express from 'express';

const app = express();
const port = 3000;
"#;
        let mut server_program = parse_ast_program(&allocator, server_content, SourceType::ts());

        let result = delete_from_server_ts(&mut server_program, |_statements| Some(1));

        assert!(result.is_ok());

        let transformed_code = Codegen::new()
            .with_options(CodegenOptions::default())
            .build(&server_program)
            .code;

        assert!(!transformed_code.contains("const app = express();"));
        assert!(transformed_code.contains("import express from \"express\";"));
        assert!(transformed_code.contains("const port = 3e3;"));
    }
}
