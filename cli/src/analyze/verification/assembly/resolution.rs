//! Resolve generated binding paths through ordinary Rust modules/use declarations.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use anyhow::{Result, ensure};
use syn::{Item, UseTree};

pub(super) struct Resolver {
    directory: PathBuf,
    contract: Option<PathBuf>,
}
impl Resolver {
    pub(super) fn new(directory: &Path) -> Result<Self> {
        Ok(Self {
            directory: directory.canonicalize()?,
            contract: forklaunch_core::build::provenance::contract_directory(directory)?,
        })
    }
    pub(super) fn owns_source(&self, source: &Path) -> bool {
        source.starts_with(&self.directory)
            || self
                .contract
                .as_ref()
                .is_some_and(|root| source.starts_with(root))
    }
    pub(super) fn binding(&self, source: &Path, path: &syn::Path) -> Result<(PathBuf, String)> {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        let mut active = BTreeSet::new();
        self.resolve_file(source, &segments, 0, &mut active, &self.directory)?
            .ok_or_else(|| anyhow::anyhow!("Unresolved static binding type path"))
    }
    fn external(
        &self,
        path: &[String],
        depth: usize,
        active: &mut BTreeSet<String>,
    ) -> Result<Option<(PathBuf, String)>> {
        if path.is_empty() {
            return Ok(None);
        }
        let cargo: toml::Value =
            toml::from_str(&std::fs::read_to_string(self.directory.join("Cargo.toml"))?)?;
        let Some(dependencies) = cargo.get("dependencies").and_then(toml::Value::as_table) else {
            return Ok(None);
        };
        let dependency = dependencies
            .iter()
            .find(|(name, _)| name.replace('-', "_") == path[0])
            .map(|(_, value)| value);
        let Some(relative) = dependency
            .and_then(|value| value.get("path"))
            .and_then(toml::Value::as_str)
        else {
            return Ok(None);
        };
        let directory = self.directory.join(relative).canonicalize()?;
        ensure!(
            self.owns_source(&directory),
            "Binding contract dependency must be service-owned in this verifier profile"
        );
        let cargo: toml::Value =
            toml::from_str(&std::fs::read_to_string(directory.join("Cargo.toml"))?)?;
        let entry = cargo
            .get("lib")
            .and_then(|value| value.get("path"))
            .and_then(toml::Value::as_str)
            .unwrap_or("src/lib.rs");
        self.resolve_file(
            &directory.join(entry),
            &path[1..],
            depth + 1,
            active,
            &directory,
        )
    }
    fn resolve_file(
        &self,
        file: &Path,
        path: &[String],
        depth: usize,
        active: &mut BTreeSet<String>,
        crate_root: &Path,
    ) -> Result<Option<(PathBuf, String)>> {
        ensure!(
            depth < 32,
            "Binding module resolution exceeds recursion bound at {} for {:?}",
            file.display(),
            path
        );
        let file = file.canonicalize()?;
        ensure!(self.owns_source(&file), "Binding source leaves service");
        let key = format!("{}:{path:?}", file.display());
        if !active.insert(key.clone()) {
            return Ok(None);
        }
        let items = syn::parse_file(&source_text(&file)?)?.items;
        let result = self.resolve_items(&file, &items, path, depth, active, crate_root);
        active.remove(&key);
        result
    }
    fn resolve_items(
        &self,
        file: &Path,
        items: &[Item],
        path: &[String],
        depth: usize,
        active: &mut BTreeSet<String>,
        crate_root: &Path,
    ) -> Result<Option<(PathBuf, String)>> {
        ensure!(
            depth < 32,
            "Binding module resolution exceeds recursion bound at {} for {:?}",
            file.display(),
            path
        );
        if path.is_empty() {
            return Ok(None);
        }
        if path[0] == "crate" {
            let directory = crate_root;
            let cargo: toml::Value =
                toml::from_str(&std::fs::read_to_string(directory.join("Cargo.toml"))?)?;
            let entry = cargo
                .get("lib")
                .and_then(|v| v.get("path"))
                .and_then(toml::Value::as_str)
                .unwrap_or("src/lib.rs");
            return self.resolve_file(
                &directory.join(entry),
                &path[1..],
                depth + 1,
                active,
                &directory,
            );
        }
        if path.len() == 1
            && items
                .iter()
                .any(|item| matches!(item,Item::Fn(function) if function.sig.ident==path[0]))
        {
            return Ok(Some((file.into(), "$function".into())));
        }
        for item in items {
            if let Item::Macro(export) = item {
                let owner = export
                    .mac
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                if owner == "forklaunch_core::contract::controller_contract" {
                    use syn::parse::Parser;
                    let (name, source) = (|input: syn::parse::ParseStream<'_>| {
                        let name = input.parse::<syn::Ident>()?;
                        input.parse::<syn::Token![,]>()?;
                        let source = input.parse::<syn::LitStr>()?;
                        Ok((name, source))
                    })
                    .parse2(export.mac.tokens.clone())?;
                    if name == path[0] {
                        let controller = file.parent().unwrap().join(source.value());
                        return self.resolve_file(
                            &controller,
                            &path[1..],
                            depth + 1,
                            active,
                            crate_root,
                        );
                    }
                }
            }
            if let Item::Mod(module) = item {
                if module.ident != path[0] {
                    continue;
                }
                ensure!(
                    !module
                        .attrs
                        .iter()
                        .any(|attribute| attribute.path().is_ident("cfg")),
                    "Conditional binding module requires explicit configuration resolution"
                );
                if module.attrs.iter().any(|attribute| {
                    attribute.path().segments.last().is_some_and(|segment| {
                        matches!(segment.ident.to_string().as_str(), "api" | "socket_api")
                    })
                }) && path.len() == 2
                    && path[1] == "Bindings"
                {
                    return Ok(Some((file.into(), module.ident.to_string())));
                }
                if let Some((_, nested)) = &module.content {
                    return self.resolve_items(
                        file,
                        nested,
                        &path[1..],
                        depth + 1,
                        active,
                        crate_root,
                    );
                }
                let explicit = module
                    .attrs
                    .iter()
                    .find_map(|attribute| match &attribute.meta {
                        syn::Meta::NameValue(value) if value.path.is_ident("path") => {
                            match &value.value {
                                syn::Expr::Lit(value) => match &value.lit {
                                    syn::Lit::Str(value) => Some(value.value()),
                                    _ => None,
                                },
                                _ => None,
                            }
                        }
                        _ => None,
                    });
                let parent = file.parent().unwrap();
                let child = if let Some(explicit) = explicit {
                    parent.join(explicit)
                } else {
                    let base = if file.file_name().is_some_and(|name| {
                        matches!(name.to_str(), Some("mod.rs" | "lib.rs" | "main.rs"))
                    }) {
                        parent.to_path_buf()
                    } else {
                        parent.join(file.file_stem().unwrap())
                    };
                    let direct = base.join(format!("{}.rs", module.ident));
                    if direct.exists() {
                        direct
                    } else {
                        base.join(module.ident.to_string()).join("mod.rs")
                    }
                };
                return self.resolve_file(&child, &path[1..], depth + 1, active, crate_root);
            }
        }
        for item in items {
            if let Item::Use(import) = item {
                for replacement in imports(&import.tree, &[], path) {
                    if replacement == path {
                        continue;
                    }
                    if let Some(found) = self.resolve_items(
                        file,
                        items,
                        &replacement,
                        depth + 1,
                        active,
                        crate_root,
                    )? {
                        return Ok(Some(found));
                    }
                }
            }
        }
        self.external(path, depth, active)
    }
}
fn imports(tree: &UseTree, prefix: &[String], path: &[String]) -> Vec<Vec<String>> {
    match tree {
        UseTree::Path(value) => {
            let mut prefix = prefix.to_vec();
            prefix.push(value.ident.to_string());
            imports(&value.tree, &prefix, path)
        }
        UseTree::Group(group) => group
            .items
            .iter()
            .flat_map(|item| imports(item, prefix, path))
            .collect(),
        UseTree::Name(name) if name.ident == path[0] => {
            let mut result = prefix.to_vec();
            result.push(name.ident.to_string());
            result.extend_from_slice(&path[1..]);
            vec![result]
        }
        UseTree::Rename(name) if name.rename == path[0] => {
            let mut result = prefix.to_vec();
            result.push(name.ident.to_string());
            result.extend_from_slice(&path[1..]);
            vec![result]
        }
        UseTree::Glob(_) => {
            let mut result = prefix.to_vec();
            result.extend_from_slice(path);
            vec![result]
        }
        _ => vec![],
    }
}

impl Resolver {
    pub(super) fn framework_type(
        &self,
        source: &Path,
        path: &syn::Path,
        expected: &str,
    ) -> Result<bool> {
        let mut segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        segments.pop();
        let items = syn::parse_file(&source_text(source)?)?.items;
        self.framework_segments(&items, &segments, expected, 0)
    }
    pub(super) fn routing_method(
        &self,
        source: &Path,
        path: &syn::Path,
        method: &str,
    ) -> Result<bool> {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        let items = syn::parse_file(&source_text(source)?)?.items;
        self.framework_segments(&items, &segments, &format!("routing::{method}"), 0)
    }
    fn framework_segments(
        &self,
        items: &[Item],
        path: &[String],
        expected: &str,
        depth: usize,
    ) -> Result<bool> {
        if depth >= 16 || path.is_empty() {
            return Ok(false);
        }
        for item in items {
            match item {
                Item::Struct(item) if item.ident == path[0] => return Ok(false),
                Item::Enum(item) if item.ident == path[0] => return Ok(false),
                Item::Type(item) if item.ident == path[0] => return Ok(false),
                Item::Mod(item) if item.ident == path[0] => return Ok(false),
                Item::Use(item) => {
                    for replacement in imports(&item.tree, &[], path) {
                        if replacement != path
                            && self.framework_segments(items, &replacement, expected, depth + 1)?
                        {
                            return Ok(true);
                        }
                    }
                }
                _ => {}
            }
        }
        let cargo: toml::Value =
            toml::from_str(&std::fs::read_to_string(self.directory.join("Cargo.toml"))?)?;
        let Some(dependencies) = cargo.get("dependencies").and_then(toml::Value::as_table) else {
            return Ok(false);
        };
        let Some((name, value)) = dependencies
            .iter()
            .find(|(name, _)| name.replace('-', "_") == path[0])
        else {
            return Ok(false);
        };
        let package = value
            .get("package")
            .and_then(toml::Value::as_str)
            .unwrap_or(name);
        Ok(match expected {
            "Router" => {
                (package == "axum" && path[1..] == ["Router"])
                    || (package == "forklaunch-axum" && path[1..] == ["native", "Router"])
            }
            "ContractRouter" | "NativeApplication" => {
                package == "forklaunch-axum" && path[1..] == ["native", expected]
            }
            "routing::with_openapi" => {
                package == "forklaunch-axum" && path[1..] == ["native", "with_openapi"]
            }
            _ => expected
                .strip_prefix("routing::")
                .is_some_and(|method| package == "axum" && path[1..] == ["routing", method]),
        })
    }
}

pub(super) fn source_text(path: &Path) -> Result<String> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 2 * 1024 * 1024,
        "Assembly source exceeds size limit"
    );
    Ok(String::from_utf8(bytes)?)
}

impl Resolver {
    pub(super) fn application_entry(&self) -> Result<PathBuf> {
        let cargo: toml::Value =
            toml::from_str(&std::fs::read_to_string(self.directory.join("Cargo.toml"))?)?;
        let entry = cargo
            .get("lib")
            .and_then(|value| value.get("path"))
            .and_then(toml::Value::as_str)
            .unwrap_or("src/lib.rs");
        let (source, kind) = self.binding(
            &self.directory.join(entry),
            &syn::parse_str("crate::application::http")?,
        )?;
        ensure!(
            kind == "$function",
            "Managed application entry is unresolved"
        );
        Ok(source)
    }
    pub(super) fn standard_ok(&self, source: &Path) -> Result<bool> {
        let items = syn::parse_file(&source_text(source)?)?.items;
        Ok(!items.iter().any(|item| match item {
            Item::Use(item) => !imports(&item.tree, &[], &["Ok".into()]).is_empty(),
            Item::Fn(item) => item.sig.ident == "Ok",
            Item::Struct(item) => item.ident == "Ok",
            Item::Type(item) => item.ident == "Ok",
            Item::Enum(item) => item.ident == "Ok",
            _ => false,
        }))
    }
}
