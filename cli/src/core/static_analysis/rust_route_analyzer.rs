//! The Rust half of the route scan: every HTTP route a Rust service registers, read from its
//! source with a real parser. Nothing is built, expanded or run: no `cargo`, no build script,
//! no macro expansion.
//!
//! Three shapes are read.
//!
//! 1. Typed ForkLaunch contracts. A module marked `#[forklaunch_core::contract::api]` declares
//!    one endpoint per public function: `Endpoint::new(name, summary, HttpMethod::Get, "/path",
//!    200)`. `#[access(Authenticated)]` on the function (or `.with_access(AccessLevel::..)` in
//!    its body) is the access level; with neither, the framework's default is `Public`. How
//!    callers are checked is the policy the endpoint is mounted with:
//!    `CreateNote::bind_authorized(policy, handler)`. `Health::bind(handler)` is the public
//!    binding and carries no policy.
//! 2. `RouteContract::get(name, summary, "/path").with_access(..).with_auth(..)`, the untyped
//!    contract a `ForkLaunchRouter` registers.
//! 3. Raw Axum: `Router::new().route("/x", get(h).post(h2))`, `.nest("/api", ..)`,
//!    `.route_service`, `.nest_service`. The framework never sees these, so they are listed as
//!    outside it, the same as a raw express route.
//!
//! A file the parser cannot read is reported, never skipped. A contract the reader finds but
//! cannot decide (a computed path, an access level set away from the declaration, a non-public
//! endpoint whose binding is not in the scanned source) is reported as undecided rather than
//! guessed either way.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use proc_macro2::{TokenStream, TokenTree};
use syn::{
    Attribute, Expr, ExprCall, ExprMethodCall, ItemFn, ItemMod, Lit, UseTree,
    spanned::Spanned,
    visit::{self, Visit},
};

use super::route_analyzer::{RouteScan, RouteSource, StaticRoute, UndecidedRoute};

/// Directories that hold no production Rust: build output, dependencies, tests and fixtures.
const SKIP_DIRS: &[&str] = &[
    "target",
    "node_modules",
    "dist",
    "vendor",
    "tests",
    "test",
    "__test__",
    "__tests__",
    "benches",
    "examples",
    "fixtures",
    "__fixtures__",
    "e2e",
    "scripts",
    "seeders",
    "seeds",
    "migrations",
];

/// Larger than any hand-written source file; a bigger one is reported unread, not parsed.
const MAX_SOURCE_BYTES: u64 = 4 * 1024 * 1024;

/// Axum (and actix) method-router constructors and the verb each registers.
const METHOD_ROUTERS: &[(&str, &str)] = &[
    ("get", "GET"),
    ("post", "POST"),
    ("put", "PUT"),
    ("patch", "PATCH"),
    ("delete", "DELETE"),
    ("head", "HEAD"),
    ("options", "OPTIONS"),
    ("trace", "TRACE"),
    ("connect", "CONNECT"),
    ("any", "ANY"),
    ("get_service", "GET"),
    ("post_service", "POST"),
    ("put_service", "PUT"),
    ("patch_service", "PATCH"),
    ("delete_service", "DELETE"),
    ("head_service", "HEAD"),
    ("options_service", "OPTIONS"),
    ("trace_service", "TRACE"),
    ("connect_service", "CONNECT"),
    ("any_service", "ANY"),
];

/// `ForkLaunchRouter` methods that register one contract with one handler.
const CONTRACT_REGISTRATIONS: &[&str] = &[
    "route", "get", "post", "put", "patch", "delete", "head", "options", "trace",
];

/// One parsed production source file.
pub(crate) struct RustFile {
    /// Path relative to the scanned root, with `/` separators.
    pub(crate) rel: String,
    pub(crate) ast: syn::File,
}

/// The production Rust under a root, and the files the parser could not read.
#[derive(Default)]
pub(crate) struct RustSources {
    pub(crate) files: Vec<RustFile>,
    pub(crate) unreadable: Vec<String>,
}

fn is_production_rust(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with(".rs")
        && name != "tests.rs"
        && name != "test.rs"
        && !name.ends_with("_test.rs")
        && !name.ends_with("_tests.rs")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // A symlink can point back up the tree; follow none, so the walk always ends.
        if fs::symlink_metadata(&path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
        {
            continue;
        }
        if path.is_dir() {
            if !SKIP_DIRS.contains(&name) && !name.starts_with('.') {
                walk(&path, out);
            }
        } else if is_production_rust(&path) {
            out.push(path);
        }
    }
}

/// The path of every production `.rs` file under `root`, sorted, relative to it.
pub(crate) fn rust_production_files(root: &Path) -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    walk(root, &mut files);
    files
        .into_iter()
        .map(|file| {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, file)
        })
        .collect()
}

/// Parse every production Rust file under `root`.
pub(crate) fn read_rust_sources(root: &Path) -> RustSources {
    let mut texts = Vec::new();
    let mut unreadable = Vec::new();
    for (rel, file) in rust_production_files(root) {
        let small = fs::metadata(&file).is_ok_and(|m| m.len() <= MAX_SOURCE_BYTES);
        match fs::read_to_string(&file) {
            Ok(text) if small => texts.push((rel, text)),
            _ => unreadable.push(rel),
        }
    }
    let mut sources = parse_sources(texts);
    sources.unreadable.extend(unreadable);
    sources.unreadable.sort();
    sources
}

/// Parse `(relative path, source)` pairs, dropping modules compiled only for tests.
pub(crate) fn parse_sources(texts: Vec<(String, String)>) -> RustSources {
    let mut sources = RustSources::default();
    for (rel, text) in texts {
        match syn::parse_file(&text) {
            Ok(ast) => sources.files.push(RustFile { rel, ast }),
            Err(_) => sources.unreadable.push(rel),
        }
    }
    // `#[cfg(test)] mod checks;` names a file that only test builds compile.
    let mut test_only: BTreeSet<String> = BTreeSet::new();
    for file in &sources.files {
        for item in &file.ast.items {
            let syn::Item::Mod(module) = item else {
                continue;
            };
            if module.content.is_none() && has_cfg_test(&module.attrs) {
                test_only.extend(module_file_candidates(&file.rel, module));
            }
        }
    }
    let is_test_only = |rel: &str| {
        test_only
            .iter()
            .any(|t| rel == t || rel.starts_with(&format!("{}/", t.trim_end_matches(".rs"))))
    };
    sources.files.retain(|f| !is_test_only(&f.rel));
    sources.unreadable.retain(|rel| !is_test_only(rel));
    sources
}

/// Where an out-of-line `mod name;` declared in `rel` may live.
fn module_file_candidates(rel: &str, module: &ItemMod) -> Vec<String> {
    let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    let join = |dir: &str, tail: &str| {
        if dir.is_empty() {
            tail.to_string()
        } else {
            format!("{dir}/{tail}")
        }
    };
    if let Some(path) = module.attrs.iter().find_map(path_attribute) {
        return vec![join(dir, &path)];
    }
    let name = module.ident.to_string();
    let mut candidates = vec![
        join(dir, &format!("{name}.rs")),
        join(dir, &format!("{name}/mod.rs")),
    ];
    let stem = file.trim_end_matches(".rs");
    if !matches!(stem, "mod" | "lib" | "main") {
        let nested = join(dir, stem);
        candidates.push(format!("{nested}/{name}.rs"));
        candidates.push(format!("{nested}/{name}/mod.rs"));
    }
    candidates
}

/// The string in `#[path = "..."]`.
fn path_attribute(attribute: &Attribute) -> Option<String> {
    if !attribute.path().is_ident("path") {
        return None;
    }
    let syn::Meta::NameValue(value) = &attribute.meta else {
        return None;
    };
    string_literal(&value.value)
}

/// `#[cfg(test)]`, `#[cfg(all(test, ..))]`, `#[test]`, `#[tokio::test]`.
pub(crate) fn has_cfg_test(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        if attribute.path().is_ident("cfg") {
            attribute
                .parse_args::<TokenStream>()
                .is_ok_and(mentions_test)
        } else {
            attribute
                .path()
                .segments
                .last()
                .is_some_and(|s| s.ident == "test")
        }
    })
}

/// Whether a `cfg` predicate requires `test` (a `not(test)` branch does not).
fn mentions_test(tokens: TokenStream) -> bool {
    let mut negated = false;
    for token in tokens {
        match token {
            TokenTree::Ident(ident) if ident == "not" => negated = true,
            TokenTree::Ident(ident) if ident == "test" => return true,
            TokenTree::Group(group) => {
                if !negated && mentions_test(group.stream()) {
                    return true;
                }
                negated = false;
            }
            _ => negated = false,
        }
    }
    false
}

fn segments(path: &syn::Path) -> Vec<String> {
    path.segments.iter().map(|s| s.ident.to_string()).collect()
}

/// Strip parentheses, `&`, `?` and `.await`: wrappers that do not change which value this is.
fn peel(mut expr: &Expr) -> &Expr {
    loop {
        expr = match expr {
            Expr::Paren(inner) => &inner.expr,
            Expr::Group(inner) => &inner.expr,
            Expr::Reference(inner) => &inner.expr,
            Expr::Try(inner) => &inner.expr,
            Expr::Await(inner) => &inner.base,
            _ => return expr,
        };
    }
}

pub(crate) fn string_literal(expr: &Expr) -> Option<String> {
    match peel(expr) {
        Expr::Lit(literal) => match &literal.lit {
            Lit::Str(text) => Some(text.value()),
            _ => None,
        },
        _ => None,
    }
}

/// The expression as written in the source, on one line.
fn text_of<T: Spanned>(node: &T) -> String {
    let text = node.span().source_text().unwrap_or_default();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        "(expression)".to_string()
    } else {
        text
    }
}

fn line_of<T: Spanned>(node: &T) -> usize {
    node.span().start().line
}

/// A method chain, outermost call first, and the expression it starts from.
fn flatten(call: &ExprMethodCall) -> (Vec<&ExprMethodCall>, &Expr) {
    let mut links = vec![call];
    let mut base = peel(&call.receiver);
    while let Expr::MethodCall(inner) = base {
        links.push(inner);
        base = peel(&inner.receiver);
    }
    (links, base)
}

/// `AccessLevel::Protected` -> `protected`.
fn access_level(expr: &Expr) -> Option<String> {
    let Expr::Path(path) = peel(expr) else {
        return None;
    };
    let name = path.path.segments.last()?.ident.to_string();
    matches!(
        name.as_str(),
        "Public" | "Authenticated" | "Protected" | "Internal"
    )
    .then(|| name.to_lowercase())
}

/// `HttpMethod::PropFind` -> `PROPFIND`.
fn http_method(expr: &Expr) -> Option<String> {
    let Expr::Path(path) = peel(expr) else {
        return None;
    };
    let names = segments(&path.path);
    (names.len() >= 2 && names[names.len() - 2] == "HttpMethod")
        .then(|| names[names.len() - 1].to_uppercase())
}

/// `RouteContract::get(..)` / `RouteContract::new(..)`: the method and path it declares.
/// `None` when the call is not a route contract constructor.
fn route_contract_constructor(call: &ExprCall) -> Option<(Option<String>, Option<String>)> {
    let Expr::Path(function) = peel(&call.func) else {
        return None;
    };
    let names = segments(&function.path);
    if names.len() < 2 || names[names.len() - 2] != "RouteContract" {
        return None;
    }
    let constructor = names[names.len() - 1].as_str();
    match (constructor, call.args.len()) {
        ("new", 4) => Some((http_method(&call.args[2]), string_literal(&call.args[3]))),
        (verb, 3) if verb.chars().all(|c| c.is_ascii_lowercase()) => {
            Some((Some(verb.to_uppercase()), string_literal(&call.args[2])))
        }
        _ => None,
    }
}

/// What a contract's own builder chain states about its protection.
#[derive(Default)]
struct Declared {
    access: Option<String>,
    auth: Option<String>,
}

impl Declared {
    fn read(&mut self, link: &ExprMethodCall) {
        match (link.method.to_string().as_str(), link.args.first()) {
            ("with_access", Some(level)) => {
                if let Some(level) = access_level(level) {
                    self.access.get_or_insert(level);
                }
            }
            ("with_auth" | "with_rbac", Some(auth)) => {
                self.auth.get_or_insert_with(|| text_of(auth));
            }
            _ => {}
        }
    }
}

/// One endpoint declared in an `api` module.
struct Contract {
    function: String,
    /// The generated operation tag: `create_note` -> `CreateNote`.
    tag: String,
    /// Names that identify the declaring module: the file stem and enclosing modules.
    owners: Vec<String>,
    method: String,
    path: String,
    access: String,
    auth: Option<String>,
    file: String,
    line: usize,
}

#[derive(PartialEq, Clone, Copy)]
enum BindKind {
    Public,
    Authorized,
}

/// `Tag::bind(handler)`, `Tag::bind_authorized(policy, handler)`, or
/// `router.bind_authorized(contract::create_note(), policy, handler)`.
struct Binding {
    /// The tag (`CreateNote`) or the declaring function (`create_note`).
    target: String,
    by_function: bool,
    /// Path segments written before the target, e.g. `controllers::notes::contract`.
    qualifier: Vec<String>,
    kind: BindKind,
    policy: Option<String>,
    file: String,
}

fn operation_tag(function: &str) -> String {
    function
        .trim_start_matches("r#")
        .split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect()
}

fn bind_kind(name: &str) -> Option<BindKind> {
    match name {
        "bind" => Some(BindKind::Public),
        "bind_authorized" | "bind_sse_authorized" | "bind_sse_authorized_with_headers" => {
            Some(BindKind::Authorized)
        }
        _ => None,
    }
}

#[derive(Default)]
struct Facts {
    contracts: Vec<Contract>,
    bindings: Vec<Binding>,
    routes: Vec<StaticRoute>,
    undecided: Vec<UndecidedRoute>,
    /// Every `use` path, flattened, per file.
    imports: BTreeMap<String, Vec<Vec<String>>>,
}

struct Reader<'a> {
    file: &'a str,
    facts: &'a mut Facts,
    modules: Vec<String>,
    /// Prefixes of the `.nest("/prefix", ..)` calls being read.
    prefixes: Vec<String>,
    /// Route contract constructors already read as part of a chain: (line, column).
    seen: BTreeSet<(usize, usize)>,
}

impl Reader<'_> {
    fn undecided(&mut self, line: usize, what: String, why: &str) {
        self.facts.undecided.push(UndecidedRoute {
            file: self.file.to_string(),
            line,
            what,
            why: why.to_string(),
        });
    }

    fn raw_route(&mut self, method: &str, path: &str, line: usize) {
        let prefix: String = self.prefixes.concat();
        let path = if prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}{}", prefix.trim_end_matches('/'), path)
        };
        self.facts.routes.push(StaticRoute {
            method: method.to_string(),
            path,
            access: None,
            auth: None,
            source: RouteSource::OutsideFramework,
            file: self.file.to_string(),
            line,
        });
    }

    /// The verbs a method-router expression registers: `get(a).post(b)` -> GET, POST.
    fn method_router_verbs(handler: &Expr) -> Vec<&'static str> {
        let verb = |name: &str| {
            METHOD_ROUTERS
                .iter()
                .find(|(function, _)| *function == name)
                .map(|(_, verb)| *verb)
        };
        let mut verbs = Vec::new();
        let base = match peel(handler) {
            Expr::MethodCall(call) => {
                let (links, base) = flatten(call);
                for link in links.iter().rev() {
                    verbs.extend(verb(&link.method.to_string()));
                }
                base
            }
            other => other,
        };
        if let Expr::Call(call) = base
            && let Expr::Path(function) = peel(&call.func)
            && let Some(name) = function.path.segments.last()
        {
            let name = name.ident.to_string();
            if matches!(name.as_str(), "on" | "on_service") {
                // on(MethodFilter::GET, handler)
                if let Some(Expr::Path(filter)) = call.args.first().map(peel)
                    && let Some(method) = filter.path.segments.last()
                    && let Some((_, verb)) = METHOD_ROUTERS
                        .iter()
                        .find(|(_, verb)| method.ident == *verb)
                {
                    verbs.insert(0, verb);
                }
            } else if let Some(first) = verb(&name) {
                verbs.insert(0, first);
            }
        }
        verbs
    }

    /// `.route("/x", get(h).post(h2))` on a raw router.
    fn read_raw_route(&mut self, link: &ExprMethodCall) -> bool {
        let (path, handler) = (&link.args[0], &link.args[1]);
        let verbs = Self::method_router_verbs(handler);
        let literal = string_literal(path);
        if verbs.is_empty() && literal.is_none() {
            // `.route(contract, handler)` on a ForkLaunchRouter, or not a route at all.
            return false;
        }
        let path = literal.unwrap_or_else(|| text_of(path));
        let line = line_of(&link.method);
        if verbs.is_empty() {
            self.raw_route("UNKNOWN", &path, line);
        }
        for verb in verbs {
            self.raw_route(verb, &path, line);
        }
        true
    }

    /// A `RouteContract` constructor and the builder calls chained on it.
    ///
    /// `registered` says the chain is handed straight to a router registration, so what the
    /// chain states is all there is; otherwise the value may be changed before it is mounted.
    fn read_route_contract(
        &mut self,
        constructor: &ExprCall,
        links: &[&ExprMethodCall],
        registered: bool,
    ) {
        let Some((method, path)) = route_contract_constructor(constructor) else {
            return;
        };
        let start = constructor.span().start();
        if !self.seen.insert((start.line, start.column)) {
            return;
        }
        let line = start.line;
        let mut declared = Declared::default();
        for link in links.iter().rev() {
            declared.read(link);
        }
        let (Some(method), Some(path)) = (method, path) else {
            self.undecided(
                line,
                "a route contract".to_string(),
                "its method or path is computed, not written as a literal",
            );
            return;
        };
        if declared.access.is_none() && declared.auth.is_none() && !registered {
            self.undecided(
                line,
                format!("{method} {path}"),
                "its access level and auth are not set where it is declared",
            );
            return;
        }
        // With no `.with_access(..)` on a contract registered as written, the framework's
        // default applies: `RouteContract::new` starts every contract as public.
        let access = declared
            .access
            .or_else(|| (registered && declared.auth.is_none()).then(|| "public".to_string()));
        self.facts.routes.push(StaticRoute {
            method,
            path,
            access,
            auth: declared.auth,
            source: RouteSource::Contract,
            file: self.file.to_string(),
            line,
        });
    }

    /// One `pub fn` of an `api` module: an endpoint declaration.
    fn read_endpoint(&mut self, function: &ItemFn) {
        let name = function.sig.ident.to_string();
        let line = line_of(&function.sig.ident);
        let mut body = EndpointBody::default();
        body.visit_block(&function.block);
        let attribute = function
            .attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("access"))
            .find_map(|attribute| attribute.parse_args::<syn::Ident>().ok())
            .map(|level| level.to_string().to_lowercase());
        let (Some(method), Some(path)) = (body.method, body.path) else {
            self.undecided(
                line,
                format!("endpoint `{name}`"),
                "its method or path is computed, not written as a literal",
            );
            return;
        };
        // An unannotated declaration is public: that is the framework's default, and the
        // macro gives only such declarations the public `bind` constructor.
        let access = attribute
            .or(body.declared.access)
            .unwrap_or_else(|| "public".to_string());
        let mut owners = self.modules.clone();
        let stem = self
            .file
            .trim_end_matches(".rs")
            .trim_end_matches("/mod")
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();
        owners.push(stem);
        self.facts.contracts.push(Contract {
            tag: operation_tag(&name),
            function: name,
            owners,
            method,
            path,
            access,
            auth: body.declared.auth,
            file: self.file.to_string(),
            line,
        });
    }
}

/// What an endpoint declaration's body states: the `Endpoint::new(..)` call and any
/// `.with_access(..)` / `.with_auth(..)` on it.
#[derive(Default)]
struct EndpointBody {
    method: Option<String>,
    path: Option<String>,
    declared: Declared,
}

impl<'ast> Visit<'ast> for EndpointBody {
    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(function) = peel(&call.func) {
            let names = segments(&function.path);
            let is_endpoint_new = names.len() >= 2
                && names[names.len() - 2] == "Endpoint"
                && names[names.len() - 1] == "new"
                && call.args.len() >= 4;
            if is_endpoint_new && self.method.is_none() {
                self.method = http_method(&call.args[2]);
                self.path = string_literal(&call.args[3]);
            } else if self.method.is_none()
                && let Some((method, path)) = route_contract_constructor(call)
            {
                // Endpoint::from_contract(RouteContract::get(..))
                self.method = method;
                self.path = path;
            }
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        self.declared.read(call);
        visit::visit_expr_method_call(self, call);
    }
}

fn collect_imports(tree: &UseTree, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_imports(&path.tree, prefix, out);
            prefix.pop();
        }
        UseTree::Name(name) => {
            let mut full = prefix.clone();
            full.push(name.ident.to_string());
            out.push(full);
        }
        UseTree::Rename(rename) => {
            let mut full = prefix.clone();
            full.push(rename.ident.to_string());
            out.push(full);
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_imports(item, prefix, out);
            }
        }
        UseTree::Glob(_) => {}
    }
}

impl<'ast> Visit<'ast> for Reader<'_> {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut paths = Vec::new();
        collect_imports(&item.tree, &mut Vec::new(), &mut paths);
        self.facts
            .imports
            .entry(self.file.to_string())
            .or_default()
            .extend(paths);
    }

    fn visit_item_mod(&mut self, module: &'ast ItemMod) {
        if has_cfg_test(&module.attrs) {
            return;
        }
        let is_api = module.attrs.iter().any(|attribute| {
            attribute
                .path()
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "api")
        });
        self.modules.push(module.ident.to_string());
        if is_api && let Some((_, items)) = &module.content {
            for item in items {
                match item {
                    syn::Item::Fn(function)
                        if matches!(function.vis, syn::Visibility::Public(_)) =>
                    {
                        self.read_endpoint(function);
                    }
                    other => self.visit_item(other),
                }
            }
        } else {
            visit::visit_item_mod(self, module);
        }
        self.modules.pop();
    }

    fn visit_item_fn(&mut self, function: &'ast ItemFn) {
        if !has_cfg_test(&function.attrs) {
            visit::visit_item_fn(self, function);
        }
    }

    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        if !has_cfg_test(&function.attrs) {
            visit::visit_impl_item_fn(self, function);
        }
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(function) = peel(&call.func) {
            let names = segments(&function.path);
            if let [qualifier @ .., target, name] = names.as_slice()
                && let Some(kind) = bind_kind(name)
            {
                // CreateNote::bind_authorized(policy, handler)
                let policy = (kind == BindKind::Authorized)
                    .then(|| call.args.first().map(text_of))
                    .flatten();
                self.facts.bindings.push(Binding {
                    target: target.clone(),
                    by_function: false,
                    qualifier: qualifier.to_vec(),
                    kind,
                    policy,
                    file: self.file.to_string(),
                });
            }
        }
        self.read_route_contract(call, &[], false);
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        let (links, base) = flatten(call);
        if let Expr::Call(constructor) = base {
            self.read_route_contract(constructor, &links, false);
        }
        // Innermost call first, so routes come out in source order.
        for link in links.iter().rev() {
            let name = link.method.to_string();
            let mut nested = false;
            match (name.as_str(), link.args.len()) {
                ("route", 2) if self.read_raw_route(link) => {}
                ("route_service" | "nest_service", 2) => {
                    if let Some(path) = string_literal(&link.args[0]) {
                        self.raw_route("ANY", &path, line_of(&link.method));
                    }
                }
                ("nest", 2) => {
                    if let Some(prefix) = string_literal(&link.args[0]) {
                        // Routes written inline in the nested router carry its prefix.
                        self.visit_expr(&link.args[0]);
                        self.prefixes.push(prefix);
                        self.visit_expr(&link.args[1]);
                        self.prefixes.pop();
                        nested = true;
                    }
                }
                (registration, 2) if CONTRACT_REGISTRATIONS.contains(&registration) => {
                    // router.get(RouteContract::get(..).with_access(..), handler)
                    match peel(&link.args[0]) {
                        Expr::Call(constructor) => {
                            self.read_route_contract(constructor, &[], true);
                        }
                        Expr::MethodCall(chain) => {
                            let (inner, inner_base) = flatten(chain);
                            if let Expr::Call(constructor) = inner_base {
                                self.read_route_contract(constructor, &inner, true);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            if let Some(kind) = bind_kind(&name)
                && link.args.len() >= 2
                && let Expr::Call(endpoint) = peel(&link.args[0])
                && endpoint.args.is_empty()
                && let Expr::Path(function) = peel(&endpoint.func)
                && let [qualifier @ .., target] = segments(&function.path).as_slice()
            {
                // router.bind_authorized(contract::create_note(), policy, handler)
                let policy = (kind == BindKind::Authorized)
                    .then(|| link.args.get(1).map(text_of))
                    .flatten();
                self.facts.bindings.push(Binding {
                    target: target.clone(),
                    by_function: true,
                    qualifier: qualifier.to_vec(),
                    kind,
                    policy,
                    file: self.file.to_string(),
                });
            }
            if !nested {
                for argument in &link.args {
                    self.visit_expr(argument);
                }
            }
        }
        self.visit_expr(base);
    }
}

/// One way of telling which declaring module a binding means.
type Narrowing<'a> = Box<dyn Fn(&Contract) -> bool + 'a>;

/// The contracts a binding can refer to, narrowed until one declaring module is left.
fn resolve<'c>(binding: &Binding, facts: &'c Facts) -> Vec<&'c Contract> {
    let candidates: Vec<&Contract> = facts
        .contracts
        .iter()
        .filter(|contract| {
            if binding.by_function {
                contract.function == binding.target
            } else {
                contract.tag == binding.target
            }
        })
        .collect();
    let one_module = |found: &[&Contract]| {
        found
            .iter()
            .map(|contract| (&contract.file, &contract.owners))
            .collect::<BTreeSet<_>>()
            .len()
            == 1
    };
    if candidates.is_empty() || one_module(&candidates) {
        return candidates;
    }
    let file_stem = |file: &str| {
        file.trim_end_matches(".rs")
            .trim_end_matches("/mod")
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string()
    };
    let owned_by = |contract: &Contract, names: &[String]| {
        names
            .iter()
            .any(|name| name != "contract" && contract.owners.contains(name))
    };
    let imports = facts.imports.get(&binding.file);
    let narrowings: [Narrowing<'_>; 4] = [
        // crate::api::controllers::notes::contract::CreateNote::bind(..)
        Box::new(|contract| owned_by(contract, &binding.qualifier)),
        // use crate::api::controllers::notes::contract::{CreateNote, ..};
        Box::new(|contract| {
            imports.is_some_and(|paths| {
                paths.iter().any(|path| {
                    (path.last() == Some(&binding.target)
                        || binding.qualifier.first() == path.last())
                        && owned_by(contract, path)
                })
            })
        }),
        // Declared and bound in the same file.
        Box::new(|contract| contract.file == binding.file),
        // api/routes/notes.rs binds api/controllers/notes.rs.
        Box::new(|contract| contract.owners.contains(&file_stem(&binding.file))),
    ];
    for narrowing in &narrowings {
        let narrowed: Vec<&Contract> = candidates
            .iter()
            .copied()
            .filter(|contract| narrowing(contract))
            .collect();
        if !narrowed.is_empty() && one_module(&narrowed) {
            return narrowed;
        }
    }
    Vec::new()
}

/// Read the routes out of parsed sources.
fn analyze(sources: &RustSources) -> RouteScan {
    let mut facts = Facts::default();
    for file in &sources.files {
        let mut reader = Reader {
            file: &file.rel,
            facts: &mut facts,
            modules: Vec::new(),
            prefixes: Vec::new(),
            seen: BTreeSet::new(),
        };
        reader.visit_file(&file.ast);
    }

    // The policy each endpoint is mounted with, and whether anything mounts it publicly.
    let mut policies: BTreeMap<(String, usize), (Option<String>, bool, bool)> = BTreeMap::new();
    for binding in &facts.bindings {
        for contract in resolve(binding, &facts) {
            let entry = policies
                .entry((contract.file.clone(), contract.line))
                .or_default();
            match binding.kind {
                BindKind::Authorized => {
                    entry.2 = true;
                    if entry.0.is_none() {
                        entry.0 = binding.policy.clone();
                    }
                }
                BindKind::Public => entry.1 = true,
            }
        }
    }

    let mut scan = RouteScan {
        routes: std::mem::take(&mut facts.routes),
        unreadable: sources.unreadable.clone(),
        undecided: std::mem::take(&mut facts.undecided),
    };
    for contract in &facts.contracts {
        let (policy, public_bind, authorized_bind) = policies
            .get(&(contract.file.clone(), contract.line))
            .cloned()
            .unwrap_or_default();
        let auth = contract.auth.clone().or(policy);
        if contract.access != "public" && auth.is_none() && !public_bind && !authorized_bind {
            // Declared non-public, and nothing in the scanned source mounts it. The policy
            // may be bound in another crate; that is unknown, not missing.
            scan.undecided.push(UndecidedRoute {
                file: contract.file.clone(),
                line: contract.line,
                what: format!("{} {}", contract.method, contract.path),
                why: format!(
                    "it declares access '{}' but no `bind_authorized(policy, ..)` for it was found in the scanned source",
                    contract.access
                ),
            });
            continue;
        }
        scan.routes.push(StaticRoute {
            method: contract.method.clone(),
            path: contract.path.clone(),
            access: Some(contract.access.clone()),
            auth,
            source: RouteSource::Contract,
            file: contract.file.clone(),
            line: contract.line,
        });
    }
    scan.routes
        .sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    scan.undecided
        .sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    scan
}

/// Every route the Rust sources under `root` register.
pub fn scan_rust_routes(root: &Path) -> RouteScan {
    analyze(&read_rust_sources(root))
}

/// The routes in `(relative path, source)` pairs.
#[cfg(test)]
pub fn rust_routes_in_sources(files: &[(&str, &str)]) -> RouteScan {
    analyze(&parse_sources(
        files
            .iter()
            .map(|(rel, text)| (rel.to_string(), text.to_string()))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTROLLER: &str = r#"
use forklaunch_axum::native::{EndpointError, State};

pub async fn create_note(State(state): State<AppState>, caller: Caller, input: NoteInput) -> Result<Note, EndpointError> {
    state.notes.create(&caller.owner_id, input).await
}

#[forklaunch_core::contract::api]
pub mod contract {
    use forklaunch_core::contract::{HttpMethod, endpoint::{Endpoint, JsonBody, NoBody}};

    #[access(Authenticated)]
    pub fn create_note() -> Endpoint<JsonBody<NoteInput>, JsonBody<Note>> {
        Endpoint::new("Create Note", "Create a note", HttpMethod::Post, "/notes", 201)
            .expect("valid create note endpoint")
    }
    #[access(Protected)]
    pub fn delete_note() -> Endpoint<NoBody, NoBody> {
        Endpoint::new("Delete Note", "Delete a note", HttpMethod::Delete, "/notes/:id", 204).unwrap()
    }
    #[access(Public)]
    pub fn list_public() -> Endpoint<NoBody, JsonBody<NoteList>> {
        Endpoint::new("List Public", "Public notes", HttpMethod::Get, "/public", 200).unwrap()
    }
    pub fn health() -> Endpoint<NoBody, JsonBody<Health>> {
        Endpoint::new("Get Health", "Reports health", HttpMethod::Get, "/health", 200).unwrap()
    }
    fn helper() -> u16 { 200 }
}
"#;

    const ROUTES: &str = r#"
use crate::api::controllers::notes::{contract::{Bindings, CreateNote, DeleteNote, Health, ListPublic}, create_note};
pub fn mount(router: ContractRouter<AppState>, auth: &BearerSession) -> Result<ContractRouter<AppState>, String> {
    Bindings {
        create_note: CreateNote::bind_authorized(auth.clone(), create_note),
        delete_note: DeleteNote::bind_authorized(RolePolicy::admin(auth), delete_note),
        list_public: ListPublic::bind(list_public),
        health: Health::bind(health),
    }
    .mount(router)
}
"#;

    fn summary(scan: &RouteScan) -> Vec<(String, Option<String>, Option<String>, RouteSource)> {
        scan.routes
            .iter()
            .map(|r| {
                (
                    format!("{} {}", r.method, r.path),
                    r.access.clone(),
                    r.auth.clone(),
                    r.source,
                )
            })
            .collect()
    }

    #[test]
    fn reads_access_from_the_contract_and_auth_from_the_binding() {
        let scan = rust_routes_in_sources(&[
            ("api/controllers/notes.rs", CONTROLLER),
            ("api/routes/notes.rs", ROUTES),
        ]);
        assert!(scan.unreadable.is_empty() && scan.undecided.is_empty());
        let contract = RouteSource::Contract;
        assert_eq!(
            summary(&scan),
            vec![
                (
                    "POST /notes".to_string(),
                    Some("authenticated".to_string()),
                    Some("auth.clone()".to_string()),
                    contract
                ),
                (
                    "DELETE /notes/:id".to_string(),
                    Some("protected".to_string()),
                    Some("RolePolicy::admin(auth)".to_string()),
                    contract
                ),
                (
                    "GET /public".to_string(),
                    Some("public".to_string()),
                    None,
                    contract
                ),
                (
                    "GET /health".to_string(),
                    Some("public".to_string()),
                    None,
                    contract
                ),
            ]
        );
        let first = &scan.routes[0];
        assert_eq!(
            (first.file.as_str(), first.line),
            ("api/controllers/notes.rs", 13)
        );
        assert!(scan.routes.iter().all(|route| !route.missing_auth()));
    }

    #[test]
    fn a_non_public_endpoint_mounted_without_a_policy_is_missing_auth() {
        // An unannotated declaration keeps the public `bind` constructor even when its body
        // raises the access level, so this compiles and is mounted with no policy.
        let controller = r#"
#[forklaunch_core::contract::api]
pub mod contract {
    pub fn export() -> Endpoint<NoBody, TextBody> {
        Endpoint::new("Export", "Export", HttpMethod::Get, "/export", 200)
            .unwrap()
            .with_access(AccessLevel::Protected)
    }
}
"#;
        let routes = "pub fn mount(r: ContractRouter<S>) -> Result<ContractRouter<S>, String> { Bindings { export: Export::bind(export) }.mount(r) }";
        let scan = rust_routes_in_sources(&[
            ("api/controllers/export.rs", controller),
            ("api/routes/export.rs", routes),
        ]);
        assert_eq!(scan.routes.len(), 1);
        let route = &scan.routes[0];
        assert_eq!(route.access.as_deref(), Some("protected"));
        assert_eq!(route.auth, None);
        assert!(route.missing_auth());
    }

    #[test]
    fn a_non_public_endpoint_with_no_binding_in_sight_is_undecided_not_failed() {
        let scan = rust_routes_in_sources(&[("api/controllers/notes.rs", CONTROLLER)]);
        let listed: Vec<_> = scan
            .routes
            .iter()
            .map(|r| format!("{} {}", r.method, r.path))
            .collect();
        assert_eq!(listed, vec!["GET /public", "GET /health"]);
        let undecided: Vec<_> = scan.undecided.iter().map(|u| u.what.as_str()).collect();
        assert_eq!(undecided, vec!["POST /notes", "DELETE /notes/:id"]);
        assert!(scan.undecided[0].why.contains("bind_authorized"));
    }

    #[test]
    fn endpoints_with_the_same_name_resolve_to_their_own_controller() {
        let health = r#"
#[forklaunch_core::contract::api]
pub mod contract {
    #[access(Internal)]
    pub fn status() -> Endpoint<NoBody, TextBody> { Endpoint::new("Status", "Status", HttpMethod::Get, "/internal/status", 200).unwrap() }
}
"#;
        let public = r#"
#[forklaunch_core::contract::api]
pub mod contract {
    pub fn status() -> Endpoint<NoBody, TextBody> { Endpoint::new("Status", "Status", HttpMethod::Get, "/status", 200).unwrap() }
}
"#;
        let scan = rust_routes_in_sources(&[
            ("api/controllers/internal.rs", health),
            ("api/controllers/public.rs", public),
            (
                "api/routes/internal.rs",
                "fn mount(r: R, hmac: Hmac) { Bindings { status: crate::api::controllers::internal::contract::Status::bind_authorized(hmac, status) }.mount(r) }",
            ),
            (
                "api/routes/public.rs",
                "use crate::api::controllers::public::contract::{Bindings, Status};\nfn mount(r: R) { Bindings { status: Status::bind(status) }.mount(r) }",
            ),
        ]);
        assert!(scan.undecided.is_empty());
        assert_eq!(
            summary(&scan),
            vec![
                (
                    "GET /internal/status".to_string(),
                    Some("internal".to_string()),
                    Some("hmac".to_string()),
                    RouteSource::Contract
                ),
                (
                    "GET /status".to_string(),
                    Some("public".to_string()),
                    None,
                    RouteSource::Contract
                ),
            ]
        );
    }

    #[test]
    fn reads_route_contracts_and_the_router_binding_form() {
        let source = r#"
pub fn router(service: ServiceInfo, policy: Hmac) -> ForkLaunchRouter {
    ForkLaunchRouter::new(service)
        .get(RouteContract::get("Health", "Health", "/health"), health)
        .post(
            RouteContract::post("Sync", "Sync", "/sync")
                .with_access(AccessLevel::Internal)
                .with_auth(AuthConfig::Hmac(HmacAuth::new())),
            sync,
        )
        .route(RouteContract::new("Drop", "Drop", HttpMethod::Delete, "/things/:id").with_access(AccessLevel::Protected), drop_thing)
}
fn wrapped() -> RouteContract { auth(RouteContract::put("Plan", "Plan", "/plan")) }
fn computed(path: &str) -> RouteContract { RouteContract::get("X", "X", path).with_access(AccessLevel::Public) }
pub fn native(policy: Hmac) -> Result<Router<S>, String> {
    ContractRouter::new(registry())
        .bind_authorized(contract::rotate(), policy, rotate)?
        .finish()
}
#[forklaunch_core::contract::api]
pub mod contract {
    #[access(Internal)]
    pub fn rotate() -> Endpoint<NoBody, NoBody> { Endpoint::new("Rotate", "Rotate", HttpMethod::Post, "/rotate", 204).unwrap() }
}
"#;
        let scan = rust_routes_in_sources(&[("api/http.rs", source)]);
        let contract = RouteSource::Contract;
        assert_eq!(
            summary(&scan),
            vec![
                (
                    "GET /health".to_string(),
                    Some("public".to_string()),
                    None,
                    contract
                ),
                (
                    "POST /sync".to_string(),
                    Some("internal".to_string()),
                    Some("AuthConfig::Hmac(HmacAuth::new())".to_string()),
                    contract
                ),
                (
                    "DELETE /things/:id".to_string(),
                    Some("protected".to_string()),
                    None,
                    contract
                ),
                (
                    "POST /rotate".to_string(),
                    Some("internal".to_string()),
                    Some("policy".to_string()),
                    contract
                ),
            ]
        );
        assert!(scan.routes[2].missing_auth(), "protected with no auth");
        let undecided: Vec<_> = scan
            .undecided
            .iter()
            .map(|u| (u.what.as_str(), u.line))
            .collect();
        assert_eq!(
            undecided,
            vec![("PUT /plan", 13), ("a route contract", 14)],
            "a contract finished elsewhere, and one with a computed path, are not guessed"
        );
    }

    #[test]
    fn lists_raw_axum_routes_with_method_chains_nest_and_merge() {
        let source = r#"
pub fn app(state: AppState) -> Router {
    let admin = Router::new().route("/users", get(users::list).post(users::create));
    Router::new()
        .route("/api/healthz", get(|| async { "ok" }))
        .route(
            "/api/documents/{id}",
            axum::routing::get(documents::detail).patch(documents::update).delete(documents::remove),
        )
        .route(LOGIN_PATH, post(auth::login))
        .route("/hook", on(MethodFilter::POST, hook))
        .route("/everything", any(everything))
        .nest("/api/v1", Router::new().route("/items", get(items).put(replace)).route_service("/files", files))
        .nest("/admin", admin)
        .nest_service("/assets", ServeDir::new("assets"))
        .merge(Router::new().route("/merged", MethodRouter::new().head(probe)))
        .with_state(state)
}
fn unrelated(map: &Map, router: ForkLaunchRouter) { map.get("key"); let _ = router.route(contract(), handler); }
"#;
        let scan = rust_routes_in_sources(&[("src/main.rs", source)]);
        let listed: Vec<_> = scan
            .routes
            .iter()
            .map(|r| (r.method.as_str(), r.path.as_str(), r.line))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("GET", "/users", 3),
                ("POST", "/users", 3),
                ("GET", "/api/healthz", 5),
                ("GET", "/api/documents/{id}", 6),
                ("PATCH", "/api/documents/{id}", 6),
                ("DELETE", "/api/documents/{id}", 6),
                ("POST", "LOGIN_PATH", 10),
                ("POST", "/hook", 11),
                ("ANY", "/everything", 12),
                ("GET", "/api/v1/items", 13),
                ("PUT", "/api/v1/items", 13),
                ("ANY", "/api/v1/files", 13),
                ("ANY", "/assets", 15),
                ("HEAD", "/merged", 16),
            ]
        );
        assert!(scan.routes.iter().all(|r| {
            r.source == RouteSource::OutsideFramework
                && r.access.is_none()
                && r.auth.is_none()
                && !r.missing_auth()
        }));
        assert!(scan.undecided.is_empty() && scan.unreadable.is_empty());
    }

    #[test]
    fn a_file_the_parser_cannot_read_is_reported_and_test_code_is_not_scanned() {
        let dir = std::env::temp_dir().join("fl-rust-route-scan");
        let _ = fs::remove_dir_all(&dir);
        let raw = "pub fn app() -> Router { Router::new().route(\"/x\", get(x)) }\n";
        for (path, body) in [
            (
                "svc/src/main.rs",
                format!(
                    "mod broken;\n#[cfg(test)]\nmod checks;\n{raw}#[cfg(test)]\nmod tests {{ fn t() {{ Router::new().route(\"/in-test\", get(x)); }} }}\n"
                ),
            ),
            (
                "svc/src/broken.rs",
                "pub fn app() -> Router { Router::new().route(\"/y\", get(y) ".to_string(),
            ),
            ("svc/src/checks.rs", raw.replace("/x", "/cfg-test-module")),
            ("svc/src/api_tests.rs", raw.replace("/x", "/tests-file")),
            ("svc/tests/http.rs", raw.replace("/x", "/integration")),
            (
                "svc/target/debug/build/out.rs",
                raw.replace("/x", "/target"),
            ),
            ("svc/benches/load.rs", "not rust at all {{{".to_string()),
        ] {
            let file = dir.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, body).unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&dir, dir.join("svc/src/loop")).unwrap();
        let scan = scan_rust_routes(&dir);
        assert_eq!(scan.unreadable, vec!["svc/src/broken.rs".to_string()]);
        let paths: Vec<_> = scan.routes.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["/x"]);
        assert_eq!(scan.routes[0].file, "svc/src/main.rs");
        let _ = fs::remove_dir_all(&dir);
    }
}
