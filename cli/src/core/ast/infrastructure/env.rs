use std::{collections::HashSet, fs, path::Path};

use anyhow::Result;
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    CallExpression, Expression, MemberExpression, ObjectProperty, ObjectPropertyKind, PropertyKey,
};
use oxc_ast_visit::Visit;
use oxc_parser::{Parser, ParserReturn};
use oxc_span::SourceType;

use crate::core::rendered_template::RenderedTemplatesCache;

#[derive(Debug, Clone)]
pub struct EnvVarUsage {
    pub var_name: String,
    /// Optionality of this read. A config-injector `type:` declaration takes
    /// precedence; an untyped read can be optional through its own fallback.
    ///
    /// `None` means the sighting carried no type information at all — a bare
    /// `getEnvVar` or `process.env` read outside a config-injector chain. Such a
    /// sighting folds to "required": an undeclared read is precisely the case
    /// this design refuses to guess about.
    ///
    /// A fallback never overrides an explicitly required schema declaration,
    /// and never changes another read of the same variable into an optional one.
    pub optional: Option<bool>,
}

pub struct EnvVarVisitor {
    pub env_vars: Vec<EnvVarUsage>,
    /// Span starts of `getEnvVar(...)` calls already attributed to a
    /// config-injector property. The generic call pass consults this so a typed
    /// read is not recorded a second time as an untyped sighting.
    consumed_calls: HashSet<u32>,
    defaulted_reads: HashSet<u32>,
}

impl EnvVarVisitor {
    pub fn new() -> Self {
        Self {
            env_vars: Vec::new(),
            consumed_calls: HashSet::new(),
            defaulted_reads: HashSet::new(),
        }
    }
}

/// True for a config-injector type of the form `optional(...)`.
fn is_optional_type(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            matches!(&call.callee, Expression::Identifier(ident) if ident.name == "optional")
        }
        _ => false,
    }
}

/// Collects every `getEnvVar('NAME')` read inside a single expression, with the
/// span of each call so the outer visitor can mark it as already attributed.
struct GetEnvVarCollector {
    found: Vec<(String, u32)>,
}

impl<'a> Visit<'a> for GetEnvVarCollector {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Expression::Identifier(ident) = &call.callee {
            if ident.name == "getEnvVar" {
                if let Some(arg) = call.arguments.first() {
                    if let Some(Expression::StringLiteral(str_lit)) = arg.as_expression() {
                        self.found
                            .push((str_lit.value.display().to_string(), call.span.start));
                    }
                }
            }
        }

        oxc_ast_visit::walk::walk_call_expression(self, call);
    }
}

impl<'a> Visit<'a> for EnvVarVisitor {
    /// Handles the config-injector shape, the only place a declared type and
    /// the read that uses it sit together:
    ///
    /// ```ignore
    /// VERSION: {
    ///   lifetime: Lifetime.Singleton,
    ///   type: optional(string),
    ///   value: getEnvVar('VERSION') ?? 'v1'
    /// }
    /// ```
    fn visit_object_property(&mut self, prop: &ObjectProperty<'a>) {
        if let Expression::ObjectExpression(config) = &prop.value {
            let mut declared_type = None;
            let mut value_expr = None;

            for kind in &config.properties {
                let ObjectPropertyKind::ObjectProperty(inner) = kind else {
                    continue;
                };
                let PropertyKey::StaticIdentifier(key) = &inner.key else {
                    continue;
                };
                match key.name.as_str() {
                    "type" => declared_type = Some(&inner.value),
                    "value" => value_expr = Some(&inner.value),
                    _ => {}
                }
            }

            if let (Some(declared_type), Some(value_expr)) = (declared_type, value_expr) {
                let optional = is_optional_type(declared_type);
                let mut collector = GetEnvVarCollector { found: Vec::new() };
                collector.visit_expression(value_expr);

                for (var_name, span_start) in collector.found {
                    self.consumed_calls.insert(span_start);
                    self.env_vars.push(EnvVarUsage {
                        var_name,
                        optional: Some(optional),
                    });
                }
            }
        }

        oxc_ast_visit::walk::walk_object_property(self, prop);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Expression::Identifier(ident) = &call.callee {
            if ident.name == "getEnvVar" && !self.consumed_calls.contains(&call.span.start) {
                if let Some(arg) = call.arguments.first() {
                    if let Some(Expression::StringLiteral(str_lit)) = arg.as_expression() {
                        self.env_vars.push(EnvVarUsage {
                            var_name: str_lit.value.display().to_string(),
                            optional: self
                                .defaulted_reads
                                .contains(&call.span.start)
                                .then_some(true),
                        });
                    }
                }
            }
        }

        oxc_ast_visit::walk::walk_call_expression(self, call);
    }
}

/// Env read spans that supply their own default: `process.env.X ?? 12`,
/// `getEnvVar('X') || 'public'`.
///
/// A bare read folds to required because the scanner cannot see whether the
/// reader copes without a value. When the read is immediately defaulted it
/// CAN see: the fallback is the value the code uses when the variable is
/// absent, which is the definition of optional. Treating those as required
/// blocks a deploy on variables the application demonstrably runs without —
/// forklaunch-platform's staging deploy stalled on nine of them, every one
/// with a literal default a line away.
///
/// Only the left-hand side counts. `someOtherThing ?? process.env.X` is a
/// read of X as the fallback, and nothing defaults X itself.
struct DefaultedEnvCollector {
    reads: HashSet<u32>,
}

impl DefaultedEnvCollector {
    fn record(&mut self, expr: &Expression<'_>) {
        match expr {
            Expression::StaticMemberExpression(member) => {
                if let Expression::StaticMemberExpression(inner) = &member.object {
                    if inner.property.name == "env" {
                        if let Expression::Identifier(ident) = &inner.object {
                            if ident.name == "process" {
                                self.reads.insert(member.span.start);
                            }
                        }
                    }
                }
            }
            Expression::CallExpression(call) => {
                if let Expression::Identifier(ident) = &call.callee {
                    if ident.name == "getEnvVar" {
                        if let Some(arg) = call.arguments.first() {
                            if let Some(Expression::StringLiteral(_)) = arg.as_expression() {
                                self.reads.insert(call.span.start);
                            }
                        }
                    }
                }
            }
            // `(process.env.X) ?? d` and `process.env.X! ?? d` read the same.
            Expression::ParenthesizedExpression(inner) => self.record(&inner.expression),
            Expression::TSNonNullExpression(inner) => self.record(&inner.expression),
            _ => {}
        }
    }
}

/// Whether a fallback actually hands the reader a value.
///
/// `?? undefined` and `|| undefined` are not defaults. They are the shape you
/// write to normalise an absent-or-empty variable into an explicit `undefined`
/// for a caller that treats it as unset — `process.env.S3_URL || undefined`
/// exists so an empty env var is not passed to the S3 client as an endpoint.
/// `?? null` is the same. Counting those as defaults reported a variable the
/// code genuinely needs as optional, which is the worse direction to be wrong
/// in: the platform's config gate passes and the service fails at runtime.
///
/// Anything that does yield a value still counts, `?? DEFAULT_PATH` included.
/// Only the two spellings of "no value" are excluded.
fn supplies_a_value(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::NullLiteral(_) => false,
        // `undefined` is a global identifier in oxc, not a literal.
        Expression::Identifier(ident) => ident.name != "undefined",
        Expression::ParenthesizedExpression(inner) => supplies_a_value(&inner.expression),
        // `void 0` is `undefined` written the long way.
        Expression::UnaryExpression(unary) => unary.operator != oxc_ast::ast::UnaryOperator::Void,
        _ => true,
    }
}

impl<'a> Visit<'a> for DefaultedEnvCollector {
    fn visit_logical_expression(&mut self, expr: &oxc_ast::ast::LogicalExpression<'a>) {
        if matches!(
            expr.operator,
            oxc_ast::ast::LogicalOperator::Coalesce | oxc_ast::ast::LogicalOperator::Or
        ) && supplies_a_value(&expr.right)
        {
            self.record(&expr.left);
        }

        oxc_ast_visit::walk::walk_logical_expression(self, expr);
    }
}

fn defaulted_env_reads(source_code: &str) -> HashSet<u32> {
    let allocator = Allocator::default();
    let ParserReturn { program, .. } = Parser::new(
        &allocator,
        source_code,
        SourceType::default().with_typescript(true),
    )
    .parse();

    let mut collector = DefaultedEnvCollector {
        reads: HashSet::new(),
    };
    collector.visit_program(&program);
    collector.reads
}

/// Visitor that extracts `process.env.IDENTIFIER` usage from source code.
pub struct ProcessEnvVisitor {
    pub env_vars: Vec<EnvVarUsage>,
    defaulted_reads: HashSet<u32>,
}

impl ProcessEnvVisitor {
    pub fn new() -> Self {
        Self {
            env_vars: Vec::new(),
            defaulted_reads: HashSet::new(),
        }
    }
}

impl<'a> Visit<'a> for ProcessEnvVisitor {
    fn visit_member_expression(&mut self, expr: &MemberExpression<'a>) {
        // Match `process.env.IDENTIFIER`
        if let MemberExpression::StaticMemberExpression(static_member) = expr {
            let property_name = static_member.property.name.to_string();
            // Check that the object is `process.env`
            if let Expression::StaticMemberExpression(inner) = &static_member.object {
                if inner.property.name == "env" {
                    if let Expression::Identifier(ident) = &inner.object {
                        if ident.name == "process" {
                            self.env_vars.push(EnvVarUsage {
                                var_name: property_name,
                                // Only this read's own fallback can make it
                                // optional; another read of the name cannot.
                                optional: self
                                    .defaulted_reads
                                    .contains(&static_member.span.start)
                                    .then_some(true),
                            });
                        }
                    }
                }
            }
        }

        oxc_ast_visit::walk::walk_member_expression(self, expr);
    }
}

/// Collects `getEnvVar('NAME')` reads without attributing a declared type.
///
/// The whole-tree sweep uses this rather than [`extract_env_vars_from_source`]:
/// outside a config-injector chain there is no `type:` to read. Only a read's
/// own fallback can make an ordinary sighting optional. Declared optionality is
/// read from a project's `registrations.ts` and root `mikro-orm.config.ts`;
/// ordinary source files retain this conservative untyped extraction.
pub fn extract_untyped_env_vars_from_source(source_code: &str) -> Result<Vec<EnvVarUsage>> {
    let allocator = Allocator::default();

    let ParserReturn {
        program,
        diagnostics: errors,
        ..
    } = Parser::new(
        &allocator,
        source_code,
        SourceType::default().with_typescript(true),
    )
    .parse();

    if errors.has_errors() {
        log::debug!("TypeScript parse errors during env scan: {:?}", errors);
    }

    let mut collector = GetEnvVarCollector { found: Vec::new() };
    collector.visit_program(&program);

    let defaulted = defaulted_env_reads(source_code);

    Ok(collector
        .found
        .into_iter()
        .map(|(var_name, span)| EnvVarUsage {
            optional: defaulted.contains(&span).then_some(true),
            var_name,
        })
        .collect())
}

pub fn extract_process_env_vars_from_source(source_code: &str) -> Result<Vec<EnvVarUsage>> {
    let allocator = Allocator::default();

    let ParserReturn {
        program,
        diagnostics: errors,
        ..
    } = Parser::new(
        &allocator,
        source_code,
        SourceType::default().with_typescript(true),
    )
    .parse();

    if errors.has_errors() {
        log::debug!(
            "TypeScript parse errors during process.env scan: {:?}",
            errors
        );
    }

    let mut visitor = ProcessEnvVisitor::new();
    visitor.defaulted_reads = defaulted_env_reads(source_code);
    visitor.visit_program(&program);

    Ok(visitor.env_vars)
}

fn is_test_directory(name: &str) -> bool {
    matches!(name, "test" | "tests" | "__test__" | "__tests__")
}

/// Recursively find runtime `.ts` source files, excluding test sources.
fn find_all_source_files(project_path: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut source_files = Vec::new();
    walk_source_files(project_path, &mut source_files)?;
    Ok(source_files)
}

fn walk_source_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
    if !dir.exists() || !dir.is_dir() {
        return Ok(());
    }

    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    if dir_name == "node_modules"
        || dir_name == "dist"
        || dir_name == ".git"
        || is_test_directory(&dir_name)
    {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            walk_source_files(&path, files)?;
        } else if path.is_file() {
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Test setup/cleanup assigns and deletes process.env values. Those
            // are not runtime requirements, even when tests live beside code.
            if file_name.ends_with(".ts")
                && !file_name.ends_with(".d.ts")
                && !file_name.ends_with(".test.ts")
                && !file_name.ends_with(".spec.ts")
            {
                files.push(path);
            }
        }
    }

    Ok(())
}

pub fn extract_env_vars_from_file(
    file_path: &Path,
    rendered_templates_cache: &RenderedTemplatesCache,
) -> Result<Vec<EnvVarUsage>> {
    let source_code = rendered_templates_cache.get(file_path)?.unwrap().content;

    extract_env_vars_from_source(&source_code)
}

pub fn extract_env_vars_from_source(source_code: &str) -> Result<Vec<EnvVarUsage>> {
    let allocator = Allocator::default();

    let ParserReturn {
        program,
        diagnostics: errors,
        ..
    } = Parser::new(
        &allocator,
        source_code,
        SourceType::default().with_typescript(true),
    )
    .parse();

    if errors.has_errors() {
        log::debug!("TypeScript parse errors during env scan: {:?}", errors);
    }

    let mut visitor = EnvVarVisitor::new();
    visitor.defaulted_reads = defaulted_env_reads(source_code);
    visitor.visit_program(&program);

    Ok(visitor.env_vars)
}

/// Folds every sighting of one variable into a single declared optionality.
///
/// This is the rule that decides how a name used inconsistently across files
/// resolves: a variable is optional only when *every* sighting of it says so.
///
/// Among sightings that carry a type, required wins — a name declared optional
/// in one place and required in another is required, because the required
/// reader is the one that breaks when the value is missing.
///
/// An untyped sighting (`None`) also counts as required. A bare `process.env.X`
/// read outside any config injector carries no declared type, so the scanner
/// cannot see whether that reader copes with a missing value, and an undeclared
/// read is exactly the case this design refuses to guess about.
///
/// Applied twice, with the same rule both times: once per project as sightings
/// are folded into variables, and again in `determine_env_var_scopes` when a
/// variable used by several projects is promoted to application scope.
pub(crate) fn fold_optionality(sightings: &[Option<bool>]) -> Option<bool> {
    if sightings.is_empty() {
        // Nothing was sighted at all — this is a synthesized variable rather
        // than one the scanner read out of source, so it has no optionality to
        // report and the manifest omits the field.
        return None;
    }

    // A variable is optional only when every sighting of it says so. A sighting
    // that carries no type at all — a bare `process.env` or `getEnvVar` read
    // outside any config injector — counts as required rather than abstaining:
    // the scanner cannot see whether that reader copes with a missing value, and
    // an undeclared read is exactly the case this design refuses to guess about.
    Some(sightings.iter().all(|sighting| *sighting == Some(true)))
}

/// Collapses raw sightings into one entry per variable name, preserving
/// first-seen order so manifest output stays stable across runs.
fn fold_sightings(sightings: Vec<EnvVarUsage>) -> Vec<EnvVarUsage> {
    let mut order: Vec<String> = Vec::new();
    let mut grouped: std::collections::HashMap<String, Vec<EnvVarUsage>> =
        std::collections::HashMap::new();

    for sighting in sightings {
        let name = sighting.var_name.clone();
        if !grouped.contains_key(&name) {
            order.push(name.clone());
        }
        grouped.entry(name).or_default().push(sighting);
    }

    order
        .into_iter()
        .map(|var_name| {
            let declared: Vec<Option<bool>> =
                grouped[&var_name].iter().map(|s| s.optional).collect();
            let optional = fold_optionality(&declared);
            EnvVarUsage { var_name, optional }
        })
        .collect()
}

pub fn find_all_env_vars(
    modules_path: &Path,
    rendered_templates_cache: &RenderedTemplatesCache,
) -> Result<std::collections::HashMap<String, Vec<EnvVarUsage>>> {
    // Every sighting is gathered first and folded at the end. The previous
    // implementation deduplicated as it went and kept the first sighting of a
    // name, which let filesystem order decide which one survived — fine when a
    // sighting was just a name, wrong once it also carries optionality.
    let mut sightings: std::collections::HashMap<String, Vec<EnvVarUsage>> =
        std::collections::HashMap::new();

    // Step 1: registrations.ts files — the main config-injector declarations.
    // Root mikro-orm.config.ts declarations are read in the source pass below.
    let registrations_files = find_registrations_files(modules_path)?;

    for file_path in &registrations_files {
        let project_name = get_project_name_from_path(file_path)?;
        let env_vars = extract_env_vars_from_file(file_path, rendered_templates_cache)?;
        sightings.entry(project_name).or_default().extend(env_vars);
    }

    // Step 2: Scan all .ts source files for process.env.* and getEnvVar() usage
    if modules_path.exists() {
        for entry in fs::read_dir(modules_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let project_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let source_files = find_all_source_files(&path)?;
                let mut extra_env_vars = Vec::new();

                // registrations.ts was already read above, with its declared
                // types intact. Reading it again here would add an untyped
                // sighting of every variable it declares, and an untyped
                // sighting folds to required — which would quietly force every
                // `optional(...)` declaration back to required.
                let registrations_path = path.join("registrations.ts");

                for source_file in source_files.iter() {
                    if source_file == &registrations_path {
                        continue;
                    }

                    if let Ok(source_code) = fs::read_to_string(&source_file) {
                        if let Ok(vars) = extract_process_env_vars_from_source(&source_code) {
                            extra_env_vars.extend(vars);
                        }
                        // The generated ORM config owns its own typed injector
                        // (including optional legacy encryption keys). Treating
                        // those declarations as bare reads invents requirements.
                        // Other files still use the conservative untyped scan.
                        let vars = if source_file == &path.join("mikro-orm.config.ts") {
                            extract_env_vars_from_source(&source_code)
                        } else {
                            extract_untyped_env_vars_from_source(&source_code)
                        };
                        if let Ok(vars) = vars {
                            extra_env_vars.extend(vars);
                        }
                    }
                }

                if !extra_env_vars.is_empty() {
                    sightings
                        .entry(project_name)
                        .or_default()
                        .extend(extra_env_vars);
                }
            }
        }
    }

    Ok(sightings
        .into_iter()
        .map(|(project_name, project_sightings)| (project_name, fold_sightings(project_sightings)))
        .collect())
}

fn find_registrations_files(modules_path: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut registrations_files = Vec::new();

    if !modules_path.exists() {
        return Ok(registrations_files);
    }

    for entry in fs::read_dir(modules_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|name| is_test_directory(&name.to_string_lossy()))
            {
                continue;
            }
            let registrations_path = path.join("registrations.ts");
            if registrations_path.exists() {
                registrations_files.push(registrations_path);
            }
        }
    }

    Ok(registrations_files)
}

fn get_project_name_from_path(file_path: &Path) -> Result<String> {
    let parent = file_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?;

    let project_name = parent
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Could not extract project name"))?
        .to_string_lossy()
        .to_string();

    Ok(project_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaulted_helper_setting_is_not_required_by_test_setup() {
        let root = tempfile::tempdir().unwrap();
        let backend = root.path().join("backend");
        fs::create_dir_all(backend.join("tests/claim-workspace")).unwrap();
        fs::write(
            backend.join("cache.ts"),
            r#"
            function integerEnv(name: string, fallback: number): number {
                const raw = process.env[name]?.trim();
                if (!raw) return fallback;
                const parsed = Number.parseInt(raw, 10);
                return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
            }
            const bytes = integerEnv('VERSION_CONTENT_CACHE_MAX_BYTES', 256 * 1024 * 1024);
        "#,
        )
        .unwrap();
        fs::write(
            backend.join("tests/claim-workspace/cache.unit.test.ts"),
            r#"
            process.env.VERSION_CONTENT_CACHE_MAX_BYTES = '1000';
            delete process.env.VERSION_CONTENT_CACHE_MAX_BYTES;
        "#,
        )
        .unwrap();
        let result = find_all_env_vars(root.path(), &RenderedTemplatesCache::new()).unwrap();
        assert!(
            result
                .values()
                .flatten()
                .all(|usage| usage.var_name != "VERSION_CONTENT_CACHE_MAX_BYTES")
        );
    }

    #[test]
    fn test_test_sources_do_not_create_deployment_requirements() {
        let root = tempfile::tempdir().unwrap();
        let backend = root.path().join("backend");
        fs::create_dir_all(backend.join("services")).unwrap();
        fs::write(
            backend.join("services/cache.ts"),
            "const limit = process.env.CACHE_LIMIT ?? 268435456; const token = process.env.REQUIRED_TOKEN;",
        ).unwrap();
        for relative in [
            "tests/claim-workspace/cache.unit.test.ts",
            "test/setup.ts",
            "__tests__/setup.ts",
            "__test__/setup.ts",
            "services/cache.test.ts",
            "services/cache.spec.ts",
        ] {
            let path = backend.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                "process.env.CACHE_LIMIT = '0'; delete process.env.TEST_ONLY;",
            )
            .unwrap();
        }
        // A tests-only workspace package must not acquire runtime requirements
        // through either the source sweep or the registrations pass.
        fs::create_dir(root.path().join("tests")).unwrap();
        fs::write(
            root.path().join("tests/registrations.ts"),
            "const token = getEnvVar('TEST_PACKAGE_TOKEN');",
        )
        .unwrap();
        let result = find_all_env_vars(root.path(), &RenderedTemplatesCache::new()).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result["backend"].len(), 2);
        assert_eq!(optionality(&result["backend"], "CACHE_LIMIT"), Some(true));
        assert_eq!(
            optionality(&result["backend"], "REQUIRED_TOKEN"),
            Some(false)
        );
    }

    #[test]
    fn test_runtime_files_in_nested_non_test_directories_are_still_scanned() {
        let root = tempfile::tempdir().unwrap();
        let backend = root.path().join("backend");
        fs::create_dir_all(backend.join("domain/testimonials")).unwrap();
        fs::write(
            backend.join("domain/testimonials/service.ts"),
            "const token = process.env.REQUIRED_TOKEN;",
        )
        .unwrap();
        let result = find_all_env_vars(root.path(), &RenderedTemplatesCache::new()).unwrap();
        assert_eq!(
            optionality(&result["backend"], "REQUIRED_TOKEN"),
            Some(false)
        );
    }

    #[test]
    fn test_extract_env_vars_basic() {
        let source = r#"
        const environmentConfig = configInjector.chain({
          HOST: {
            lifetime: Lifetime.Singleton,
            type: string,
            value: getEnvVar('HOST')
          },
          PORT: {
            lifetime: Lifetime.Singleton,
            type: number,
            value: Number(getEnvVar('PORT'))
          }
        });
        "#;

        let env_vars = extract_env_vars_from_source(source).unwrap();
        assert_eq!(env_vars.len(), 2);

        let var_names: HashSet<_> = env_vars.iter().map(|v| &v.var_name).collect();
        assert!(var_names.contains(&"HOST".to_string()));
        assert!(var_names.contains(&"PORT".to_string()));
    }

    #[test]
    fn test_extract_process_env_vars() {
        let source = r#"
        const host = process.env.HOST;
        const port = process.env.PORT;
        const dbName = process.env.DB_NAME;
        "#;

        let env_vars = extract_process_env_vars_from_source(source).unwrap();
        assert_eq!(env_vars.len(), 3);

        let var_names: HashSet<_> = env_vars.iter().map(|v| &v.var_name).collect();
        assert!(var_names.contains(&"HOST".to_string()));
        assert!(var_names.contains(&"PORT".to_string()));
        assert!(var_names.contains(&"DB_NAME".to_string()));
    }

    #[test]
    fn test_extract_process_env_vars_no_match() {
        let source = r#"
        const foo = someObj.env.BAR;
        const baz = process.config.QUX;
        "#;

        let env_vars = extract_process_env_vars_from_source(source).unwrap();
        assert_eq!(env_vars.len(), 0);
    }

    #[test]
    fn test_extract_env_vars_with_defaults() {
        let source = r#"
        const environmentConfig = configInjector.chain({
          VERSION: {
            lifetime: Lifetime.Singleton,
            type: optional(string),
            value: getEnvVar('VERSION') ?? 'v1'
          },
          CORS_ORIGINS: {
            lifetime: Lifetime.Singleton,
            type: array(string),
            value: getEnvVar('CORS_ORIGINS')?.split(',')
          }
        });
        "#;

        let env_vars = extract_env_vars_from_source(source).unwrap();
        assert_eq!(env_vars.len(), 2);

        let var_names: HashSet<_> = env_vars.iter().map(|v| &v.var_name).collect();
        assert!(var_names.contains(&"VERSION".to_string()));
        assert!(var_names.contains(&"CORS_ORIGINS".to_string()));

        // VERSION is declared `optional(string)`, so it is optional despite the
        // inline fallback. CORS_ORIGINS is optional-chained but declared
        // `array(string)` — an inline shape does not make it optional.
        assert_eq!(optionality(&env_vars, "VERSION"), Some(true));
        assert_eq!(optionality(&env_vars, "CORS_ORIGINS"), Some(false));
    }

    /// Optionality recorded for `name`, panicking if it was never sighted.
    fn optionality(env_vars: &[EnvVarUsage], name: &str) -> Option<bool> {
        env_vars
            .iter()
            .find(|v| v.var_name == name)
            .unwrap_or_else(|| panic!("{name} was not found by the scanner"))
            .optional
    }

    fn sighting(var_name: &str, optional: Option<bool>) -> EnvVarUsage {
        EnvVarUsage {
            var_name: var_name.to_string(),
            optional,
        }
    }

    #[test]
    fn test_declared_optionality_shapes() {
        let source = r#"
        const environmentConfig = configInjector.chain({
          OTEL_LEVEL: {
            lifetime: Lifetime.Singleton,
            type: optional(string),
            value: getEnvVar('OTEL_LEVEL') ?? 'info'
          },
          MAYBE_UNDEFINED: {
            lifetime: Lifetime.Singleton,
            type: optional(string),
            value: getEnvVar('MAYBE_UNDEFINED') ?? undefined
          },
          OPTIONAL_CHAINED: {
            lifetime: Lifetime.Singleton,
            type: optional(array(string)),
            value: getEnvVar('OPTIONAL_CHAINED')?.split(',')
          },
          FALLBACK_ONLY: {
            lifetime: Lifetime.Singleton,
            type: string,
            value: getEnvVar('FALLBACK_ONLY') ?? 'https://example.com'
          },
          WRAPPED: {
            lifetime: Lifetime.Singleton,
            type: number,
            value: Number(getEnvVar('WRAPPED'))
          },
          PLAIN: {
            lifetime: Lifetime.Singleton,
            type: string,
            value: getEnvVar('PLAIN')
          }
        });
        "#;

        let env_vars = extract_env_vars_from_source(source).unwrap();

        // A schema-level optional(...) is the only signal that counts.
        assert_eq!(optionality(&env_vars, "OTEL_LEVEL"), Some(true));
        assert_eq!(optionality(&env_vars, "MAYBE_UNDEFINED"), Some(true));
        assert_eq!(optionality(&env_vars, "OPTIONAL_CHAINED"), Some(true));

        // An inline fallback means the app starts on a value nobody chose —
        // that is exactly what this must keep flagging as required.
        assert_eq!(optionality(&env_vars, "FALLBACK_ONLY"), Some(false));
        assert_eq!(optionality(&env_vars, "WRAPPED"), Some(false));
        assert_eq!(optionality(&env_vars, "PLAIN"), Some(false));
    }

    #[test]
    fn test_bare_read_outside_config_injector_carries_no_type() {
        // No enclosing type, so the sighting records none. `fold_optionality`
        // is what turns an untyped sighting into "required"; at this level it is
        // simply absent.
        let source = r#"
        const url = getEnvVar('BETTER_AUTH_URL');
        "#;

        let env_vars = extract_env_vars_from_source(source).unwrap();
        assert_eq!(env_vars.len(), 1);
        assert_eq!(optionality(&env_vars, "BETTER_AUTH_URL"), None);
    }

    #[test]
    fn test_typed_read_is_recorded_once() {
        // The property pass and the generic call pass both see this call; only
        // the typed sighting should survive.
        let source = r#"
        const environmentConfig = configInjector.chain({
          PORT: {
            lifetime: Lifetime.Singleton,
            type: number,
            value: Number(getEnvVar('PORT'))
          }
        });
        "#;

        let env_vars = extract_env_vars_from_source(source).unwrap();
        assert_eq!(env_vars.len(), 1);
        assert_eq!(optionality(&env_vars, "PORT"), Some(false));
    }

    #[test]
    fn test_process_env_sighting_carries_no_type() {
        let source = r#"
        const host = process.env.DB_HOST;
        "#;

        let env_vars = extract_process_env_vars_from_source(source).unwrap();
        assert_eq!(env_vars.len(), 1);
        assert_eq!(
            optionality(&env_vars, "DB_HOST"),
            None,
            "the sighting itself carries no type"
        );
    }

    #[test]
    fn test_root_orm_declarations_preserve_optional_keys_without_hiding_required_readers() {
        let root = tempfile::tempdir().unwrap();
        let modules = root.path().join("modules");
        let iam = modules.join("iam");
        let records = modules.join("records");
        fs::create_dir_all(&iam).unwrap();
        fs::create_dir_all(&records).unwrap();
        // This is the injector shape emitted by the generated IAM/records ORM
        // configs. The records service also declares these in registrations.ts.
        let declarations = r#"
            const injector = createConfigInjector(schemaValidator, {
              ENCRYPTION_KEY: { type: string, value: getEnvVar('ENCRYPTION_KEY') },
              LEGACY_ENCRYPTION_KEYS: { type: optional(string), value: getEnvVar('LEGACY_ENCRYPTION_KEYS') }
            });
        "#;
        fs::write(iam.join("mikro-orm.config.ts"), declarations).unwrap();
        fs::write(records.join("mikro-orm.config.ts"), declarations).unwrap();
        fs::write(records.join("registrations.ts"), declarations).unwrap();
        let scan = || find_all_env_vars(&modules, &RenderedTemplatesCache::new()).unwrap();
        for project in ["iam", "records"] {
            let result = scan();
            assert_eq!(
                optionality(&result[project], "LEGACY_ENCRYPTION_KEYS"),
                Some(true)
            );
            assert_eq!(optionality(&result[project], "ENCRYPTION_KEY"), Some(false));
        }

        // A required declaration, a bare read elsewhere, and a process.env
        // reader inside the ORM file must EACH override the optional declaration.
        for extra in [
            "const keys = getEnvVar('LEGACY_ENCRYPTION_KEYS');",
            "const keys = process.env.LEGACY_ENCRYPTION_KEYS;",
            "const required = { KEYS: { type: string, value: getEnvVar('LEGACY_ENCRYPTION_KEYS') } };",
        ] {
            fs::write(
                iam.join("mikro-orm.config.ts"),
                format!("{declarations}\n{extra}"),
            )
            .unwrap();
            assert_eq!(
                optionality(&scan()["iam"], "LEGACY_ENCRYPTION_KEYS"),
                Some(false)
            );
        }
        fs::write(iam.join("mikro-orm.config.ts"), declarations).unwrap();
        fs::write(
            iam.join("reader.ts"),
            "const keys = getEnvVar('LEGACY_ENCRYPTION_KEYS');",
        )
        .unwrap();
        assert_eq!(
            optionality(&scan()["iam"], "LEGACY_ENCRYPTION_KEYS"),
            Some(false)
        );
        // A similarly named file in an arbitrary nested folder is not a new
        // trusted declaration surface.
        fs::remove_file(iam.join("reader.ts")).unwrap();
        fs::create_dir(iam.join("other")).unwrap();
        fs::write(iam.join("other/mikro-orm.config.ts"), declarations).unwrap();
        assert_eq!(
            optionality(&scan()["iam"], "LEGACY_ENCRYPTION_KEYS"),
            Some(false)
        );
    }

    #[test]
    fn test_orm_untyped_literal_fallback_remains_optional() {
        let root = tempfile::tempdir().unwrap();
        let service = root.path().join("iam");
        fs::create_dir(&service).unwrap();
        fs::write(
            service.join("mikro-orm.config.ts"),
            "const mode = getEnvVar('ORM_MODE') ?? 'safe';",
        )
        .unwrap();
        let result = find_all_env_vars(root.path(), &RenderedTemplatesCache::new()).unwrap();
        assert_eq!(optionality(&result["iam"], "ORM_MODE"), Some(true));
    }

    #[test]
    fn test_sweep_extraction_carries_no_type() {
        // Declared optionality is read only from recognized configuration
        // files. The ordinary source sweep reports names alone, even if it
        // contains a config injector of its own, so nothing outside the
        // declaration surface can claim a variable is optional.
        let source = r#"
        const configInjector = createConfigInjector(schemaValidator, {
          DB_DEBUG: {
            lifetime: Lifetime.Singleton,
            type: optional(string),
            value: getEnvVar('DB_DEBUG')
          }
        });
        "#;

        let env_vars = extract_untyped_env_vars_from_source(source).unwrap();

        assert_eq!(env_vars.len(), 1);
        assert_eq!(optionality(&env_vars, "DB_DEBUG"), None);
    }

    #[test]
    fn test_bare_read_elsewhere_overrides_declared_optional() {
        // A variable declared optional in registrations.ts and also read bare in
        // another file folds to required: the bare reader carries no type, and
        // the scanner will not guess that it copes with a missing value.
        let declared = extract_env_vars_from_source(
            r#"
            const c = configInjector.chain({
              OTEL_LEVEL: {
                lifetime: Lifetime.Singleton,
                type: optional(string),
                value: getEnvVar('OTEL_LEVEL')
              }
            });
            "#,
        )
        .unwrap();
        let bare = extract_untyped_env_vars_from_source(
            r#"
            const level = getEnvVar('OTEL_LEVEL');
            "#,
        )
        .unwrap();

        let mut sightings = declared;
        sightings.extend(bare);
        let folded = fold_sightings(sightings);

        assert_eq!(optionality(&folded, "OTEL_LEVEL"), Some(false));
    }

    #[test]
    fn test_fold_required_wins_over_optional() {
        // A name optional in one file and required in another resolves to
        // required — the required reader is the one that breaks when unset.
        let folded = fold_optionality(&[Some(true), Some(false)]);
        assert_eq!(folded, Some(false));
    }

    #[test]
    fn test_a_read_with_its_own_default_is_optional() {
        // Every one of these blocked forklaunch-platform's staging deploy as
        // "missing configuration" while the code ran fine without them.
        let source = r#"
const DRAIN_TIMEOUT_MS = Number(process.env.WORKER_DRAIN_TIMEOUT_MS ?? 110_000);
const minutes = Math.trunc(Number(process.env.WORKER_TASK_PROTECTION_MINUTES) || 12);
const schema = getEnvVar('DB_SCHEMA') || 'public';
const sid = getEnvVar('TWILIO_ACCOUNT_SID') || '';
"#;
        let process_vars = extract_process_env_vars_from_source(source).unwrap();
        let drain = process_vars
            .iter()
            .find(|v| v.var_name == "WORKER_DRAIN_TIMEOUT_MS")
            .expect("drain timeout sighted");
        assert_eq!(drain.optional, Some(true));

        let untyped = extract_untyped_env_vars_from_source(source).unwrap();
        for name in ["DB_SCHEMA", "TWILIO_ACCOUNT_SID"] {
            let found = untyped
                .iter()
                .find(|v| v.var_name == name)
                .unwrap_or_else(|| panic!("{name} sighted"));
            assert_eq!(
                found.optional,
                Some(true),
                "{name} supplies its own default"
            );
        }
    }

    #[test]
    fn test_a_bare_read_is_still_required() {
        // Unchanged: nothing here says what happens when the value is absent.
        let source = r#"
const region = process.env.AWS_REGION;
const url = getEnvVar('PLATFORM_URL');
"#;
        let process_vars = extract_process_env_vars_from_source(source).unwrap();
        assert_eq!(
            process_vars
                .iter()
                .find(|v| v.var_name == "AWS_REGION")
                .unwrap()
                .optional,
            None
        );
        let untyped = extract_untyped_env_vars_from_source(source).unwrap();
        assert_eq!(
            untyped
                .iter()
                .find(|v| v.var_name == "PLATFORM_URL")
                .unwrap()
                .optional,
            None
        );
    }

    #[test]
    fn test_a_read_used_as_the_fallback_is_not_defaulted() {
        // `x ?? process.env.Y` defaults x, not Y: Y is still required.
        let source = r#"
const value = configured ?? process.env.FALLBACK_ONLY;
"#;
        let vars = extract_process_env_vars_from_source(source).unwrap();
        assert_eq!(
            vars.iter()
                .find(|v| v.var_name == "FALLBACK_ONLY")
                .unwrap()
                .optional,
            None
        );
    }

    #[test]
    fn test_fold_untyped_sighting_forces_required() {
        // A bare read carries no type, so the scanner cannot tell whether that
        // reader copes without a value. It folds to required even alongside a
        // correct optional(...) declaration elsewhere.
        assert_eq!(fold_optionality(&[Some(true), None]), Some(false));

        // Nothing typed anywhere is the same case: required.
        assert_eq!(fold_optionality(&[None]), Some(false));

        // A variable is optional only when every sighting agrees.
        assert_eq!(fold_optionality(&[Some(true), Some(true)]), Some(true));

        // No sightings at all is a synthesized variable, not a required one.
        assert_eq!(fold_optionality(&[]), None);
    }

    #[test]
    fn test_fold_is_order_independent() {
        // The previous first-wins dedup let filesystem order decide which
        // sighting survived; folding must give the same answer either way.
        let forwards = fold_sightings(vec![
            sighting("A", Some(true)),
            sighting("A", Some(false)),
            sighting("B", None),
        ]);
        let backwards = fold_sightings(vec![
            sighting("A", Some(false)),
            sighting("A", Some(true)),
            sighting("B", None),
        ]);

        assert_eq!(forwards.len(), 2, "each name folds to exactly one entry");
        assert_eq!(optionality(&forwards, "A"), Some(false));
        assert_eq!(optionality(&backwards, "A"), Some(false));
        assert_eq!(optionality(&forwards, "B"), Some(false));
    }

    #[test]
    fn test_a_fallback_that_supplies_no_value_is_not_a_default() {
        // `?? undefined` and `|| undefined` are not defaults. They are the shape
        // you write to turn an absent variable into an explicit `undefined` for
        // a caller that treats it as "unset" — `process.env.S3_URL || undefined`
        // maps '' to undefined so an empty env var is not passed as an endpoint.
        //
        // Reading the left side and ignoring the right marked those optional, so
        // a variable the code genuinely needs sailed through the platform's
        // config gate and failed at runtime instead. Only a fallback that
        // supplies a value makes a read optional.
        let vars = extract_process_env_vars_from_source(
            r#"
            const a = process.env.NO_VALUE_COALESCE ?? undefined;
            const b = process.env.NO_VALUE_OR || undefined;
            const c = process.env.NULL_FALLBACK ?? null;
            const d = process.env.STRING_FALLBACK || 'ffmpeg';
            const e = process.env.NUMBER_FALLBACK ?? 8;
            const f = process.env.BOOL_FALLBACK ?? false;
            const g = process.env.CONST_FALLBACK ?? DEFAULT_PATH;
            "#,
        )
        .unwrap();

        assert_eq!(optionality(&vars, "NO_VALUE_COALESCE"), None);
        assert_eq!(optionality(&vars, "NO_VALUE_OR"), None);
        assert_eq!(optionality(&vars, "NULL_FALLBACK"), None);

        // Everything that does supply a value keeps counting, including a
        // non-literal fallback — `?? DEFAULT_PATH` is a real default.
        assert_eq!(optionality(&vars, "STRING_FALLBACK"), Some(true));
        assert_eq!(optionality(&vars, "NUMBER_FALLBACK"), Some(true));
        assert_eq!(optionality(&vars, "BOOL_FALLBACK"), Some(true));
        assert_eq!(optionality(&vars, "CONST_FALLBACK"), Some(true));
    }

    #[test]
    fn test_a_default_does_not_hide_a_required_read_in_the_same_file() {
        let source = r#"
            const optionalProcess = process.env.SHARED_PROCESS ?? 'fallback';
            const requiredProcess = process.env.SHARED_PROCESS;
            const optionalGetter = getEnvVar('SHARED_GETTER') || 'fallback';
            const requiredGetter = getEnvVar('SHARED_GETTER');
        "#;
        let process = fold_sightings(extract_process_env_vars_from_source(source).unwrap());
        let untyped = fold_sightings(extract_untyped_env_vars_from_source(source).unwrap());
        let declarations = fold_sightings(extract_env_vars_from_source(source).unwrap());
        assert_eq!(optionality(&process, "SHARED_PROCESS"), Some(false));
        assert_eq!(optionality(&untyped, "SHARED_GETTER"), Some(false));
        assert_eq!(optionality(&declarations, "SHARED_GETTER"), Some(false));
    }
}
