//! What an application writes to its caches and object stores, read from its source.
//!
//! ForkLaunch services reach Redis and S3 through the framework clients: `TtlCache`
//! (`putRecord`, `enqueueRecord`, ...) and `ObjectStore` (`putObject`, `putFile`, ...). Each
//! write states its key and, for a cache, its TTL; a write that passes a tenant compliance
//! context is encrypted with that tenant's key before it is stored. The platform's
//! compliance documents describe that from this scan: what each service keeps in a cache
//! and for how long, what it puts in a bucket, and whether either is encrypted per tenant.
//!
//! Only files that use the framework clients are read (they name `TtlCache`/`ObjectStore`
//! or import the cache/object-store packages), so an unrelated `putObject` elsewhere is
//! not mistaken for one.
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, BinaryOperator, CallExpression, Expression, MemberExpression, ObjectExpression,
    ObjectPropertyKind, PropertyKey, Statement, VariableDeclarationKind,
};
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use serde::Serialize;

use super::route_analyzer::walk;

/// Cache writes, and the argument index of their compliance context.
const CACHE_WRITES: &[(&str, usize)] = &[
    ("putRecord", 1),
    ("putBatchRecords", 1),
    ("enqueueRecord", 2),
    ("enqueueBatchRecords", 2),
];

/// Object-store writes, and the argument index of their compliance context. `putFile` and
/// `presignUpload` take none: the bytes are stored as given, under the bucket's encryption.
const OBJECT_WRITES: &[(&str, Option<usize>)] = &[
    ("putObject", Some(1)),
    ("putBatchObjects", Some(1)),
    ("streamUploadObject", Some(1)),
    ("streamUploadBatchObjects", Some(1)),
    ("putFile", None),
    ("presignUpload", None),
];

const CACHE_MARKERS: &[&str] = &[
    "TtlCache",
    "@forklaunch/core/cache",
    "@forklaunch/infrastructure-redis",
];
const OBJECT_MARKERS: &[&str] = &[
    "ObjectStore",
    "@forklaunch/core/objectstore",
    "@forklaunch/infrastructure-s3",
];

/// One kind of write found in the code. Call sites that agree on module, store, operation,
/// key prefix, TTL and encryption are folded into one entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    /// The module (service) directory the call is in.
    pub module: String,
    /// `cache` or `objectstore`.
    pub store: &'static str,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_prefix: Option<String>,
    /// The TTL in whole seconds, when the code states it as a constant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,
    /// The TTL as written, when it is not a constant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_expression: Option<String>,
    /// A tenant compliance context is passed: the value is encrypted per tenant.
    pub tenant_encrypted: bool,
    /// Path of the first call site, relative to the modules directory.
    pub file: String,
    pub line: usize,
    pub occurrences: usize,
}

/// Every cache and object-store write under `modules_path`, sorted for a stable report.
pub fn scan_storage_usage(modules_path: &Path) -> Vec<StorageUsage> {
    let mut files = Vec::new();
    walk(modules_path, &mut files);
    let mut found = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(modules_path)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(source) = fs::read_to_string(&file) else {
            continue;
        };
        found.extend(storage_in_source(&source, &rel));
    }
    fold(found)
}

/// The writes in one file, unfolded (one per call).
pub fn storage_in_source(source: &str, rel_path: &str) -> Vec<StorageUsage> {
    let cache = CACHE_MARKERS.iter().any(|m| source.contains(m));
    let objects = OBJECT_MARKERS.iter().any(|m| source.contains(m));
    if !cache && !objects {
        return Vec::new();
    }
    let module = rel_path.split('/').next().unwrap_or("").to_string();
    let allocator = Allocator::default();
    let parsed = Parser::new(
        &allocator,
        source,
        SourceType::default().with_typescript(true),
    )
    .parse();
    let constants = file_constants(&parsed.program.body);
    let mut visitor = StorageVisitor {
        source,
        file: rel_path,
        module,
        cache,
        objects,
        constants,
        found: Vec::new(),
    };
    visitor.visit_program(&parsed.program);
    visitor.found
}

fn fold(found: Vec<StorageUsage>) -> Vec<StorageUsage> {
    let mut folded: BTreeMap<String, StorageUsage> = BTreeMap::new();
    for usage in found {
        let key = format!(
            "{}|{}|{}|{:?}|{:?}|{:?}|{}",
            usage.module,
            usage.store,
            usage.operation,
            usage.key_prefix,
            usage.ttl_seconds,
            usage.ttl_expression,
            usage.tenant_encrypted
        );
        folded
            .entry(key)
            .and_modify(|u| u.occurrences += 1)
            .or_insert(usage);
    }
    folded.into_values().collect()
}

/// What a top-level `const` in the file stands for: a number (`const TTL = 60 * 1000`) or a
/// cache-key prefix (`const userKey = createCacheKey('user')`).
#[derive(Debug, Clone)]
enum Constant {
    Number(f64),
    KeyPrefix(String),
}

fn file_constants<'a>(body: &[Statement<'a>]) -> HashMap<String, Constant> {
    let mut constants = HashMap::new();
    for stmt in body {
        let decl = match stmt {
            Statement::VariableDeclaration(d) => d,
            Statement::ExportDeclaration(e) => match &e.declaration {
                oxc_ast::ast::Declaration::VariableDeclaration(d) => d,
                _ => continue,
            },
            _ => continue,
        };
        if decl.kind != VariableDeclarationKind::Const {
            continue;
        }
        for d in &decl.declarations {
            let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &d.id else {
                continue;
            };
            let Some(init) = &d.init else { continue };
            let value = if let Some(n) = number(init, &constants) {
                Some(Constant::Number(n))
            } else {
                cache_key_factory(init).map(Constant::KeyPrefix)
            };
            if let Some(value) = value {
                constants.insert(id.name.to_string(), value);
            }
        }
    }
    constants
}

/// `createCacheKey('user')` makes keys `user:<id>`.
fn cache_key_factory(expr: &Expression) -> Option<String> {
    let Expression::CallExpression(call) = expr.without_parentheses() else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if callee.name != "createCacheKey" {
        return None;
    }
    match call.arguments.first()? {
        Argument::StringLiteral(s) => Some(format!("{}:", s.value.display())),
        _ => None,
    }
}

/// A numeric constant expression: literals, `*`, `+`, `-`, `/`, and file constants.
fn number(expr: &Expression, constants: &HashMap<String, Constant>) -> Option<f64> {
    match expr.without_parentheses() {
        Expression::NumericLiteral(n) => Some(n.value),
        Expression::Identifier(id) => match constants.get(id.name.as_str()) {
            Some(Constant::Number(n)) => Some(*n),
            _ => None,
        },
        Expression::BinaryExpression(b) => {
            let l = number(&b.left, constants)?;
            let r = number(&b.right, constants)?;
            match b.operator {
                BinaryOperator::Multiplication => Some(l * r),
                BinaryOperator::Addition => Some(l + r),
                BinaryOperator::Subtraction => Some(l - r),
                BinaryOperator::Division if r != 0.0 => Some(l / r),
                _ => None,
            }
        }
        _ => None,
    }
}

struct StorageVisitor<'s> {
    source: &'s str,
    file: &'s str,
    module: String,
    cache: bool,
    objects: bool,
    constants: HashMap<String, Constant>,
    found: Vec<StorageUsage>,
}

impl StorageVisitor<'_> {
    fn line_of(&self, offset: u32) -> usize {
        self.source[..(offset as usize).min(self.source.len())]
            .matches('\n')
            .count()
            + 1
    }

    fn text(&self, span: oxc_span::Span) -> String {
        let raw = &self.source[span.start as usize..span.end as usize];
        raw.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The fixed start of a key: a string, a template's leading text, or a
    /// `createCacheKey` prefix (called directly or through a file constant).
    fn key_prefix(&self, expr: &Expression) -> Option<String> {
        match expr.without_parentheses() {
            Expression::StringLiteral(s) => Some(s.value.display().to_string()),
            Expression::TemplateLiteral(t) => t
                .quasis
                .first()
                .map(|q| q.value.raw.to_string())
                .filter(|p| !p.is_empty()),
            Expression::CallExpression(call) => match &call.callee {
                Expression::Identifier(id) => match self.constants.get(id.name.as_str()) {
                    Some(Constant::KeyPrefix(p)) => Some(p.clone()),
                    _ => None,
                },
                callee => cache_key_factory(callee),
            },
            _ => None,
        }
    }

    fn object_key_prefix(&self, obj: &ObjectExpression) -> Option<String> {
        property(obj, "key").and_then(|v| self.key_prefix(v))
    }

    /// The record (or first record of a batch) a write is called with.
    fn first_object<'b, 'a>(arg: Option<&'b Argument<'a>>) -> Option<&'b ObjectExpression<'a>> {
        match arg? {
            Argument::ObjectExpression(o) => Some(o),
            Argument::ArrayExpression(a) => match a.elements.first()?.as_expression()? {
                Expression::ObjectExpression(o) => Some(o),
                _ => None,
            },
            _ => None,
        }
    }

    fn passes_context(call: &CallExpression, index: usize) -> bool {
        match call.arguments.get(index) {
            None => false,
            Some(Argument::Identifier(id)) => id.name != "undefined",
            Some(_) => true,
        }
    }
}

fn property<'b, 'a>(obj: &'b ObjectExpression<'a>, name: &str) -> Option<&'b Expression<'a>> {
    obj.properties.iter().find_map(|p| match p {
        ObjectPropertyKind::ObjectProperty(p) => {
            let key = match &p.key {
                PropertyKey::StaticIdentifier(k) => k.name.as_str(),
                PropertyKey::StringLiteral(k) => k.value.as_str().unwrap_or_default(),
                _ => return None,
            };
            (key == name).then_some(&p.value)
        }
        _ => None,
    })
}

impl<'a> Visit<'a> for StorageVisitor<'_> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Some(MemberExpression::StaticMemberExpression(m)) =
            call.callee.as_member_expression()
        {
            let op = m.property.name.as_str();
            let line = self.line_of(call.span.start);
            if self.cache {
                if let Some((_, ctx)) = CACHE_WRITES.iter().find(|(name, _)| *name == op) {
                    let enqueue = op.starts_with("enqueue");
                    let (key_prefix, ttl) = if enqueue {
                        (
                            call.arguments
                                .first()
                                .and_then(|a| a.as_expression())
                                .and_then(|e| self.key_prefix(e)),
                            None,
                        )
                    } else {
                        let record = Self::first_object(call.arguments.first());
                        (
                            record.and_then(|o| self.object_key_prefix(o)),
                            record.and_then(|o| property(o, "ttlMilliseconds")),
                        )
                    };
                    let (ttl_seconds, ttl_expression) = match ttl {
                        Some(expr) => match number(expr, &self.constants) {
                            Some(ms) if ms >= 0.0 => (Some((ms / 1000.0).round() as u64), None),
                            _ => (None, Some(self.text(expr.span()))),
                        },
                        None => (None, None),
                    };
                    self.found.push(StorageUsage {
                        module: self.module.clone(),
                        store: "cache",
                        operation: op.to_string(),
                        key_prefix,
                        ttl_seconds,
                        ttl_expression,
                        tenant_encrypted: Self::passes_context(call, *ctx),
                        file: self.file.to_string(),
                        line,
                        occurrences: 1,
                    });
                }
            }
            if self.objects {
                if let Some((_, ctx)) = OBJECT_WRITES.iter().find(|(name, _)| *name == op) {
                    let key_prefix = match op {
                        "putFile" | "presignUpload" => call
                            .arguments
                            .first()
                            .and_then(|a| a.as_expression())
                            .and_then(|e| self.key_prefix(e)),
                        _ => Self::first_object(call.arguments.first())
                            .and_then(|o| self.object_key_prefix(o)),
                    };
                    self.found.push(StorageUsage {
                        module: self.module.clone(),
                        store: "objectstore",
                        operation: op.to_string(),
                        key_prefix,
                        ttl_seconds: None,
                        ttl_expression: None,
                        tenant_encrypted: ctx.is_some_and(|i| Self::passes_context(call, i)),
                        file: self.file.to_string(),
                        line,
                        occurrences: 1,
                    });
                }
            }
        }
        oxc_ast_visit::walk::walk_call_expression(self, call);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION_SERVICE: &str = r#"
import { TtlCache, createCacheKey } from '@forklaunch/core/cache';
const SESSION_TTL = 60 * 60 * 1000;
const sessionKey = createCacheKey('session');
export class SessionService {
  constructor(private cache: TtlCache) {}
  async save(id: string, tenantId: string, value: unknown) {
    await this.cache.putRecord(
      { key: sessionKey(id), value, ttlMilliseconds: SESSION_TTL },
      { tenantId }
    );
  }
  async remember(id: string, value: unknown) {
    await this.cache.putRecord({ key: `otp:${id}`, value, ttlMilliseconds: this.cache.getTtlMilliseconds() });
  }
  async queue(job: unknown, tenantId: string) {
    await this.cache.enqueueRecord('jobs', job, { tenantId });
  }
}
"#;

    #[test]
    fn reads_keys_ttls_and_tenant_encryption_from_cache_writes() {
        let found = storage_in_source(SESSION_SERVICE, "iam/services/session.service.ts");
        assert_eq!(found.len(), 3);
        let save = &found[0];
        assert_eq!(save.module, "iam");
        assert_eq!(save.store, "cache");
        assert_eq!(save.key_prefix.as_deref(), Some("session:"));
        assert_eq!(save.ttl_seconds, Some(3600));
        assert!(save.tenant_encrypted);
        assert_eq!(save.line, 8);
        let remember = &found[1];
        assert_eq!(remember.key_prefix.as_deref(), Some("otp:"));
        assert_eq!(remember.ttl_seconds, None);
        assert_eq!(
            remember.ttl_expression.as_deref(),
            Some("this.cache.getTtlMilliseconds()")
        );
        assert!(!remember.tenant_encrypted);
        let queue = &found[2];
        assert_eq!(queue.operation, "enqueueRecord");
        assert_eq!(queue.key_prefix.as_deref(), Some("jobs"));
        assert!(queue.tenant_encrypted);
    }

    #[test]
    fn reads_object_store_writes() {
        let source = r#"
import type { ObjectStore } from '@forklaunch/core/objectstore';
export async function upload(store: ObjectStore<unknown>, id: string, tenantId: string, body: Buffer) {
  await store.putObject({ key: `reports/${id}.json`, body: 'x' }, { tenantId });
  await store.putFile(`uploads/${id}`, body, { contentType: 'application/pdf' });
  await store.putObject({ key: id, body: 'y' });
}
"#;
        let found = storage_in_source(source, "records/services/upload.ts");
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].key_prefix.as_deref(), Some("reports/"));
        assert!(found[0].tenant_encrypted);
        assert_eq!(found[1].operation, "putFile");
        assert_eq!(found[1].key_prefix.as_deref(), Some("uploads/"));
        assert!(
            !found[1].tenant_encrypted,
            "putFile never encrypts in the app"
        );
        assert_eq!(found[2].key_prefix, None);
        assert!(!found[2].tenant_encrypted);
    }

    #[test]
    fn ignores_files_that_do_not_use_the_framework_clients() {
        let source = "const s3 = new S3(); s3.putObject({ Bucket: 'b', Key: 'k' });";
        assert!(storage_in_source(source, "svc/aws.ts").is_empty());
    }

    #[test]
    fn folds_identical_call_sites() {
        let source = r#"
import { TtlCache } from '@forklaunch/core/cache';
function a(c: TtlCache) { c.putRecord({ key: 'x', value: 1, ttlMilliseconds: 1000 }); }
function b(c: TtlCache) { c.putRecord({ key: 'x', value: 2, ttlMilliseconds: 1000 }); }
"#;
        let folded = fold(storage_in_source(source, "svc/a.ts"));
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].occurrences, 2);
        assert_eq!(folded[0].line, 3);
    }
}
