//! Every HTTP route an application registers, read from its source: no build, no install,
//! no running service.
//!
//! A ForkLaunch route is a contract: `handlers.get(schemaValidator, '/path', { access, auth,
//! ... }, handler)`. The contract states its access level and, unless it is public, how callers
//! authenticate; the framework enforces both. A route registered on the raw HTTP application
//! with an inline handler (`app.get('/ready', (req, res) => ...)`) is outside the framework, so
//! none of that applies to it: it is listed so a person can confirm it is meant to be open.
//!
//! The scan reads production TypeScript under the modules directory (tests, fixtures and build
//! output are skipped) with the same parser the rest of the CLI uses.
use std::{
    fs,
    path::{Path, PathBuf},
};

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, CallExpression, Expression, MemberExpression, ObjectPropertyKind, PropertyKey,
};
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use serde::Serialize;

const VERBS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "head", "options", "trace", "all",
];
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    "coverage",
    ".git",
    "__test__",
    "__tests__",
    "test",
    "tests",
    "fixtures",
    "__fixtures__",
    "e2e",
    "scripts",
    "seeders",
    "seeds",
    "migrations",
];

/// Where a route's protection comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteSource {
    /// A ForkLaunch contract: the framework applies its access level and auth.
    Contract,
    /// Registered on the raw HTTP application with an inline handler.
    OutsideFramework,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticRoute {
    pub method: String,
    /// The path as written at the registration (without any router mount prefix).
    pub path: String,
    /// `public`, `authenticated`, `protected` or `internal`; `None` when the contract does not
    /// state one inline (or the route is outside the framework).
    pub access: Option<String>,
    /// The contract's `auth` expression as written, e.g. `jwtAuth(MEMBER_ROLES)`.
    pub auth: Option<String>,
    pub source: RouteSource,
    /// Path relative to the scanned root, with `/` separators.
    pub file: String,
    pub line: usize,
}

impl StaticRoute {
    /// A contract route that should require a credential but declares no way to check one.
    pub fn missing_auth(&self) -> bool {
        self.source == RouteSource::Contract
            && self.access.as_deref() != Some("public")
            && self.auth.is_none()
    }
}

fn is_production_ts(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    (name.ends_with(".ts") || name.ends_with(".mts"))
        && !name.ends_with(".d.ts")
        && !name.contains(".test.")
        && !name.contains(".spec.")
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
        } else if is_production_ts(&path) {
            out.push(path);
        }
    }
}

/// The routes found under a root, and the files the parser could not fully read (their routes
/// may be missing, so a scan with any is incomplete).
#[derive(Debug, Default)]
pub struct RouteScan {
    pub routes: Vec<StaticRoute>,
    pub unreadable: Vec<String>,
}

/// Every route under `root`, sorted by file and line.
pub fn scan_routes(root: &Path) -> RouteScan {
    let mut files = Vec::new();
    walk(root, &mut files);
    let mut scan = RouteScan::default();
    for file in files {
        let rel = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(source) = fs::read_to_string(&file) else {
            scan.unreadable.push(rel);
            continue;
        };
        let (routes, complete) = parse_routes(&source, &rel);
        scan.routes.extend(routes);
        if !complete {
            scan.unreadable.push(rel);
        }
    }
    scan
}

/// The routes registered in one TypeScript source file.
#[cfg(test)]
pub fn routes_in_source(source: &str, rel_path: &str) -> Vec<StaticRoute> {
    parse_routes(source, rel_path).0
}

/// The routes in one file, and whether the parser read all of it.
fn parse_routes(source: &str, rel_path: &str) -> (Vec<StaticRoute>, bool) {
    let allocator = Allocator::default();
    let parsed = Parser::new(
        &allocator,
        source,
        SourceType::default().with_typescript(true),
    )
    .parse();
    let complete = !parsed.panicked && parsed.errors.is_empty();
    let mut v = RouteVisitor {
        source,
        file: rel_path,
        routes: Vec::new(),
    };
    v.visit_program(&parsed.program);
    (v.routes, complete)
}

struct RouteVisitor<'s> {
    source: &'s str,
    file: &'s str,
    routes: Vec<StaticRoute>,
}

impl RouteVisitor<'_> {
    fn line_of(&self, offset: u32) -> usize {
        self.source[..(offset as usize).min(self.source.len())]
            .matches('\n')
            .count()
            + 1
    }

    fn text(&self, span: oxc_span::Span) -> String {
        self.source[span.start as usize..span.end as usize].to_string()
    }
}

fn string_arg(arg: &Argument) -> Option<String> {
    match arg {
        Argument::StringLiteral(s) => Some(s.value.to_string()),
        Argument::TemplateLiteral(t) if t.expressions.is_empty() => {
            t.quasis.first().map(|q| q.value.raw.to_string())
        }
        _ => None,
    }
}

fn is_inline_function(arg: &Argument) -> bool {
    matches!(
        arg,
        Argument::ArrowFunctionExpression(_) | Argument::FunctionExpression(_)
    )
}

/// `app`, `server`, `this.app`, `x.internal`: the raw HTTP application rather than a ForkLaunch router.
fn is_raw_application(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => matches!(
            id.name.as_str(),
            "app" | "server" | "application" | "expressApp" | "httpServer" | "rawApp"
        ),
        Expression::StaticMemberExpression(m) => {
            m.property.name == "internal"
                || m.property.name == "app"
                || m.property.name == "expressApp"
        }
        _ => false,
    }
}

impl<'a> Visit<'a> for RouteVisitor<'_> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Some(MemberExpression::StaticMemberExpression(m)) =
            call.callee.as_member_expression()
        {
            let verb = m.property.name.as_str();
            if VERBS.contains(&verb) {
                let is_handlers =
                    matches!(&m.object, Expression::Identifier(id) if id.name == "handlers");
                if is_handlers {
                    // handlers.<verb>(schemaValidator, path, contract, ...handlers)
                    if let (Some(path), Some(contract)) = (
                        call.arguments.get(1).and_then(string_arg),
                        call.arguments.get(2),
                    ) {
                        let (mut access, mut auth) = (None, None);
                        if let Argument::ObjectExpression(obj) = contract {
                            for prop in &obj.properties {
                                if let ObjectPropertyKind::ObjectProperty(p) = prop {
                                    let key = match &p.key {
                                        PropertyKey::StaticIdentifier(k) => k.name.as_str(),
                                        PropertyKey::StringLiteral(k) => k.value.as_str(),
                                        _ => continue,
                                    };
                                    match key {
                                        "access" => {
                                            if let Expression::StringLiteral(s) = &p.value {
                                                access = Some(s.value.to_string());
                                            }
                                        }
                                        "auth" => auth = Some(self.text(p.value.span())),
                                        _ => {}
                                    }
                                }
                            }
                        }
                        self.routes.push(StaticRoute {
                            method: verb.to_uppercase(),
                            path,
                            access,
                            auth,
                            source: RouteSource::Contract,
                            file: self.file.to_string(),
                            line: self.line_of(call.span.start),
                        });
                    }
                } else if let Some(path) = call.arguments.first().and_then(string_arg) {
                    // app.get('/ready', (req, res) => ...): a route the framework never sees.
                    // An inline handler, or any handler on the raw application, never passes through
                    // a contract. A router wired to a `handlers.*` contract is not listed here.
                    let inline = call.arguments.iter().skip(1).any(is_inline_function);
                    if inline || (is_raw_application(&m.object) && call.arguments.len() >= 2) {
                        self.routes.push(StaticRoute {
                            method: verb.to_uppercase(),
                            path,
                            access: None,
                            auth: None,
                            source: RouteSource::OutsideFramework,
                            file: self.file.to_string(),
                            line: self.line_of(call.span.start),
                        });
                    }
                }
            }
        }
        oxc_ast_visit::walk::walk_call_expression(self, call);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTROLLER: &str = r#"
import { handlers, schemaValidator, jwtAuth } from '@credpath/core';
export const listRequests = handlers.get(
  schemaValidator,
  '/requests',
  {
    name: 'List requests',
    access: 'protected',
    auth: jwtAuth(OFFICE_ROLES),
    responses: { 200: Rows }
  },
  async (req, res) => res.status(200).json([])
);
export const discovery = handlers.get(schemaValidator, '/auth-methods', { name: 'Auth methods', access: 'public', responses: {} }, async () => {});
export const forgotten = handlers.post(schemaValidator, `/notes`, { name: 'Add note', access: 'protected', body: Note, responses: {} }, async () => {});
export const nolevel = handlers.delete(schemaValidator, '/notes/:id', { name: 'Delete note', responses: {} }, async () => {});
export const internal = handlers.post(schemaValidator, '/sync', { name: 'Sync', access: 'internal', auth: { hmac: { secretKeys: KEYS } }, responses: {} }, async () => {});
"#;

    #[test]
    fn reads_access_and_auth_from_contracts() {
        let routes = routes_in_source(CONTROLLER, "svc/api/controllers/x.controller.ts");
        assert_eq!(routes.len(), 5);
        let r = &routes[0];
        assert_eq!((r.method.as_str(), r.path.as_str()), ("GET", "/requests"));
        assert_eq!(r.access.as_deref(), Some("protected"));
        assert_eq!(r.auth.as_deref(), Some("jwtAuth(OFFICE_ROLES)"));
        assert_eq!(r.line, 3);
        assert!(!r.missing_auth());
        assert!(!routes[1].missing_auth(), "public routes need no auth");
        assert!(routes[2].missing_auth(), "protected without auth");
        assert_eq!(routes[2].path, "/notes");
        assert!(routes[3].missing_auth(), "no access level and no auth");
        assert_eq!(routes[3].access, None);
        assert!(!routes[4].missing_auth());
        assert!(routes.iter().all(|r| r.source == RouteSource::Contract));
    }

    #[test]
    fn lists_routes_registered_outside_the_framework() {
        let server = r#"
const app = forklaunchExpress(schemaValidator, otel);
app.internal.get('/ready', (_req, res) => { res.status(200).send('ok'); });
app.get('/api/auth/test-callback', async (req, res) => res.redirect(String(req.query.callbackURL)));
app.use('/credentialing', credentialingRouter);
credentialingRouter.get('/', listRequests);
const m = new Map(); m.get('key');
"#;
        let routes = routes_in_source(server, "svc/server.ts");
        let paths: Vec<_> = routes
            .iter()
            .map(|r| (r.method.as_str(), r.path.as_str(), r.source))
            .collect();
        assert_eq!(
            paths,
            vec![
                ("GET", "/ready", RouteSource::OutsideFramework),
                (
                    "GET",
                    "/api/auth/test-callback",
                    RouteSource::OutsideFramework
                ),
            ],
            "router mounts, framework-wired router routes and Map.get are not raw routes"
        );
        assert!(
            !routes[0].missing_auth(),
            "outside-framework routes are reported separately"
        );
    }

    #[test]
    fn skips_tests_and_build_output() {
        let dir = std::env::temp_dir().join("fl-route-scan-skip");
        let _ = fs::remove_dir_all(&dir);
        for (p, body) in [
            ("svc/api/controllers/a.controller.ts", CONTROLLER),
            ("svc/__test__/a.test.ts", CONTROLLER),
            ("svc/dist/a.controller.ts", CONTROLLER),
            ("svc/node_modules/x/index.ts", CONTROLLER),
        ] {
            let f = dir.join(p);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, body).unwrap();
        }
        let scan = scan_routes(&dir);
        assert_eq!(scan.routes.len(), 5);
        assert!(
            scan.routes
                .iter()
                .all(|r| r.file == "svc/api/controllers/a.controller.ts")
        );
        assert!(scan.unreadable.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reports_files_the_parser_cannot_read_and_ignores_symlinks() {
        let dir = std::env::temp_dir().join("fl-route-scan-broken");
        let _ = fs::remove_dir_all(&dir);
        let svc = dir.join("svc");
        fs::create_dir_all(svc.join("api")).unwrap();
        fs::write(svc.join("api/ok.controller.ts"), CONTROLLER).unwrap();
        fs::write(
            svc.join("api/broken.controller.ts"),
            "export const x = handlers.get(schemaValidator, '/x', { access: 'protected' ",
        )
        .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&dir, svc.join("loop")).unwrap();
        let scan = scan_routes(&dir);
        assert_eq!(
            scan.unreadable,
            vec!["svc/api/broken.controller.ts".to_string()]
        );
        assert_eq!(
            scan.routes
                .iter()
                .filter(|r| r.file == "svc/api/ok.controller.ts")
                .count(),
            5
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
