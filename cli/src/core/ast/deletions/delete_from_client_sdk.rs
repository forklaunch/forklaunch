use anyhow::Result;
use convert_case::{Case, Casing};
use oxc_allocator::Allocator;
use oxc_ast::ast::{BindingPattern, Declaration, Program, Statement};

use crate::core::ast::deletions::delete_import_statement::delete_import_statement;

fn delete_from_client_sdk_function<'a>(
    _allocator: &'a Allocator,
    app_program_ast: &mut Program<'a>,
    camel_case_name: &str,
) -> Result<()> {
    let mut statements_to_remove = Vec::new();

    for (index, stmt) in app_program_ast.body.iter().enumerate() {
        let export = match stmt {
            Statement::ExportDeclaration(export) => export,
            _ => continue,
        };

        let var_decl = match &export.declaration {
            Declaration::VariableDeclaration(var_decl) => var_decl,
            _ => continue,
        };

        if let Some(declarator) = var_decl.declarations.first() {
            if let BindingPattern::BindingIdentifier(var_name) = &declarator.id {
                if var_name.name == format!("{}SdkClient", camel_case_name).as_str() {
                    statements_to_remove.push(index);
                }
            }
        }
    }

    for &index in statements_to_remove.iter().rev() {
        app_program_ast.body.remove(index);
    }

    Ok(())
}

pub(crate) fn delete_from_client_sdk<'a>(
    allocator: &'a Allocator,
    app_program_ast: &mut Program<'a>,
    app_name: &str,
    name: &str,
) -> Result<()> {
    let kebab_app_name = &app_name.to_case(Case::Kebab);
    let camel_case_name = &name.to_case(Case::Camel);
    let kebab_case_name = &name.to_case(Case::Kebab);

    let import_source = format!("@{}/{}", kebab_app_name, kebab_case_name);
    delete_import_statement(allocator, app_program_ast, &import_source)?;

    let blueprint_import_source =
        format!("@{}/{}{}", kebab_app_name, "blueprint-", kebab_case_name);
    delete_import_statement(allocator, app_program_ast, &blueprint_import_source)?;

    delete_from_client_sdk_function(allocator, app_program_ast, camel_case_name)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_ast::ast::SourceType;
    use oxc_codegen::{Codegen, CodegenOptions};

    use crate::core::ast::{
        deletions::delete_from_client_sdk::delete_from_client_sdk,
        parse_ast_program::parse_ast_program,
    };

    #[test]
    fn test_delete_from_client_sdk_function() {
        let allocator = Allocator::default();

        let app_code = r#"
        import { BillingSdkClient } from '@forklaunch/blueprint-billing-base';
        import { IamSdkClient } from '@forklaunch/blueprint-iam-base';
        import { UserServiceSdkClient } from '@forklaunch/blueprint-user-service';
        import { universalSdk } from '@forklaunch/universal-sdk';

        export const billingSdkClient: typeof universalSdk<BillingSdkClient> = universalSdk<BillingSdkClient>;
        export const iamSdkClient: typeof universalSdk<IamSdkClient> = universalSdk<IamSdkClient>;
        export const userServiceSdkClient: typeof universalSdk<UserServiceSdkClient> = universalSdk<UserServiceSdkClient>;
        "#;
        let mut app_program = parse_ast_program(&allocator, app_code, SourceType::ts());

        let result =
            delete_from_client_sdk(&allocator, &mut app_program, "forklaunch", "user-service");

        assert!(result.is_ok());

        let expected_code = "import { BillingSdkClient } from \"@forklaunch/blueprint-billing-base\";\nimport { IamSdkClient } from \"@forklaunch/blueprint-iam-base\";\nimport { universalSdk } from \"@forklaunch/universal-sdk\";\nexport const billingSdkClient: typeof universalSdk<BillingSdkClient> = universalSdk<BillingSdkClient>;\nexport const iamSdkClient: typeof universalSdk<IamSdkClient> = universalSdk<IamSdkClient>;\n";

        assert_eq!(
            Codegen::new()
                .with_options(CodegenOptions::default())
                .build(&app_program)
                .code,
            expected_code
        );
    }

    #[test]
    fn test_delete_from_client_sdk_with_better_auth() {
        let allocator = Allocator::default();

        let app_code = r#"
        import { BillingSdkClient } from '@forklaunch/blueprint-billing-base';
        import { IamSdkClient } from '@forklaunch/blueprint-iam-base';
        import {
          BetterAuthConfig,
          IamSdkClient as IamBetterAuthSdkClient
        } from '@forklaunch/blueprint-iam-better-auth';
        import { universalSdk, RegistryOptions } from '@forklaunch/universal-sdk';
        import { createAuthClient } from 'better-auth/client';
        import { inferAdditionalFields } from 'better-auth/client/plugins';

        export const billingSdkClient: typeof universalSdk<BillingSdkClient> = universalSdk<BillingSdkClient>;
        export const iamSdkClient: typeof universalSdk<IamSdkClient> = universalSdk<IamSdkClient>;
        export const iamBetterAuthSdkClient = async ({
          host,
          registryOptions
        }: {
          host: string;
          registryOptions: RegistryOptions;
        }) => ({
          core: await universalSdk<IamBetterAuthSdkClient>({
            host,
            registryOptions: registryOptions
          }),
          betterAuth: createAuthClient({
            baseURL: host,
            plugins: [inferAdditionalFields<BetterAuthConfig>()]
          })
        });
        "#;
        let mut app_program = parse_ast_program(&allocator, app_code, SourceType::ts());

        let result = delete_from_client_sdk(
            &allocator,
            &mut app_program,
            "forklaunch",
            "iam-better-auth",
        );

        assert!(result.is_ok());

        let expected_code = "import { BillingSdkClient } from \"@forklaunch/blueprint-billing-base\";\nimport { IamSdkClient } from \"@forklaunch/blueprint-iam-base\";\nimport { universalSdk, RegistryOptions } from \"@forklaunch/universal-sdk\";\nimport { createAuthClient } from \"better-auth/client\";\nimport { inferAdditionalFields } from \"better-auth/client/plugins\";\nexport const billingSdkClient: typeof universalSdk<BillingSdkClient> = universalSdk<BillingSdkClient>;\nexport const iamSdkClient: typeof universalSdk<IamSdkClient> = universalSdk<IamSdkClient>;\n";

        assert_eq!(
            Codegen::new()
                .with_options(CodegenOptions::default())
                .build(&app_program)
                .code,
            expected_code
        );
    }
}
