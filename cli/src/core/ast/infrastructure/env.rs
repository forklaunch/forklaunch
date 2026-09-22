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
    /// Declared optionality, taken from the config-injector `type:` property
    /// wrapping this read.
    ///
    /// `None` means the sighting carried no type information at all — a bare
    /// `getEnvVar` or `process.env` read outside a config-injector chain. Such a
    /// sighting folds to "required": an undeclared read is precisely the case
    /// this design refuses to guess about.
    ///
    /// An inline fallback (`getEnvVar('X') ?? 'default'`) deliberately does
    /// *not* make a variable optional: the app would start on a value nobody
    /// chose. Only a schema-level `optional(...)` counts.
    pub optional: Option<bool>,
}

pub struct EnvVarVisitor {
    pub env_vars: Vec<EnvVarUsage>,
    /// Span starts of `getEnvVar(...)` calls already attributed to a
    /// config-injector property. The generic call pass consults this so a typed
    /// read is not recorded a second time as an untyped sighting.
    consumed_calls: HashSet<u32>,
}

impl EnvVarVisitor {
    pub fn new() -> Self {
        Self {
            env_vars: Vec::new(),
            consumed_calls: HashSet::new(),
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
                            .push((str_lit.value.to_string(), call.span.start));
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
                            var_name: str_lit.value.to_string(),
                            optional: None,
                        });
                    }
                }
            }
        }

        oxc_ast_visit::walk::walk_call_expression(self, call);
    }
}

/// Env var names whose read supplies its own default: `process.env.X ?? 12`,
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
    names: HashSet<String>,
}

impl DefaultedEnvCollector {
    fn record(&mut self, expr: &Expression<'_>) {
        match expr {
            Expression::StaticMemberExpression(member) => {
                if let Expression::StaticMemberExpression(inner) = &member.object {
                    if inner.property.name == "env" {
                        if let Expression::Identifier(ident) = &inner.object {
                            if ident.name == "process" {
                                self.names.insert(member.property.name.to_string());
                            }
                        }
                    }
                }
            }
            Expression::CallExpression(call) => {
                if let Expression::Identifier(ident) = &call.callee {
                    if ident.name == "getEnvVar" {
                        if let Some(arg) = call.arguments.first() {
                            if let Some(Expression::StringLiteral(lit)) = arg.as_expression() {
                                self.names.insert(lit.value.to_string());
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

impl<'a> Visit<'a> for DefaultedEnvCollector {
    fn visit_logical_expression(&mut self, expr: &oxc_ast::ast::LogicalExpression<'a>) {
        if matches!(
            expr.operator,
            oxc_ast::ast::LogicalOperator::Coalesce | oxc_ast::ast::LogicalOperator::Or
        ) {
            self.record(&expr.left);
        }

        oxc_ast_visit::walk::walk_logical_expression(self, expr);
    }
}

fn defaulted_env_names(source_code: &str) -> HashSet<String> {
    let allocator = Allocator::default();
    let ParserReturn { program, .. } = Parser::new(
        &allocator,
        source_code,
        SourceType::default().with_typescript(true),
    )
    .parse();

    let mut collector = DefaultedEnvCollector {
        names: HashSet::new(),
    };
    collector.visit_program(&program);
    collector.names
}

/// Visitor that extracts `process.env.IDENTIFIER` usage from source code.
pub struct ProcessEnvVisitor {
    pub env_vars: Vec<EnvVarUsage>,
}

impl ProcessEnvVisitor {
    pub fn new() -> Self {
        Self {
            env_vars: Vec::new(),
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
                                // A `process.env` read carries no declared
                                // type; it folds to required.
                                optional: None,
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
/// outside a config-injector chain there is no `type:` to read, so every
/// sighting it produces is untyped by construction. Declared optionality is
/// read only from a project's `registrations.ts`, which is the declaration
/// surface this step is scoped to.
pub fn extract_untyped_env_vars_from_source(source_code: &str) -> Result<Vec<EnvVarUsage>> {
    let allocator = Allocator::default();

    let ParserReturn {
        program, errors, ..
    } = Parser::new(
        &allocator,
        source_code,
        SourceType::default().with_typescript(true),
    )
    .parse();

    if !errors.is_empty() {
        log::debug!("TypeScript parse errors during env scan: {:?}", errors);
    }

    let mut collector = GetEnvVarCollector { found: Vec::new() };
    collector.visit_program(&program);

    let defaulted = defaulted_env_names(source_code);

    Ok(collector
        .found
        .into_iter()
        .map(|(var_name, _)| EnvVarUsage {
            optional: defaulted.contains(&var_name).then_some(true),
            var_name,
        })
        .collect())
}

pub fn extract_process_env_vars_from_source(source_code: &str) -> Result<Vec<EnvVarUsage>> {
    let allocator = Allocator::default();

    let ParserReturn {
        program, errors, ..
    } = Parser::new(
        &allocator,
        source_code,
        SourceType::default().with_typescript(true),
    )
    .parse();

    if !errors.is_empty() {
        log::debug!(
            "TypeScript parse errors during process.env scan: {:?}",
            errors
        );
    }

    let mut visitor = ProcessEnvVisitor::new();
    visitor.visit_program(&program);

    let defaulted = defaulted_env_names(source_code);

    Ok(visitor
        .env_vars
        .into_iter()
        .map(|usage| EnvVarUsage {
            optional: defaulted
                .contains(&usage.var_name)
                .then_some(true)
                .or(usage.optional),
            ..usage
        })
        .collect())
}

/// Recursively find all `.ts` source files under a directory,
/// excluding `node_modules`, `.d.ts` files, and test directories.
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
        || dir_name == "__test__"
        || dir_name == "__tests__"
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

            if file_name.ends_with(".ts") && !file_name.ends_with(".d.ts") {
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
        program, errors, ..
    } = Parser::new(
        &allocator,
        source_code,
        SourceType::default().with_typescript(true),
    )
    .parse();

    if !errors.is_empty() {
        log::debug!("TypeScript parse errors during env scan: {:?}", errors);
    }

    let mut visitor = EnvVarVisitor::new();
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

    // Step 1: registrations.ts files — config-injector chains, and the only
    // place a declared `optional(...)` type is visible to the scanner.
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
                        if let Ok(vars) = extract_untyped_env_vars_from_source(&source_code) {
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
    fn test_sweep_extraction_carries_no_type() {
        // Declared optionality is read only from registrations.ts. The
        // whole-tree sweep reports names alone, even where the source happens to
        // contain a config injector of its own, so nothing outside the
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
}
