//! Bounded syntactic router assembly, never a runtime authorization attestation.
mod resolution;
mod syntax;
use anyhow::{Result, ensure};
use forklaunch_core::build::provenance::BindingManifest;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use syn::{Item, ItemFn};

#[derive(Clone, Default)]
struct Assembly {
    groups: BTreeSet<String>,
    raw: Vec<RawRoute>,
    finished: bool,
    application: bool,
}
#[derive(Clone)]
struct RawRoute {
    method: String,
    path: String,
    source: PathBuf,
}
#[derive(Clone)]
enum Value {
    Router(Assembly),
    Bindings(String),
    Unknown,
}
struct Function {
    function: ItemFn,
    source: PathBuf,
}
struct Verifier {
    resolver: resolution::Resolver,
    group_sources: BTreeMap<(PathBuf, String), String>,
    functions: BTreeMap<String, Vec<Function>>,
    groups: BTreeMap<Vec<String>, String>,
    active: BTreeSet<String>,
}

impl Verifier {
    fn load(directory: &Path, manifest: &BindingManifest) -> Result<Self> {
        let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for item in &manifest.declarations {
            grouped
                .entry(format!("{}::{}", item.source, item.group))
                .or_default()
                .push(item.member.clone());
        }
        let resolver = resolution::Resolver::new(directory)?;
        let mut group_sources = BTreeMap::new();
        for item in &manifest.declarations {
            let source = Path::new(&item.source);
            let source = if source.is_absolute() {
                source.to_path_buf()
            } else {
                directory.join(source)
            }
            .canonicalize()?;
            ensure!(
                resolver.owns_source(&source),
                "Declared binding source leaves service"
            );
            group_sources.insert(
                (source, item.group.clone()),
                format!("{}::{}", item.source, item.group),
            );
        }
        let mut groups = BTreeMap::new();
        for (group, mut members) in grouped {
            members.sort();
            ensure!(
                groups.insert(members, group).is_none(),
                "Ambiguous static binding field sets require resolved assembly metadata"
            );
        }
        let mut result = Self {
            resolver,
            group_sources,
            functions: BTreeMap::new(),
            groups,
            active: BTreeSet::new(),
        };
        result.scan(directory)?;
        Ok(result)
    }
    fn scan(&mut self, directory: &Path) -> Result<()> {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            if matches!(
                name.to_str(),
                Some("target" | "vendor" | "node_modules" | "__test__" | "tests" | "migrations")
            ) {
                continue;
            }
            ensure!(
                !entry.file_type()?.is_symlink(),
                "Source symlinks need explicit assembly resolution"
            );
            if path.is_dir() {
                self.scan(&path)?;
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = resolution::source_text(&path)?;
                ensure!(
                    source.len() <= 2 * 1024 * 1024,
                    "Assembly source exceeds limit"
                );
                let file = syn::parse_file(&source)?;
                self.items(&file.items, &path);
            }
        }
        Ok(())
    }
    fn items(&mut self, items: &[Item], source: &Path) {
        for item in items {
            match item {
                Item::Fn(function) if routing_return(&function.sig.output) => self
                    .functions
                    .entry(function.sig.ident.to_string())
                    .or_default()
                    .push(Function {
                        function: function.clone(),
                        source: source.into(),
                    }),
                Item::Mod(module)
                    if !module.attrs.iter().any(|attribute| {
                        attribute.path().segments.last().is_some_and(|segment| {
                            matches!(
                                segment.ident.to_string().as_str(),
                                "api" | "socket_api" | "cfg"
                            )
                        })
                    }) =>
                {
                    if let Some((_, items)) = &module.content {
                        self.items(items, source);
                    }
                }
                _ => {}
            }
        }
    }
    fn function(
        &mut self,
        name: &str,
        location: Option<&Path>,
        arguments: Vec<Value>,
    ) -> Result<Value> {
        let functions = self
            .functions
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unresolved assembly function {name}"))?;
        let functions: Vec<_> = functions
            .iter()
            .filter(|function| location.is_none_or(|path| function.source == path))
            .collect();
        ensure!(functions.len() == 1, "Ambiguous assembly function {name}");
        let function = functions[0].function.clone();
        ensure!(
            !function
                .attrs
                .iter()
                .any(|attribute| attribute.path().is_ident("cfg")
                    || attribute.path().is_ident("cfg_attr")),
            "Conditional assembly functions need explicit resolution"
        );
        let source = functions[0].source.clone();
        ensure!(
            self.active.insert(name.into()),
            "Recursive router assembly is unsupported"
        );
        let mut variables = BTreeMap::new();
        for (index, argument) in function.sig.inputs.iter().enumerate() {
            let syn::FnArg::Typed(argument) = argument else {
                anyhow::bail!("Receiver assembly functions unsupported")
            };
            let syn::Pat::Ident(name) = argument.pat.as_ref() else {
                anyhow::bail!("Assembly parameters must be named")
            };
            variables.insert(
                name.ident.to_string(),
                arguments.get(index).cloned().unwrap_or(Value::Unknown),
            );
        }
        let mut output = Value::Unknown;
        for statement in &function.block.stmts {
            match statement {
                syn::Stmt::Local(local) => {
                    if let (syn::Pat::Ident(name), Some(initializer)) = (&local.pat, &local.init) {
                        let value = self.expression(&initializer.expr, &mut variables, &source)?;
                        ensure!(
                            name.mutability.is_none() || matches!(value, Value::Unknown),
                            "Mutable router/binding locals need explicit dataflow resolution"
                        );
                        variables.insert(name.ident.to_string(), value);
                    } else {
                        anyhow::bail!("Unsupported assembly local pattern");
                    }
                }
                syn::Stmt::Expr(expression, None) => {
                    output = self.expression(expression, &mut variables, &source)?
                }
                // Unused statements cannot establish that a returned router mounts a binding.
                syn::Stmt::Expr(expression, Some(_)) => {
                    ensure!(
                        !matches!(expression, syn::Expr::Assign(_) | syn::Expr::Binary(_)),
                        "Router assembly mutation needs explicit resolution"
                    );
                    self.expression(expression, &mut variables, &source)?;
                }
                syn::Stmt::Item(_) => {
                    anyhow::bail!("Local item shadowing needs explicit resolution")
                }
                syn::Stmt::Macro(_) => {
                    anyhow::bail!("Router assembly macro needs explicit resolution")
                }
            }
        }
        self.active.remove(name);
        Ok(output)
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Policy {
    format_version: String,
    raw_routes: Vec<Exemption>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Exemption {
    method: String,
    path: String,
    source: String,
    owner: String,
    reason: String,
    expires: String,
}
fn verify_exemptions(root: &Path, project: &str, routes: &[RawRoute]) -> Result<()> {
    if routes.is_empty() {
        return Ok(());
    }
    let path = root.join(".forklaunch/analyze.json");
    ensure!(
        path.canonicalize()?.starts_with(root.canonicalize()?),
        "Analysis policy leaves workspace"
    );
    ensure!(
        std::fs::metadata(&path)?.len() <= 64 * 1024,
        "Analysis policy exceeds size bound"
    );
    let policy: Policy = serde_json::from_slice(&std::fs::read(path)?)?;
    ensure!(
        policy.format_version == "forklaunch.analysis.policy.v1",
        "Unsupported analysis policy"
    );
    for route in routes {
        let source = route.source.strip_prefix(root)?.to_string_lossy();
        let matches: Vec<_> = policy
            .raw_routes
            .iter()
            .filter(|item| {
                item.method == route.method && item.path == route.path && item.source == source
            })
            .collect();
        ensure!(
            matches.len() == 1,
            "Unmanaged route {} {} requires a unique explicit exemption at {}",
            route.method,
            route.path,
            source
        );
        let item = matches[0];
        ensure!(
            item.owner == project && !item.reason.trim().is_empty(),
            "Unmanaged route exemption needs owning project and reason"
        );
        let expires = chrono::NaiveDate::parse_from_str(&item.expires, "%Y-%m-%d")?;
        ensure!(
            expires >= chrono::Utc::now().date_naive(),
            "Unmanaged route exemption expired"
        );
    }
    Ok(())
}

pub(super) fn verify(
    root: &Path,
    directory: &Path,
    project: &str,
    manifest: &BindingManifest,
) -> Result<String> {
    let canonical_root = root.canonicalize()?;
    let canonical_directory = directory.canonicalize()?;
    let root = canonical_root.as_path();
    let mut verifier = Verifier::load(&canonical_directory, manifest)?;
    let entry = verifier.resolver.application_entry()?;
    let Value::Router(router) = verifier.function("http", Some(&entry), Vec::new())? else {
        anyhow::bail!("Generated application::http assembly is unresolved")
    };
    ensure!(
        router.application,
        "Assembly must reach the framework NativeApplication constructor"
    );
    ensure!(
        router.finished,
        "Application router lacks completed managed boundary"
    );
    let expected: BTreeSet<_> = verifier.groups.values().cloned().collect();
    if router.groups != expected {
        return Err(UnmountedOperation.into());
    }
    verify_exemptions(root, project, &router.raw)?;
    Ok(format!(
        "Syntactic assembly verified for {} compiled static operation groups; {} explicit unmanaged route exemptions. Supported scope: unique generated functions, direct static Bindings.mount/finish and Router.merge chains reaching NativeApplication::new. Compiler typechecking is required separately; runtime auth/encryption and execution are not attested.",
        router.groups.len(),
        router.raw.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(body: &str) -> (tempfile::TempDir, BindingManifest) {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("application.rs"),
            format!("use contract::notes; use axum::{{Router,routing::get}}; use framework::native::{{ContractRouter,NativeApplication}}; {body}"),
        )
        .unwrap();
        std::fs::write(root.path().join("lib.rs"), "pub mod application;").unwrap();
        std::fs::create_dir(root.path().join("domain")).unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[lib]\npath=\"lib.rs\"\n[dependencies]\ncontract={path=\"domain\"}\naxum=\"0.8\"\nframework={package=\"forklaunch-axum\",version=\"0.1\"}\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("domain/Cargo.toml"),
            "[lib]\npath=\"../contract.rs\"\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("contract.rs"),
            "#[framework::api] pub mod notes {pub fn list(){}} ",
        )
        .unwrap();
        let declarations = vec![forklaunch_core::contract::assembly::BindingDeclaration {
            source: "contract.rs".into(),
            group: "notes".into(),
            member: "list".into(),
            transport: "http".into(),
            contract: serde_json::json!({}),
        }];
        (
            root,
            BindingManifest {
                format_version: "forklaunch.bindings.v1".into(),
                declarations,
            },
        )
    }
    #[test]
    fn private_host_compiles_service_local_contract_source_without_changing_crate_identity() {
        let workspace = tempfile::tempdir().unwrap();
        let service=workspace.path().join("src/notes");
        let host=service.join(".forklaunch/contracts");
        for path in [&host,&service.join("api/contracts"),&workspace.path().join(".forklaunch")] {std::fs::create_dir_all(path).unwrap();}
        std::fs::write(workspace.path().join(".forklaunch/manifest.toml"),"application='proof'").unwrap();
        std::fs::write(service.join("Cargo.toml"),"[lib]\npath='lib.rs'\n[dependencies]\ncontracts={path='.forklaunch/contracts'}\nforklaunch-axum='0.1'\n[build-dependencies]\ncontracts={path='.forklaunch/contracts'}\n").unwrap();
        std::fs::write(service.join("lib.rs"),"pub mod application;").unwrap();
        std::fs::write(service.join("application.rs"),"use contracts::notes; use forklaunch_axum::native::{Router,ContractRouter,NativeApplication}; fn http(){NativeApplication::new(Router::new().merge(notes::Bindings{list:notes::List::bind(handler)}.mount(ContractRouter::new(notes::registry())).unwrap().finish().unwrap()))}").unwrap();
        std::fs::write(host.join("Cargo.toml"),"[lib]\npath='lib.rs'\n").unwrap();
        std::fs::write(host.join("lib.rs"),"#[path=\"../../api/contracts/mod.rs\"] mod endpoints; pub use endpoints::*;").unwrap();
        std::fs::write(service.join("api/contracts/mod.rs"),"mod declarations; pub use crate::endpoints::declarations::notes;").unwrap();
        std::fs::write(service.join("api/contracts/declarations.rs"),"#[forklaunch_core::contract::api] pub mod notes {pub fn list(){}}").unwrap();
        let manifest=BindingManifest{format_version:"forklaunch.bindings.v1".into(), declarations:vec![forklaunch_core::contract::assembly::BindingDeclaration{source:"api/contracts/declarations.rs".into(),group:"notes".into(),member:"list".into(),transport:"http".into(),contract:serde_json::json!({})}]};
        verify(workspace.path(),&service,"notes",&manifest).unwrap();
    }

    #[test]
    fn inline_controller_contract_export_resolves_same_author_source() {
        let workspace = tempfile::tempdir().unwrap();
        let service=workspace.path().join("src/notes");
        let host=service.join(".forklaunch/contracts");
        for path in [&host,&service.join("api/controllers"),&workspace.path().join(".forklaunch")] {std::fs::create_dir_all(path).unwrap();}
        std::fs::write(workspace.path().join(".forklaunch/manifest.toml"),"application='proof'").unwrap();
        std::fs::write(service.join("Cargo.toml"),"[lib]\npath='lib.rs'\n[dependencies]\ncontracts={path='.forklaunch/contracts'}\nforklaunch-axum='0.1'\n[build-dependencies]\ncontracts={path='.forklaunch/contracts'}\n").unwrap();
        std::fs::write(service.join("lib.rs"),"pub mod application;pub mod api;").unwrap();
        std::fs::write(service.join("api/mod.rs"),"pub mod controllers;").unwrap();
        std::fs::write(service.join("api/controllers/mod.rs"),"pub mod notes;").unwrap();
        std::fs::write(service.join("application.rs"),"use crate::api::controllers::notes::contract; use forklaunch_axum::native::{Router,ContractRouter,NativeApplication}; fn http(){NativeApplication::new(Router::new().merge(contract::Bindings{list:contract::List::bind(handler)}.mount(ContractRouter::new(contracts::registry())).unwrap().finish().unwrap()))}").unwrap();
        std::fs::write(host.join("Cargo.toml"),"[lib]\npath='lib.rs'\n").unwrap();
        std::fs::write(host.join("lib.rs"),"forklaunch_core::contract::controller_contract!(notes,\"../../api/controllers/notes.rs\");").unwrap();
        std::fs::write(service.join("api/controllers/notes.rs"),"#[forklaunch_core::contract::api] pub mod contract {pub fn list(){}} fn handler(){}").unwrap();
        let manifest=BindingManifest{format_version:"forklaunch.bindings.v1".into(), declarations:vec![forklaunch_core::contract::assembly::BindingDeclaration{source:"api/controllers/notes.rs".into(),group:"contract".into(),member:"list".into(),transport:"http".into(),contract:serde_json::json!({})}]};
        verify(workspace.path(),&service,"notes",&manifest).unwrap();
        let resolver=resolution::Resolver::new(&service).unwrap();
        let resolved=resolver.binding(&service.join("application.rs"),&syn::parse_quote!(contracts::notes::contract::Bindings)).unwrap();
        assert_eq!(resolved.0,service.join("api/controllers/notes.rs").canonicalize().unwrap());
    }

    #[test]
    fn declared_sibling_contract_and_framework_router_are_resolved() {
        let workspace = tempfile::tempdir().unwrap();
        let service = workspace.path().join("src/notes");
        let host = workspace.path().join("src/notes-contracts");
        for path in [&service, &host, &workspace.path().join(".forklaunch")] {
            std::fs::create_dir_all(path).unwrap();
        }
        std::fs::write(
            workspace.path().join(".forklaunch/manifest.toml"),
            "application = 'proof'",
        )
        .unwrap();
        std::fs::write(service.join("Cargo.toml"), "[lib]\npath='lib.rs'\n[dependencies]\ncontracts={path='../notes-contracts'}\nforklaunch-axum='0.1'\n[build-dependencies]\ncontracts={path='../notes-contracts'}\n").unwrap();
        std::fs::write(service.join("lib.rs"), "pub mod application;").unwrap();
        std::fs::write(service.join("application.rs"), "use contracts::notes; use forklaunch_axum::native::{Router,ContractRouter,NativeApplication}; fn http(){NativeApplication::new(Router::new().merge(notes::Bindings{list:notes::List::bind(handler)}.mount(ContractRouter::new(notes::registry())).unwrap().finish().unwrap()))}").unwrap();
        std::fs::write(host.join("Cargo.toml"), "[lib]\npath='lib.rs'\n").unwrap();
        std::fs::write(host.join("lib.rs"), "mod endpoints; pub use endpoints::*;").unwrap();
        std::fs::write(
            host.join("endpoints.rs"),
            "#[forklaunch_core::contract::api] pub mod notes {pub fn list(){}}",
        )
        .unwrap();
        let mut manifest = BindingManifest {
            format_version: "forklaunch.bindings.v1".into(),
            declarations: vec![forklaunch_core::contract::assembly::BindingDeclaration {
                source: "../notes-contracts/endpoints.rs".into(),
                group: "notes".into(),
                member: "list".into(),
                transport: "http".into(),
                contract: serde_json::json!({}),
            }],
        };
        verify(workspace.path(), &service, "notes", &manifest).unwrap();
        std::fs::write(
            workspace.path().join("outside.rs"),
            "#[forklaunch_core::contract::api] pub mod notes {pub fn list(){}}",
        )
        .unwrap();
        manifest.declarations[0].source = "../../outside.rs".into();
        assert!(verify(workspace.path(), &service, "notes", &manifest).is_err());
        manifest.declarations[0].source = "../notes-contracts/endpoints.rs".into();
        let cargo = std::fs::read_to_string(service.join("Cargo.toml")).unwrap();
        std::fs::write(
            service.join("Cargo.toml"),
            cargo.replace(
                "[build-dependencies]\ncontracts={path='../notes-contracts'}",
                "[build-dependencies]\ncontracts={path='../other-contracts'}",
            ),
        )
        .unwrap();
        assert!(verify(workspace.path(), &service, "notes", &manifest).is_err());
    }
    #[test]
    fn returned_binding_chain_is_required_not_merely_present() {
        let (root, manifest) = fixture(
            "fn routes(){ notes::Bindings{list:notes::List::bind(handler)}.mount(ContractRouter::new(notes::registry())).unwrap().finish().unwrap() } fn http(){NativeApplication::new(Router::new().merge(routes()))}",
        );
        verify(root.path(), root.path(), "notes", &manifest).unwrap();
        for body in [
            "fn unused(){notes::Bindings{list: x}.mount(ContractRouter::new(registry())).unwrap().finish()} fn http(){NativeApplication::new(Router::new())}",
            "fn http(){if config {routes()} else {Router::new()}}",
            "fn http(){notes::Bindings{other: x}.mount(ContractRouter::new(registry())).unwrap().finish()}",
            "fn http(){NativeApplication::new(Router::new().merge(external!()))}",
            "fn http(){let mut router=Router::new(); mutate(&mut router); NativeApplication::new(router)}",
            "fn http(){let router=Router::new(); helper(&router); NativeApplication::new(router)}",
            "fn http(){if flag {mutate_global();}; NativeApplication::new(Router::new())}",
            "fn http(){let router=notes::Bindings{list:x}.mount(ContractRouter::new(registry())).unwrap().finish().unwrap(); NativeApplication::new(other::router)}",
            "fn http(){other::Bindings{list:x}.mount(ContractRouter::new(registry())).unwrap().finish()}",
        ] {
            std::fs::write(
                root.path().join("application.rs"),
                format!("use contract::notes; use axum::{{Router,routing::get}}; use framework::native::{{ContractRouter,NativeApplication}}; {body}"),
            )
            .unwrap();
            assert!(
                verify(root.path(), root.path(), "notes", &manifest).is_err(),
                "{body}"
            );
        }
    }
    #[test]
    fn raw_route_never_silently_passes_and_expiry_is_checked() {
        let (root, manifest) = fixture(
            "fn http(){NativeApplication::new(notes::Bindings{list:x}.mount(ContractRouter::new(registry())).unwrap().finish().unwrap().route(\"/health\",get(handler)))}",
        );
        assert!(verify(root.path(), root.path(), "notes", &manifest).is_err());
        std::fs::create_dir(root.path().join(".forklaunch")).unwrap();
        let path = root.path().join(".forklaunch/analyze.json");
        let mut policy = serde_json::json!({"formatVersion":"forklaunch.analysis.policy.v1","rawRoutes":[{"method":"GET","path":"/health","source":"application.rs","owner":"notes","reason":"Infrastructure probe","expires":"2099-01-01"}]});
        std::fs::write(&path, policy.to_string()).unwrap();
        verify(root.path(), root.path(), "notes", &manifest).unwrap();
        policy["rawRoutes"][0]["expires"] = "2000-01-01".into();
        std::fs::write(&path, policy.to_string()).unwrap();
        assert!(verify(root.path(), root.path(), "notes", &manifest).is_err());
    }
    #[test]
    fn notes_fixture_assembly_resolves_real_contract_modules() {
        // Fixture manifests are `Cargo.toml.template` so `cargo package` keeps them.
        fn copy(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy();
                let target = to.join(name.strip_suffix(".template").unwrap_or(&name));
                if path.is_dir() {
                    copy(&path, &target);
                } else {
                    std::fs::copy(&path, target).unwrap();
                }
            }
        }
        let fixture = tempfile::tempdir().unwrap();
        copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust-notes-assembly"), fixture.path());
        let root = fixture.path().to_path_buf();
        let directory = root.join("src/notes");
        let mut declarations = Vec::new();
        for (file, group) in [
            ("notes", "api"),
            ("attachments", "api"),
            ("insights", "api"),
            ("realtime", "streams"),
            ("realtime", "sockets"),
        ] {
            let relative = format!("api/contracts/{file}.rs");
            let source =
                syn::parse_file(&std::fs::read_to_string(directory.join(&relative)).unwrap())
                    .unwrap();
            let module = source
                .items
                .iter()
                .find_map(|item| match item {
                    Item::Mod(module) if module.ident == group => Some(module),
                    _ => None,
                })
                .unwrap();
            for item in &module.content.as_ref().unwrap().1 {
                if let Item::Fn(function) = item {
                    if matches!(function.vis, syn::Visibility::Public(_)) {
                        declarations.push(
                            forklaunch_core::contract::assembly::BindingDeclaration {
                                source: relative.clone(),
                                group: group.into(),
                                member: function.sig.ident.to_string(),
                                transport: "http".into(),
                                contract: serde_json::json!({}),
                            },
                        );
                    }
                }
            }
        }
        verify(
            &root,
            &directory,
            "notes",
            &BindingManifest {
                format_version: "forklaunch.bindings.v1".into(),
                declarations,
            },
        )
        .unwrap();
    }
    #[test]
    fn ordinary_default_static_health_and_shared_docs_adapter_resolve() {
        let root = tempfile::tempdir().unwrap();
        let templates =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/templates/rust/project/native-service");
        for name in [
            "application.rs",
            "api/routes/mod.rs",
            "api/routes/health.rs",
            "api/contracts/mod.rs",
            "api/contracts/health.rs",
        ] {
            let target = root.path().join(name);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(templates.join(name), target).unwrap();
        }
        std::fs::write(root.path().join("lib.rs"), "mod api; pub mod application;").unwrap();
        std::fs::write(root.path().join("api/mod.rs"), "pub mod routes;").unwrap();
        std::fs::create_dir(root.path().join("domain")).unwrap();
        std::fs::write(
            root.path().join("domain/Cargo.toml"),
            "[lib]\npath=\"../sdk.rs\"\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("sdk.rs"),
            "#[path=\"api/contracts/mod.rs\"] mod contracts; pub use contracts::*;",
        )
        .unwrap();
        std::fs::write(root.path().join("Cargo.toml"),"[lib]\npath=\"lib.rs\"\n[dependencies]\ncontracts={path=\"domain\"}\naxum=\"0.8\"\nforklaunch-axum=\"0.1\"\n").unwrap();
        let declarations = vec![forklaunch_core::contract::assembly::BindingDeclaration {
            source: "api/contracts/health.rs".into(),
            group: "operations".into(),
            member: "health".into(),
            transport: "http".into(),
            contract: serde_json::json!({}),
        }];
        verify(
            root.path(),
            root.path(),
            "service",
            &BindingManifest {
                format_version: "forklaunch.bindings.v1".into(),
                declarations,
            },
        )
        .unwrap();
    }
}

fn routing_return(output: &syn::ReturnType) -> bool {
    fn routed(ty: &syn::Type) -> bool {
        let syn::Type::Path(path) = ty else {
            return false;
        };
        path.path.segments.iter().any(|segment| {
            matches!(
                segment.ident.to_string().as_str(),
                "Router" | "ContractRouter" | "NativeApplication"
            ) || match &segment.arguments {
                syn::PathArguments::AngleBracketed(arguments) => arguments.args.iter().any(
                    |argument| matches!(argument,syn::GenericArgument::Type(ty) if routed(ty)),
                ),
                _ => false,
            }
        })
    }
    match output {
        syn::ReturnType::Default => true,
        syn::ReturnType::Type(_, ty) => routed(ty),
    }
}

#[derive(Debug)]
pub(super) struct UnmountedOperation;
impl std::fmt::Display for UnmountedOperation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Exported static operation set is unmounted or replaced")
    }
}
impl std::error::Error for UnmountedOperation {}
