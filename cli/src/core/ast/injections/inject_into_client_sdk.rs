use anyhow::Result;
use convert_case::{Case, Casing};
use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::{Program, SourceType, Statement};

use crate::core::ast::{
    injections::inject_into_import_statement::inject_into_import_statement,
    parse_ast_program::parse_ast_program,
};

fn inject_into_universal_sdk_function<'a>(
    allocator: &'a Allocator,
    app_program_ast: &mut Program<'a>,
    camel_case_name: &str,
    pascal_case_name: &str,
) -> Result<()> {
    let export_text = format!(
        "export const {}SdkClient = universalSdk<{}SdkClient>;",
        camel_case_name, pascal_case_name
    );
    let export_program = parse_ast_program(
        allocator,
        allocator.alloc_str(&export_text),
        SourceType::ts(),
    );

    if let Some(Statement::ExportNamedDeclaration(export_stmt)) = export_program.body.first() {
        app_program_ast.body.push(Statement::ExportNamedDeclaration(
            export_stmt.clone_in(allocator),
        ));
    }

    Ok(())
}

pub enum ClientSdkSpecialCase {
    BetterAuth,
}

pub(crate) fn inject_into_client_sdk<'a>(
    allocator: &'a Allocator,
    app_program_ast: &mut Program<'a>,
    app_name: &str,
    name: &str,
    source_text: &str,
    special_case: Option<ClientSdkSpecialCase>,
) -> Result<()> {
    let kebab_app_name = &app_name.to_case(Case::Kebab);
    let camel_case_name = &name.to_case(Case::Camel);
    let pascal_case_name = &name.to_case(Case::Pascal);
    let kebab_case_name = &name.to_case(Case::Kebab);

    if let Some(ClientSdkSpecialCase::BetterAuth) = special_case {
        let import_text = format!(
            "import {{\n  BetterAuthConfig,\n  IamSdkClient as {}SdkClient\n}} from \"@{}/{}\";",
            pascal_case_name, kebab_app_name, kebab_case_name
        );
        let mut import_program = parse_ast_program(
            allocator,
            allocator.alloc_str(&import_text),
            SourceType::ts(),
        );

        inject_into_import_statement(
            app_program_ast,
            &mut import_program,
            &format!("@{kebab_app_name}/{kebab_case_name}"),
            source_text,
        )?;

        let additional_imports = [
            "import { universalSdk, RegistryOptions } from '@forklaunch/universal-sdk';",
            "import { createAuthClient } from 'better-auth/client';",
            "import { inferAdditionalFields } from 'better-auth/client/plugins';",
        ];

        for import_text in &additional_imports {
            let import_program = parse_ast_program(
                allocator,
                allocator.alloc_str(import_text),
                SourceType::ts(),
            );
            if let Some(Statement::ImportDeclaration(import_stmt)) = import_program.body.first() {
                app_program_ast.body.push(Statement::ImportDeclaration(
                    import_stmt.clone_in(allocator),
                ));
            }
        }

        let export_text = format!(
            "export const {}SdkClient = async ({{\n  host,\n  registryOptions\n}}: {{\n  host: string;\n  registryOptions: RegistryOptions;\n}}) => ({{\n  core: await universalSdk<{}SdkClient>({{\n    host,\n    registryOptions: registryOptions\n  }}),\n  betterAuth: createAuthClient({{\n    baseURL: host,\n    fetchOptions: {{ headers: {{ origin: new URL(host).origin }} }},\n    plugins: [inferAdditionalFields<BetterAuthConfig>()]\n  }})\n}});",
            camel_case_name, pascal_case_name
        );
        let export_program = parse_ast_program(
            allocator,
            allocator.alloc_str(&export_text),
            SourceType::ts(),
        );

        if let Some(Statement::ExportNamedDeclaration(export_stmt)) = export_program.body.first() {
            app_program_ast.body.push(Statement::ExportNamedDeclaration(
                export_stmt.clone_in(allocator),
            ));
        }
    } else {
        let import_text = format!(
            "import {{ {}SdkClient }} from \"@{}/{}\";",
            pascal_case_name, kebab_app_name, kebab_case_name
        );
        let mut import_program = parse_ast_program(
            allocator,
            allocator.alloc_str(&import_text),
            SourceType::ts(),
        );

        inject_into_import_statement(
            app_program_ast,
            &mut import_program,
            &format!("@{kebab_app_name}/{kebab_case_name}"),
            source_text,
        )?;
        inject_into_universal_sdk_function(
            allocator,
            app_program_ast,
            camel_case_name,
            pascal_case_name,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_ast::ast::SourceType;
    use oxc_codegen::{Codegen, CodegenOptions};

    use super::*;
    use crate::core::ast::parse_ast_program::parse_ast_program;

    #[test]
    fn test_inject_into_client_sdk_function() {
        let allocator = Allocator::default();

        let app_code = r#"
        import { BillingSdkClient } from '@forklaunch/blueprint-billing-base';
        import { universalSdk } from '@forklaunch/universal-sdk';

        export const billingSdkClient = universalSdk<BillingSdkClient>;
        "#;
        let mut app_program = parse_ast_program(&allocator, app_code, SourceType::ts());

        let result = inject_into_client_sdk(
            &allocator,
            &mut app_program,
            "forklaunch",
            "user-service",
            app_code,
            None,
        );

        assert!(result.is_ok());

        let expected_code = "import { BillingSdkClient } from \"@forklaunch/blueprint-billing-base\";\nimport { universalSdk } from \"@forklaunch/universal-sdk\";\nimport { UserServiceSdkClient } from \"@forklaunch/user-service\";\nexport const billingSdkClient = universalSdk<BillingSdkClient>;\nexport const userServiceSdkClient = universalSdk<UserServiceSdkClient>;\n";

        assert_eq!(
            Codegen::new()
                .with_options(CodegenOptions::default())
                .build(&app_program)
                .code,
            expected_code
        );
    }

    #[test]
    fn test_inject_into_client_sdk_with_better_auth() {
        let allocator = Allocator::default();

        let app_code = r#"
        import { BillingSdkClient } from '@forklaunch/blueprint-billing-base';
        import { universalSdk } from '@forklaunch/universal-sdk';

        export const billingSdkClient = universalSdk<BillingSdkClient>;
        "#;
        let mut app_program = parse_ast_program(&allocator, app_code, SourceType::ts());

        let result = inject_into_client_sdk(
            &allocator,
            &mut app_program,
            "forklaunch",
            "iam",
            app_code,
            Some(ClientSdkSpecialCase::BetterAuth),
        );

        assert!(result.is_ok());

        let expected_code = "import { BillingSdkClient } from \"@forklaunch/blueprint-billing-base\";\nimport { BetterAuthConfig, IamSdkClient } from \"@forklaunch/iam\";\nimport { universalSdk } from \"@forklaunch/universal-sdk\";\nexport const billingSdkClient = universalSdk<BillingSdkClient>;\nimport { universalSdk, RegistryOptions } from \"@forklaunch/universal-sdk\";\nimport { createAuthClient } from \"better-auth/client\";\nimport { inferAdditionalFields } from \"better-auth/client/plugins\";\nexport const iamSdkClient = async ({ host, registryOptions }: {\n\thost: string;\n\tregistryOptions: RegistryOptions;\n}) => ({\n\tcore: await universalSdk<IamSdkClient>({\n\t\thost,\n\t\tregistryOptions\n\t}),\n\tbetterAuth: createAuthClient({\n\t\tbaseURL: host,\n\t\tfetchOptions: { headers: { origin: new URL(host).origin } },\n\t\tplugins: [inferAdditionalFields<BetterAuthConfig>()]\n\t})\n});\n";

        assert_eq!(
            Codegen::new()
                .with_options(CodegenOptions::default())
                .build(&app_program)
                .code,
            expected_code
        );
    }
}
