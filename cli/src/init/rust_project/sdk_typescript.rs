//! Use the ordinary TypeScript client SDK with service-owned declaration packages.
use super::*;
use crate::core::{
    client_sdk::{add_project_to_client_sdk, remove_project_from_client_sdk},
    rendered_template::RenderedTemplatesCache,
};
use std::collections::BTreeSet;

const START: &str = "// <forklaunch:service-sdk-types>";
const END: &str = "// </forklaunch:service-sdk-types>";

pub(super) fn reconcile(root: &Path) -> Result<()> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let modules = root.join(manifest.get("modules_path").and_then(Value::as_str).unwrap_or("src"));
    let sdk = modules.join("client-sdk");
    if !sdk.join("clientSdk.ts").is_file() || !sdk.join("package.json").is_file() {
        return Ok(());
    }
    let app_name = manifest["app_name"]
        .as_str()
        .context("Missing application name")?;
    let mut services = BTreeSet::new();
    for project in manifest["projects"].as_array().into_iter().flatten() {
        if project
            .get("metadata")
            .and_then(|metadata| metadata.get("language"))
            .and_then(Value::as_str)
            != Some("rust")
        {
            continue;
        }
        let Some(name) = project["name"].as_str() else {
            continue;
        };
        let Some(relative) = crate_path_for_project(&manifest, name) else {
            continue;
        };
        if root
            .join(relative)
            .join("sdk/typescript/package.json")
            .is_file()
        {
            services.insert(name.to_owned());
        }
    }
    // The generated export region records which services were registered.
    let previous = read_to_string(sdk.join("index.ts"))
        .map(|source| registered_services(&source))
        .unwrap_or_default();
    if services.is_empty() && previous.is_empty() {
        return Ok(());
    }
    let mut cache = RenderedTemplatesCache::new();
    for name in previous.difference(&services) {
        remove_project_from_client_sdk(&mut cache, &modules, app_name, name)?;
    }
    for name in &services {
        let source = cache
            .get(sdk.join("clientSdk.ts"))?
            .context("Client SDK source")?
            .content;
        let factory = format!("export const {}SdkClient", name.to_case(Case::Camel));
        if !source.contains(&factory) {
            add_project_to_client_sdk(&mut cache, &modules, app_name, name, None)?;
        }
    }
    let rendered = cache.drain().map(|(_, template)| template).collect();
    write_rendered_templates(
        &rendered,
        false,
        &mut StandardStream::stdout(ColorChoice::Always),
    )?;

    // Only the generated export region is owned here; application exports stay intact.
    let index = sdk.join("index.ts");
    let current = read_to_string(&index)?;
    let generated = services
        .iter()
        .map(|name| {
            format!(
                "export type {{ {}SdkClient }} from '@{}/{}';\n",
                name.to_case(Case::Pascal),
                app_name.to_case(Case::Kebab),
                name
            )
        })
        .collect::<String>();
    let region = format!("{START}\n{generated}{END}");
    let updated = if let Some((before, rest)) = current.split_once(START) {
        let (_, after) = rest
            .split_once(END)
            .context("Incomplete service SDK export region")?;
        format!("{before}{region}{after}")
    } else {
        format!("{}\n\n{region}\n", current.trim_end())
    };
    if updated != current {
        write(index, updated)?;
    }
    crate::sdk::include_typescript_sdk(root)?;
    Ok(())
}

/// Services named in the generated export region of the client SDK index.
fn registered_services(source: &str) -> BTreeSet<String> {
    let Some((_, rest)) = source.split_once(START) else {
        return BTreeSet::new();
    };
    let region = rest.split_once(END).map_or(rest, |(region, _)| region);
    region
        .lines()
        .filter_map(|line| line.trim().strip_suffix("';")?.rsplit_once('/'))
        .map(|(_, name)| name.to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_manifest_without_modules_path_or_typescript_sdk_is_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        create_dir_all(temp.path().join(".forklaunch")).unwrap();
        let path = temp.path().join(".forklaunch/manifest.toml");
        let manifest = "app_name='legacy'\nlanguage='rust'\nruntime='tokio'\nprojects=[]\n";
        write(&path, manifest).unwrap();
        reconcile(temp.path()).unwrap();
        assert_eq!(read_to_string(path).unwrap(), manifest);
        assert!(!temp.path().join("src").exists());
    }

    #[test]
    fn first_native_service_registers_type_only_aggregate_without_root_package() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for relative in [".forklaunch", "src/client-sdk", "src/notes/sdk/typescript"] {
            create_dir_all(root.join(relative)).unwrap();
        }
        write(root.join(".forklaunch/manifest.toml"), "app_name='sample'\nmodules_path='src'\nlanguage='mixed'\nruntime='node'\nprojects=[{name='core',type='Library'},{name='notes',type='Service',metadata={language='rust',cratePath='src/notes',sdkLayout='project-local-v1'}}]\n").unwrap();
        write(root.join("src/package.json"), "{\"private\":true}").unwrap();
        write(
            root.join("src/pnpm-workspace.yaml"),
            "packages: [client-sdk]\noverrides: { '@forklaunch/universal-sdk': '1.2.29' }\n",
        )
        .unwrap();
        write(
            root.join("src/client-sdk/clientSdk.ts"),
            "import { universalSdk } from '@forklaunch/universal-sdk';\n",
        )
        .unwrap();
        write(
            root.join("src/client-sdk/index.ts"),
            "export * from './clientSdk';\n",
        )
        .unwrap();
        write(
            root.join("src/client-sdk/package.json"),
            "{\"name\":\"@sample/client-sdk\",\"version\":\"0.1.0\",\"type\":\"module\"}",
        )
        .unwrap();
        write(
            root.join("src/notes/sdk/typescript/package.json"),
            "{\"name\":\"@sample/notes\"}",
        )
        .unwrap();
        super::super::ensure_rust_package_scripts(root, RustProjectKind::Service, "notes", false)
            .unwrap();
        let source = read_to_string(root.join("src/client-sdk/clientSdk.ts")).unwrap();
        assert!(source.contains("import type { NotesSdkClient }"));
        assert!(source.contains("export const notesSdkClient"));
        let package: JsonValue = serde_json::from_str(
            &read_to_string(root.join("src/client-sdk/package.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(package["devDependencies"]["@sample/notes"], "workspace:*");
        assert!(package["dependencies"].get("@sample/notes").is_none());
        for directory in [root.to_owned(), root.join("src")] {
            let workspace: JsonValue = serde_yml::from_str(
                &read_to_string(directory.join("pnpm-workspace.yaml")).unwrap(),
            )
            .unwrap();
            // The published runtime is used; nothing is vendored or overridden.
            assert_ne!(
                workspace["overrides"]["@forklaunch/universal-sdk"],
                "workspace:*"
            );
        }
        assert!(!root.join("src/.forklaunch/universal-sdk").exists());
        assert_eq!(
            registered_services(&read_to_string(root.join("src/client-sdk/index.ts")).unwrap()),
            BTreeSet::from(["notes".to_owned()])
        );
        let root_package: JsonValue =
            serde_json::from_str(&read_to_string(root.join("package.json")).unwrap()).unwrap();
        assert!(
            root_package["scripts"]["sdk:typecheck"]
                .as_str()
                .unwrap()
                .contains("./src/*/sdk/typescript")
        );
        super::super::ensure_rust_package_scripts(root, RustProjectKind::Service, "notes", false)
            .unwrap();
        assert_eq!(
            read_to_string(root.join("src/client-sdk/clientSdk.ts")).unwrap(),
            source
        );
    }
}
