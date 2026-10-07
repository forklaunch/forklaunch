use std::{collections::{HashMap, HashSet}, fs, path::{Path, PathBuf}};

use anyhow::Result;
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    CallExpression, Declaration, Expression, ImportDeclarationSpecifier, ObjectPropertyKind,
    PropertyKey, Statement,
};
use oxc_ast_visit::Visit;
use oxc_parser::{Parser, ParserReturn};
use oxc_span::SourceType;

/// Extracted compliance metadata from an entity file.
#[derive(Debug, Clone)]
pub struct EntityComplianceInfo {
    pub entity_name: String,
    pub field_classifications: HashMap<String, String>,
    pub retention: Option<RetentionInfo>,
}

#[derive(Debug, Clone)]
pub struct RetentionInfo {
    pub duration: String,
    pub action: String,
}

/// Known base property sets and their classifications.
/// These are always compliance('none') and don't need file resolution.
const KNOWN_BASE_PROPERTIES: &[(&str, &[&str])] = &[
    (
        "sqlBaseProperties",
        &["id", "createdAt", "updatedAt", "retentionAnonymizedAt"],
    ),
    (
        "nosqlBaseProperties",
        &[
            "_id",
            "id",
            "createdAt",
            "updatedAt",
            "retentionAnonymizedAt",
        ],
    ),
];

fn get_known_base_classifications(name: &str) -> Option<HashMap<String, String>> {
    KNOWN_BASE_PROPERTIES.iter().find_map(|(base_name, fields)| {
        if *base_name == name {
            Some(
                fields
                    .iter()
                    .map(|f| (f.to_string(), "none".to_string()))
                    .collect(),
            )
        } else {
            None
        }
    })
}

/// Collected variable declarations from the file.
type VarDecls<'a> = HashMap<String, &'a Expression<'a>>;

/// Import mapping: local name -> (source module, original name)
type ImportMap = HashMap<String, (String, String)>;

/// First pass: collect all top-level const/let variable declarations.
fn collect_variable_declarations<'a>(program: &'a oxc_ast::ast::Program<'a>) -> VarDecls<'a> {
    let mut decls = HashMap::new();

    for stmt in &program.body {
        let var_decl = match stmt {
            Statement::VariableDeclaration(vd) => vd,
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                Some(Declaration::VariableDeclaration(vd)) => vd,
                _ => continue,
            },
            _ => continue,
        };

        for declarator in &var_decl.declarations {
            if let Some(init) = &declarator.init {
                if let oxc_ast::ast::BindingPatternKind::BindingIdentifier(id) =
                    &declarator.id.kind
                {
                    decls.insert(id.name.to_string(), init);
                }
            }
        }
    }

    decls
}

/// Collect import declarations: maps local name -> (source, original name).
fn collect_imports(program: &oxc_ast::ast::Program) -> ImportMap {
    let mut imports = HashMap::new();

    for stmt in &program.body {
        let import = match stmt {
            Statement::ImportDeclaration(import) => import,
            _ => continue,
        };

        let source = import.source.value.to_string();

        if let Some(specifiers) = &import.specifiers {
            for spec in specifiers {
                match spec {
                    ImportDeclarationSpecifier::ImportSpecifier(s) => {
                        let local = s.local.name.to_string();
                        let imported = s.imported.name().to_string();
                        imports.insert(local, (source.clone(), imported));
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                        let local = s.local.name.to_string();
                        imports.insert(local, (source.clone(), "default".to_string()));
                    }
                    _ => {}
                }
            }
        }
    }

    imports
}

/// Resolve an import source to a file path relative to the importing file.
/// Handles both relative imports (./foo, ../foo) and package imports (@scope/pkg).
fn resolve_import_path(import_source: &str, importing_file: &Path) -> Option<PathBuf> {
    if import_source.starts_with('.') {
        // Relative import
        let dir = importing_file.parent()?;
        resolve_ts_file(&dir.join(import_source))
    } else {
        // Package import — walk up to find node_modules
        let mut search_dir = importing_file.parent();
        while let Some(dir) = search_dir {
            let candidate = dir.join("node_modules").join(import_source);
            if let Some(resolved) = resolve_ts_file(&candidate) {
                return Some(resolved);
            }
            // Also try the package's src/index.ts or index.ts
            let pkg_dir = dir.join("node_modules").join(import_source);
            if pkg_dir.is_dir() {
                // Check package.json "main" or "exports" — simplified: just try index.ts
                if let Some(resolved) = resolve_ts_file(&pkg_dir.join("src").join("index")) {
                    return Some(resolved);
                }
            }
            search_dir = dir.parent();
        }
        None
    }
}

/// Try to resolve a path to a .ts file: exact, +.ts, /index.ts
fn resolve_ts_file(base: &Path) -> Option<PathBuf> {
    if base.is_file() {
        return Some(base.to_path_buf());
    }
    let with_ext = base.with_extension("ts");
    if with_ext.is_file() {
        return Some(with_ext);
    }
    let index = base.join("index.ts");
    if index.is_file() {
        return Some(index);
    }
    None
}

/// Resolve an identifier by following its import to the source file,
/// parsing that file, and extracting the variable's object expression.
/// Uses a visited set to prevent infinite loops on circular re-exports.
fn resolve_imported_variable(
    import_source: &str,
    original_name: &str,
    importing_file: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Option<HashMap<String, String>> {
    let resolved_path = resolve_import_path(import_source, importing_file)?;

    // Canonicalize to avoid visiting the same file via different relative paths
    let canonical = fs::canonicalize(&resolved_path).unwrap_or(resolved_path.clone());
    if !visited.insert(canonical.clone()) {
        return None; // Already visited this file
    }

    let source_code = fs::read_to_string(&resolved_path).ok()?;

    let allocator = Allocator::default();
    let ParserReturn { program, .. } = Parser::new(
        &allocator,
        &source_code,
        SourceType::default().with_typescript(true),
    )
    .parse();

    // Check re-exports (export * from './other', export { X } from './other')
    for stmt in &program.body {
        if let Statement::ExportAllDeclaration(export_all) = stmt {
            let re_export_source = export_all.source.value.to_string();
            if let Some(result) =
                resolve_imported_variable(&re_export_source, original_name, &resolved_path, visited)
            {
                return Some(result);
            }
        }
        if let Statement::ExportNamedDeclaration(export) = stmt {
            if let Some(ref source) = export.source {
                let re_export_source = source.value.to_string();
                for spec in &export.specifiers {
                    let exported_name = spec.exported.name().to_string();
                    if exported_name == original_name {
                        let local_name = spec.local.name().to_string();
                        if let Some(result) = resolve_imported_variable(
                            &re_export_source,
                            &local_name,
                            &resolved_path,
                            visited,
                        ) {
                            return Some(result);
                        }
                    }
                }
            }
        }
    }

    // Look for the variable declaration in this file
    let var_decls = collect_variable_declarations(&program);
    let imports = collect_imports(&program);

    if let Some(expr) = var_decls.get(original_name) {
        if let Expression::ObjectExpression(obj) = expr {
            let mut classifications = HashMap::new();
            extract_field_classifications_static(obj, &var_decls, &imports, &resolved_path, visited, &mut classifications);
            return Some(classifications);
        }
    }

    // Maybe it's imported from another module
    if let Some((nested_source, nested_name)) = imports.get(original_name) {
        return resolve_imported_variable(nested_source, nested_name, &resolved_path, visited);
    }

    None
}

/// Static version of extract_field_classifications that doesn't need a visitor.
fn extract_field_classifications_static<'a>(
    props_obj: &oxc_ast::ast::ObjectExpression<'a>,
    var_decls: &VarDecls<'a>,
    imports: &ImportMap,
    file_path: &Path,
    visited: &mut HashSet<PathBuf>,
    classifications: &mut HashMap<String, String>,
) {
    for prop in &props_obj.properties {
        match prop {
            ObjectPropertyKind::ObjectProperty(obj_prop) => {
                let field_name = match &obj_prop.key {
                    PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                    _ => continue,
                };
                if let Some(level) = find_compliance_call(&obj_prop.value) {
                    classifications.insert(field_name, level);
                }
            }
            ObjectPropertyKind::SpreadProperty(spread) => {
                resolve_spread_static(&spread.argument, var_decls, imports, file_path, visited, classifications);
            }
        }
    }
}

/// Static version of resolve_spread.
fn resolve_spread_static<'a>(
    expr: &Expression<'a>,
    var_decls: &VarDecls<'a>,
    imports: &ImportMap,
    file_path: &Path,
    visited: &mut HashSet<PathBuf>,
    classifications: &mut HashMap<String, String>,
) {
    if let Expression::Identifier(ident) = expr {
        let name = ident.name.as_str();

        // Check known base properties
        if let Some(base_fields) = get_known_base_classifications(name) {
            classifications.extend(base_fields);
            return;
        }

        // Check same-file variable
        if let Some(resolved) = var_decls.get(name) {
            if let Expression::ObjectExpression(obj) = resolved {
                extract_field_classifications_static(obj, var_decls, imports, file_path, visited, classifications);
                return;
            }
        }

        // Check imports — follow to source file
        if let Some((source, original)) = imports.get(name) {
            if let Some(imported_fields) =
                resolve_imported_variable(source, original, file_path, visited)
            {
                classifications.extend(imported_fields);
            }
        }
    }
}

/// AST visitor that extracts compliance metadata from defineComplianceEntity calls.
struct ComplianceVisitor<'a, 'b> {
    pub entities: Vec<EntityComplianceInfo>,
    var_decls: &'b VarDecls<'a>,
    imports: &'b ImportMap,
    file_path: &'b Path,
    visited: &'b mut HashSet<PathBuf>,
}

impl<'a, 'b> ComplianceVisitor<'a, 'b> {
    fn new(
        var_decls: &'b VarDecls<'a>,
        imports: &'b ImportMap,
        file_path: &'b Path,
        visited: &'b mut HashSet<PathBuf>,
    ) -> Self {
        Self {
            entities: Vec::new(),
            var_decls,
            imports,
            file_path,
            visited,
        }
    }
}

impl<'a, 'b> Visit<'a> for ComplianceVisitor<'a, 'b> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        let is_define = match &call.callee {
            Expression::Identifier(ident) => ident.name == "defineComplianceEntity",
            _ => false,
        };

        if is_define {
            if let Some(arg) = call.arguments.first() {
                if let Some(Expression::ObjectExpression(obj)) = arg.as_expression() {
                    let mut entity_name = String::new();
                    let mut field_classifications = HashMap::new();
                    let mut retention = None;

                    for prop in &obj.properties {
                        if let ObjectPropertyKind::ObjectProperty(obj_prop) = prop {
                            let key_name = match &obj_prop.key {
                                PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
                                _ => None,
                            };

                            match key_name.as_deref() {
                                Some("name") => {
                                    if let Expression::StringLiteral(s) = &obj_prop.value {
                                        entity_name = s.value.to_string();
                                    }
                                }
                                Some("retention") => {
                                    retention = extract_retention(&obj_prop.value);
                                }
                                Some("properties") => {
                                    self.resolve_properties_expr(
                                        &obj_prop.value,
                                        &mut field_classifications,
                                    );
                                }
                                _ => {}
                            }
                        }
                    }

                    if !entity_name.is_empty() {
                        self.entities.push(EntityComplianceInfo {
                            entity_name,
                            field_classifications,
                            retention,
                        });
                    }
                }
            }
        }

        oxc_ast_visit::walk::walk_call_expression(self, call);
    }
}

impl<'a, 'b> ComplianceVisitor<'a, 'b> {
    fn resolve_properties_expr(
        &mut self,
        expr: &Expression<'a>,
        classifications: &mut HashMap<String, String>,
    ) {
        match expr {
            Expression::ObjectExpression(props_obj) => {
                self.extract_field_classifications(props_obj, classifications);
            }
            Expression::Identifier(ident) => {
                let name = ident.name.as_str();
                if let Some(resolved) = self.var_decls.get(name) {
                    // Safety: we need to re-borrow since resolve_properties_expr takes &mut self
                    // but var_decls is an immutable ref. Clone the expression pointer.
                    let resolved = *resolved;
                    self.resolve_properties_expr(resolved, classifications);
                    return;
                }
                if let Some((source, original)) = self.imports.get(name).cloned() {
                    if let Some(imported_fields) =
                        resolve_imported_variable(&source, &original, self.file_path, self.visited)
                    {
                        classifications.extend(imported_fields);
                    }
                }
            }
            _ => {}
        }
    }

    fn extract_field_classifications(
        &mut self,
        props_obj: &oxc_ast::ast::ObjectExpression<'a>,
        classifications: &mut HashMap<String, String>,
    ) {
        for prop in &props_obj.properties {
            match prop {
                ObjectPropertyKind::ObjectProperty(obj_prop) => {
                    let field_name = match &obj_prop.key {
                        PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                        _ => continue,
                    };
                    if let Some(level) = find_compliance_call(&obj_prop.value) {
                        classifications.insert(field_name, level);
                    }
                }
                ObjectPropertyKind::SpreadProperty(spread) => {
                    self.resolve_spread(&spread.argument, classifications);
                }
            }
        }
    }

    fn resolve_spread(
        &mut self,
        expr: &Expression<'a>,
        classifications: &mut HashMap<String, String>,
    ) {
        if let Expression::Identifier(ident) = expr {
            let name = ident.name.as_str();

            if let Some(base_fields) = get_known_base_classifications(name) {
                classifications.extend(base_fields);
                return;
            }

            if let Some(resolved) = self.var_decls.get(name) {
                if let Expression::ObjectExpression(obj) = resolved {
                    // Clone needed due to borrow checker — extract_field_classifications needs &mut self
                    let obj_properties: Vec<_> = obj.properties.iter().collect();
                    for prop in obj_properties {
                        match prop {
                            ObjectPropertyKind::ObjectProperty(obj_prop) => {
                                let field_name = match &obj_prop.key {
                                    PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                                    _ => continue,
                                };
                                if let Some(level) = find_compliance_call(&obj_prop.value) {
                                    classifications.insert(field_name, level);
                                }
                            }
                            ObjectPropertyKind::SpreadProperty(spread) => {
                                self.resolve_spread(&spread.argument, classifications);
                            }
                        }
                    }
                    return;
                }
            }

            if let Some((source, original)) = self.imports.get(name).cloned() {
                if let Some(imported_fields) =
                    resolve_imported_variable(&source, &original, self.file_path, self.visited)
                {
                    classifications.extend(imported_fields);
                }
            }
        }
    }
}

fn extract_retention(expr: &Expression) -> Option<RetentionInfo> {
    let obj = match expr {
        Expression::ObjectExpression(obj) => obj,
        _ => return None,
    };

    let mut duration = None;
    let mut action = None;

    for prop in &obj.properties {
        if let ObjectPropertyKind::ObjectProperty(obj_prop) = prop {
            let key_name = match &obj_prop.key {
                PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
                _ => None,
            };

            match key_name.as_deref() {
                Some("duration") => duration = extract_duration_value(&obj_prop.value),
                Some("action") => {
                    if let Expression::StringLiteral(s) = &obj_prop.value {
                        action = Some(s.value.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    match (duration, action) {
        (Some(d), Some(a)) => Some(RetentionInfo {
            duration: d,
            action: a,
        }),
        _ => None,
    }
}

fn extract_duration_value(expr: &Expression) -> Option<String> {
    match expr {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                let method = member.property.name.as_str();
                if let Some(arg) = call.arguments.first() {
                    if let Some(Expression::NumericLiteral(num)) = arg.as_expression() {
                        let n = num.value as u64;
                        return match method {
                            "years" => Some(format!("P{}Y", n)),
                            "months" => Some(format!("P{}M", n)),
                            "days" => Some(format!("P{}D", n)),
                            _ => None,
                        };
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn find_compliance_call(expr: &Expression) -> Option<String> {
    match expr {
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                if member.property.name == "compliance" {
                    if let Some(arg) = call.arguments.first() {
                        if let Some(Expression::StringLiteral(s)) = arg.as_expression() {
                            return Some(s.value.to_string());
                        }
                    }
                }
                if let Some(level) = find_compliance_call(&member.object) {
                    return Some(level);
                }
            }
            None
        }
        _ => None,
    }
}

/// Scan all entity files in a project's persistence/entities directory.
pub fn scan_entity_compliance(project_path: &Path) -> Result<Vec<EntityComplianceInfo>> {
    let entities_dir = project_path.join("persistence").join("entities");
    if !entities_dir.exists() {
        return Ok(Vec::new());
    }

    let mut all_entities = Vec::new();

    for entry in fs::read_dir(&entities_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if !file_name.ends_with(".entity.ts") || file_name.ends_with(".d.ts") {
            continue;
        }

        if file_name == "index.ts" {
            continue;
        }

        let source_code = fs::read_to_string(&path)?;
        let entities = extract_compliance_from_source_with_path(&source_code, &path)?;
        all_entities.extend(entities);
    }

    Ok(all_entities)
}

/// Extract compliance metadata from source code, with file path context for import resolution.
fn extract_compliance_from_source_with_path(
    source_code: &str,
    file_path: &Path,
) -> Result<Vec<EntityComplianceInfo>> {
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
            "TypeScript parse errors during compliance scan: {:?}",
            errors
        );
    }

    let var_decls = collect_variable_declarations(&program);
    let imports = collect_imports(&program);
    let mut visited = HashSet::new();

    // Mark the current file as visited
    if let Ok(canonical) = fs::canonicalize(file_path) {
        visited.insert(canonical);
    }

    let mut visitor = ComplianceVisitor::new(&var_decls, &imports, file_path, &mut visited);
    visitor.visit_program(&program);

    Ok(visitor.entities)
}

/// Extract compliance metadata from source code (no file path context — import resolution uses known bases only).
#[cfg(test)]
pub fn extract_compliance_from_source(source_code: &str) -> Result<Vec<EntityComplianceInfo>> {
    extract_compliance_from_source_with_path(source_code, Path::new(""))
}

/// Scan all projects under the modules path and return aggregated compliance data.
pub fn scan_all_compliance(
    modules_path: &Path,
) -> Result<(
    HashMap<String, HashMap<String, String>>,
    HashMap<String, RetentionInfo>,
)> {
    let mut all_field_classifications: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut all_retention: HashMap<String, RetentionInfo> = HashMap::new();

    if !modules_path.exists() {
        return Ok((all_field_classifications, all_retention));
    }

    for entry in fs::read_dir(modules_path)? {
        let entry = entry?;
        let project_path = entry.path();

        if !project_path.is_dir() {
            continue;
        }

        let entities = scan_entity_compliance(&project_path)?;

        for entity in entities {
            if !entity.field_classifications.is_empty() {
                all_field_classifications
                    .insert(entity.entity_name.clone(), entity.field_classifications);
            }
            if let Some(retention) = entity.retention {
                all_retention.insert(entity.entity_name.clone(), retention);
            }
        }
    }

    Ok((all_field_classifications, all_retention))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Source-level extraction tests ----

    #[test]
    fn test_extract_basic_compliance() {
        let source = r#"
        import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
        export const User = defineComplianceEntity({
            name: 'User',
            properties: {
                id: fp.uuid().primary().compliance('none'),
                email: fp.string().compliance('pii'),
                ssn: fp.string().nullable().compliance('phi'),
            }
        });
        "#;
        let entities = extract_compliance_from_source(source).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_name, "User");
        assert_eq!(entities[0].field_classifications.get("id").unwrap(), "none");
        assert_eq!(entities[0].field_classifications.get("email").unwrap(), "pii");
        assert_eq!(entities[0].field_classifications.get("ssn").unwrap(), "phi");
    }

    #[test]
    fn test_extract_with_retention_string() {
        let source = r#"
        export const Patient = defineComplianceEntity({
            name: 'Patient',
            retention: { duration: 'P3Y', action: 'anonymize' },
            properties: { id: fp.uuid().primary().compliance('none'), name: fp.string().compliance('pii') }
        });
        "#;
        let entities = extract_compliance_from_source(source).unwrap();
        assert_eq!(entities[0].retention.as_ref().unwrap().duration, "P3Y");
    }

    #[test]
    fn test_extract_with_retention_helper() {
        let source = r#"
        export const Account = defineComplianceEntity({
            name: 'Account',
            retention: { duration: RetentionDuration.years(7), action: 'anonymize' },
            properties: { id: fp.uuid().primary().compliance('none'), token: fp.string().nullable().compliance('pci') }
        });
        "#;
        let entities = extract_compliance_from_source(source).unwrap();
        assert_eq!(entities[0].retention.as_ref().unwrap().duration, "P7Y");
    }

    #[test]
    fn test_extract_no_compliance_entity() {
        let source = r#"
        export const Config = defineEntity({ name: 'Config', properties: { key: p.string() } });
        "#;
        assert_eq!(extract_compliance_from_source(source).unwrap().len(), 0);
    }

    #[test]
    fn test_extract_multiple_entities() {
        let source = r#"
        export const A = defineComplianceEntity({ name: 'A', properties: { x: fp.string().compliance('pii') } });
        export const B = defineComplianceEntity({ name: 'B', retention: { duration: 'P90D', action: 'delete' }, properties: { y: fp.string().compliance('none') } });
        "#;
        let entities = extract_compliance_from_source(source).unwrap();
        assert_eq!(entities.len(), 2);
        assert!(entities[0].retention.is_none());
        assert_eq!(entities[1].retention.as_ref().unwrap().duration, "P90D");
    }

    #[test]
    fn test_extract_chained_methods() {
        let source = r#"
        export const Payment = defineComplianceEntity({
            name: 'Payment',
            properties: {
                id: fp.uuid().primary().onCreate(() => v4()).compliance('none'),
                cardNumber: fp.string().unique().nullable().compliance('pci'),
                amount: fp.double().compliance('none'),
                status: fp.enum(() => StatusEnum).compliance('none'),
                metadata: fp.json().nullable().compliance('none'),
            }
        });
        "#;
        let entities = extract_compliance_from_source(source).unwrap();
        assert_eq!(entities[0].field_classifications.len(), 5);
        assert_eq!(entities[0].field_classifications.get("cardNumber").unwrap(), "pci");
    }

    #[test]
    fn test_relations_have_no_compliance() {
        let source = r#"
        export const Post = defineComplianceEntity({
            name: 'Post',
            properties: { id: fp.uuid().primary().compliance('none'), title: fp.string().compliance('none'), author: fp.manyToOne(() => User) }
        });
        "#;
        let entities = extract_compliance_from_source(source).unwrap();
        assert_eq!(entities[0].field_classifications.len(), 2);
        assert!(entities[0].field_classifications.get("author").is_none());
    }

    #[test]
    fn test_retention_months_helper() {
        let source = r#"
        export const S = defineComplianceEntity({ name: 'S', retention: { duration: RetentionDuration.months(6), action: 'delete' }, properties: { id: fp.uuid().primary().compliance('none') } });
        "#;
        assert_eq!(extract_compliance_from_source(source).unwrap()[0].retention.as_ref().unwrap().duration, "P6M");
    }

    #[test]
    fn test_retention_days_helper() {
        let source = r#"
        export const T = defineComplianceEntity({ name: 'T', retention: { duration: RetentionDuration.days(30), action: 'delete' }, properties: { id: fp.uuid().primary().compliance('none') } });
        "#;
        assert_eq!(extract_compliance_from_source(source).unwrap()[0].retention.as_ref().unwrap().duration, "P30D");
    }

    #[test]
    fn test_empty_source() {
        assert_eq!(extract_compliance_from_source("").unwrap().len(), 0);
    }

    #[test]
    fn test_source_with_only_imports() {
        let source = "import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';";
        assert_eq!(extract_compliance_from_source(source).unwrap().len(), 0);
    }

    // ---- Spread resolution tests ----

    #[test]
    fn test_spread_known_base_properties() {
        let source = r#"
        export const Record = defineComplianceEntity({
            name: 'Record',
            properties: { ...sqlBaseProperties, customField: fp.string().compliance('pii') }
        });
        "#;
        let entities = extract_compliance_from_source(source).unwrap();
        assert_eq!(entities[0].field_classifications.len(), 5); // 4 base + 1 custom
        assert_eq!(entities[0].field_classifications.get("id").unwrap(), "none");
        assert_eq!(entities[0].field_classifications.get("createdAt").unwrap(), "none");
        assert_eq!(entities[0].field_classifications.get("customField").unwrap(), "pii");
    }

    #[test]
    fn test_spread_nosql_base_properties() {
        let source = r#"
        export const Doc = defineComplianceEntity({
            name: 'Doc',
            properties: { ...nosqlBaseProperties, data: fp.string().compliance('phi') }
        });
        "#;
        let entities = extract_compliance_from_source(source).unwrap();
        assert_eq!(entities[0].field_classifications.get("_id").unwrap(), "none");
        assert_eq!(entities[0].field_classifications.get("data").unwrap(), "phi");
    }

    #[test]
    fn test_spread_custom_same_file_variable() {
        let source = r#"
        const customBase = {
            orgId: fp.string().compliance('none'),
            region: fp.string().compliance('none'),
        };
        export const Thing = defineComplianceEntity({
            name: 'Thing',
            properties: { ...customBase, secret: fp.string().compliance('pci') }
        });
        "#;
        let entities = extract_compliance_from_source(source).unwrap();
        assert_eq!(entities[0].field_classifications.get("orgId").unwrap(), "none");
        assert_eq!(entities[0].field_classifications.get("secret").unwrap(), "pci");
    }

    #[test]
    fn test_properties_as_variable_reference() {
        let source = r#"
        const userProps = { id: fp.uuid().primary().compliance('none'), email: fp.string().compliance('pii') };
        export const User = defineComplianceEntity({ name: 'User', properties: userProps });
        "#;
        let entities = extract_compliance_from_source(source).unwrap();
        assert_eq!(entities[0].field_classifications.get("email").unwrap(), "pii");
    }

    #[test]
    fn test_spread_with_base_and_custom_and_retention() {
        let source = r#"
        export const Patient = defineComplianceEntity({
            name: 'Patient',
            retention: { duration: RetentionDuration.years(3), action: 'anonymize' },
            properties: {
                ...sqlBaseProperties,
                name: fp.string().compliance('pii'),
                ssn: fp.string().nullable().compliance('phi'),
                visitCount: fp.integer().compliance('none'),
                org: fp.manyToOne(() => Organization),
            }
        });
        "#;
        let entities = extract_compliance_from_source(source).unwrap();
        assert_eq!(entities[0].field_classifications.len(), 7); // 4 base + 3 scalar (relation excluded)
        assert_eq!(entities[0].field_classifications.get("name").unwrap(), "pii");
        assert_eq!(entities[0].retention.as_ref().unwrap().duration, "P3Y");
    }

    // ---- Cross-file import resolution tests ----

    #[test]
    fn test_spread_imported_from_relative_file() {
        use std::fs::{create_dir_all, write};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let entities_dir = temp_dir.path().join("persistence").join("entities");
        let shared_dir = temp_dir.path().join("shared");
        create_dir_all(&entities_dir).unwrap();
        create_dir_all(&shared_dir).unwrap();

        // Shared base properties file
        write(
            shared_dir.join("base-props.ts"),
            r#"
            export const tenantBaseProperties = {
                tenantId: fp.string().compliance('none'),
                region: fp.string().compliance('none'),
                orgName: fp.string().compliance('pii'),
            };
            "#,
        )
        .unwrap();

        // Entity that imports and spreads it
        write(
            entities_dir.join("record.entity.ts"),
            r#"
            import { tenantBaseProperties } from '../../shared/base-props';
            export const Record = defineComplianceEntity({
                name: 'Record',
                properties: {
                    ...sqlBaseProperties,
                    ...tenantBaseProperties,
                    data: fp.string().compliance('phi'),
                }
            });
            "#,
        )
        .unwrap();

        let entities = scan_entity_compliance(temp_dir.path()).unwrap();
        assert_eq!(entities.len(), 1);
        let e = &entities[0];

        // 4 from sqlBaseProperties + 3 from tenantBaseProperties + 1 custom
        assert_eq!(e.field_classifications.len(), 8);
        assert_eq!(e.field_classifications.get("tenantId").unwrap(), "none");
        assert_eq!(e.field_classifications.get("orgName").unwrap(), "pii");
        assert_eq!(e.field_classifications.get("data").unwrap(), "phi");
        assert_eq!(e.field_classifications.get("id").unwrap(), "none"); // from sqlBaseProperties
    }

    #[test]
    fn test_spread_imported_via_index_reexport() {
        use std::fs::{create_dir_all, write};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let entities_dir = temp_dir.path().join("persistence").join("entities");
        let core_dir = temp_dir.path().join("core");
        create_dir_all(&entities_dir).unwrap();
        create_dir_all(&core_dir).unwrap();

        // The actual file with the properties
        write(
            core_dir.join("custom-base.ts"),
            r#"
            export const customBase = {
                tenantId: fp.string().compliance('none'),
                isActive: fp.boolean().compliance('none'),
            };
            "#,
        )
        .unwrap();

        // Re-export via index.ts
        write(
            core_dir.join("index.ts"),
            r#"export * from './custom-base';"#,
        )
        .unwrap();

        // Entity imports from the index
        write(
            entities_dir.join("item.entity.ts"),
            r#"
            import { customBase } from '../../core';
            export const Item = defineComplianceEntity({
                name: 'Item',
                properties: {
                    ...customBase,
                    value: fp.string().compliance('pci'),
                }
            });
            "#,
        )
        .unwrap();

        let entities = scan_entity_compliance(temp_dir.path()).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].field_classifications.get("tenantId").unwrap(), "none");
        assert_eq!(entities[0].field_classifications.get("isActive").unwrap(), "none");
        assert_eq!(entities[0].field_classifications.get("value").unwrap(), "pci");
    }

    // ---- File/project scanning tests ----

    #[test]
    fn test_scan_entity_compliance_from_files() {
        use std::fs::{create_dir_all, write};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let entities_dir = temp_dir.path().join("persistence").join("entities");
        create_dir_all(&entities_dir).unwrap();

        write(
            entities_dir.join("user.entity.ts"),
            "export const User = defineComplianceEntity({ name: 'User', properties: { id: fp.uuid().primary().compliance('none'), email: fp.string().compliance('pii') } });",
        ).unwrap();

        write(
            entities_dir.join("payment.entity.ts"),
            "export const Payment = defineComplianceEntity({ name: 'Payment', retention: { duration: 'P5Y', action: 'anonymize' }, properties: { id: fp.uuid().primary().compliance('none'), cardNumber: fp.string().nullable().compliance('pci') } });",
        ).unwrap();

        write(entities_dir.join("index.ts"), "export * from './user.entity';").unwrap();
        write(entities_dir.join("helpers.ts"), "export function x() {}").unwrap();

        let entities = scan_entity_compliance(temp_dir.path()).unwrap();
        assert_eq!(entities.len(), 2);

        let user = entities.iter().find(|e| e.entity_name == "User").unwrap();
        assert_eq!(user.field_classifications.get("email").unwrap(), "pii");

        let payment = entities.iter().find(|e| e.entity_name == "Payment").unwrap();
        assert_eq!(payment.retention.as_ref().unwrap().duration, "P5Y");
    }

    #[test]
    fn test_scan_entity_compliance_no_entities_dir() {
        use tempfile::TempDir;
        assert_eq!(scan_entity_compliance(TempDir::new().unwrap().path()).unwrap().len(), 0);
    }

    #[test]
    fn test_scan_all_compliance_multi_project() {
        use std::fs::{create_dir_all, write};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let modules_path = temp_dir.path();

        let a_entities = modules_path.join("svc-a").join("persistence").join("entities");
        create_dir_all(&a_entities).unwrap();
        write(
            a_entities.join("patient.entity.ts"),
            r#"
            export const Patient = defineComplianceEntity({
                name: 'Patient',
                retention: { duration: RetentionDuration.years(3), action: 'anonymize' },
                properties: { ...sqlBaseProperties, name: fp.string().compliance('pii'), ssn: fp.string().nullable().compliance('phi') }
            });
            "#,
        ).unwrap();

        let b_entities = modules_path.join("svc-b").join("persistence").join("entities");
        create_dir_all(&b_entities).unwrap();
        write(
            b_entities.join("order.entity.ts"),
            "export const Order = defineComplianceEntity({ name: 'Order', properties: { id: fp.uuid().primary().compliance('none'), total: fp.double().compliance('none') } });",
        ).unwrap();

        create_dir_all(modules_path.join("core")).unwrap();

        let (fields, retention) = scan_all_compliance(modules_path).unwrap();

        let patient = fields.get("Patient").unwrap();
        assert_eq!(patient.get("name").unwrap(), "pii");
        assert_eq!(patient.get("id").unwrap(), "none"); // from spread
        assert!(fields.contains_key("Order"));
        assert_eq!(retention.get("Patient").unwrap().duration, "P3Y");
        assert!(!retention.contains_key("Order"));
    }

    #[test]
    fn test_scan_all_compliance_empty_modules() {
        use tempfile::TempDir;
        let (f, r) = scan_all_compliance(TempDir::new().unwrap().path()).unwrap();
        assert!(f.is_empty() && r.is_empty());
    }

    #[test]
    fn test_scan_all_compliance_nonexistent_path() {
        let (f, r) = scan_all_compliance(Path::new("/nonexistent/path")).unwrap();
        assert!(f.is_empty() && r.is_empty());
    }
}
