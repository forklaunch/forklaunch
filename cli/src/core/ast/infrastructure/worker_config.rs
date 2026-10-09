use std::{collections::HashMap, fs, path::Path};

use anyhow::Result;
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrowFunctionExpression, Expression, ObjectExpression, ObjectProperty, PropertyKey,
    VariableDeclaration,
};
use oxc_ast_visit::Visit;
use oxc_parser::{Parser, ParserReturn};
use oxc_span::SourceType;

use crate::core::rendered_template::RenderedTemplatesCache;

#[derive(Debug, Clone, PartialEq)]
pub struct WorkerConfig {
    pub concurrency: Option<i32>,
    pub timeout: Option<i32>,
    pub max_retries: Option<i32>,
    pub queue: Option<String>,
    pub priority: Option<String>,
    pub dead_letter_queue: Option<bool>,
}

pub struct WorkerConfigVisitor {
    pub config: Option<WorkerConfig>,
    in_worker_options: bool,
    in_factory: bool,
    depth: usize,
}

impl WorkerConfigVisitor {
    pub fn new() -> Self {
        Self {
            config: None,
            in_worker_options: false,
            in_factory: false,
            depth: 0,
        }
    }

    fn extract_numeric_value(expr: &Expression) -> Option<i32> {
        match expr {
            Expression::NumericLiteral(lit) => {
                // Try to parse as i32
                let value = lit.value;
                if value >= i32::MIN as f64 && value <= i32::MAX as f64 {
                    Some(value as i32)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn extract_string_value(expr: &Expression) -> Option<String> {
        match expr {
            Expression::StringLiteral(lit) => Some(lit.value.to_string()),
            _ => None,
        }
    }

    fn extract_boolean_value(expr: &Expression) -> Option<bool> {
        match expr {
            Expression::BooleanLiteral(lit) => Some(lit.value),
            _ => None,
        }
    }
}

impl<'a> Visit<'a> for WorkerConfigVisitor {
    fn visit_variable_declaration(&mut self, decl: &VariableDeclaration<'a>) {
        // Look for: const runtimeDependencies = ...
        for declarator in &decl.declarations {
            if let oxc_ast::ast::BindingPatternKind::BindingIdentifier(ident) = &declarator.id.kind
            {
                if ident.name == "runtimeDependencies" {
                    // Visit the initializer to find WorkerOptions
                    if let Some(init) = &declarator.init {
                        self.visit_expression(init);
                    }
                }
            }
        }

        oxc_ast_visit::walk::walk_variable_declaration(self, decl);
    }

    fn visit_object_expression(&mut self, expr: &ObjectExpression<'a>) {
        if self.in_worker_options && self.in_factory {
            self.depth += 1;
        }

        oxc_ast_visit::walk::walk_object_expression(self, expr);

        if self.in_worker_options && self.in_factory {
            self.depth -= 1;
        }
    }

    fn visit_object_property(&mut self, prop: &ObjectProperty<'a>) {
        // Look for WorkerOptions in runtimeDependencies
        if let PropertyKey::StaticIdentifier(ident) = &prop.key {
            let prop_name = ident.name.to_string();

            // First priority: extract config if we're in the right context
            if self.in_factory && self.depth == 1 {
                // We're at the top level of the factory return value or value object
                // Extract configuration properties
                let mut config = self.config.take().unwrap_or_else(|| WorkerConfig {
                    concurrency: None,
                    timeout: None,
                    max_retries: None,
                    queue: None,
                    priority: None,
                    dead_letter_queue: None,
                });

                match prop_name.as_str() {
                    "concurrency" => {
                        if let Some(num) = Self::extract_numeric_value(&prop.value) {
                            config.concurrency = Some(num);
                        }
                    }
                    "timeout" | "jobTimeout" => {
                        if let Some(num) = Self::extract_numeric_value(&prop.value) {
                            config.timeout = Some(num);
                        }
                    }
                    "retries" | "maxRetries" => {
                        if let Some(num) = Self::extract_numeric_value(&prop.value) {
                            config.max_retries = Some(num);
                        }
                    }
                    "priority" => {
                        if let Some(str_val) = Self::extract_string_value(&prop.value) {
                            config.priority = Some(str_val);
                        }
                    }
                    "deadLetterQueue" | "deadLetter" => {
                        if let Some(bool_val) = Self::extract_boolean_value(&prop.value) {
                            config.dead_letter_queue = Some(bool_val);
                        }
                    }
                    "queueOptions" => {
                        // Nested object, might contain queue name
                        if let Expression::ObjectExpression(queue_obj) = &prop.value {
                            for queue_prop in &queue_obj.properties {
                                if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(
                                    queue_prop,
                                ) = queue_prop
                                {
                                    if let PropertyKey::StaticIdentifier(queue_ident) =
                                        &queue_prop.key
                                    {
                                        if queue_ident.name == "name" {
                                            if let Some(queue_name) =
                                                Self::extract_string_value(&queue_prop.value)
                                            {
                                                config.queue = Some(queue_name);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }

                self.config = Some(config);
            } else if self.in_worker_options {
                // We're inside WorkerOptions, look for factory or value
                if prop_name == "factory" {
                    self.in_factory = true;
                    self.depth = 0;

                    self.visit_expression(&prop.value);

                    self.in_factory = false;
                    self.depth = 0;
                } else if prop_name == "value" {
                    // Direct value object
                    self.in_factory = true; // Reuse the flag for value objects
                    self.depth = 0;

                    self.visit_expression(&prop.value);

                    self.in_factory = false;
                    self.depth = 0;
                }
            } else {
                // Check if this is WorkerOptions or a variant (BullMqWorkerOptions, KafkaWorkerOptions, etc.)
                if prop_name == "WorkerOptions" || prop_name.ends_with("WorkerOptions") {
                    self.in_worker_options = true;
                    self.depth = 0;

                    // Visit the value to find factory or value
                    self.visit_expression(&prop.value);

                    self.in_worker_options = false;
                    self.depth = 0;
                }
            }
        }

        oxc_ast_visit::walk::walk_object_property(self, prop);
    }

    fn visit_arrow_function_expression(&mut self, expr: &ArrowFunctionExpression<'a>) {
        if self.in_factory {
            // The factory is an arrow function, visit its body to find the return object
            // For block statements, look for return statements
            // For expression bodies, the visitor will naturally visit the expression
            if !expr.body.statements.is_empty() {
                // Block statement body - look for return statement
                for stmt in &expr.body.statements {
                    if let oxc_ast::ast::Statement::ReturnStatement(ret) = stmt {
                        if let Some(return_expr) = &ret.argument {
                            self.visit_expression(return_expr);
                        }
                    }
                }
            }
            // For expression bodies (when statements is empty), the walk will handle it
        }
        // Always walk the arrow function to handle expression bodies
        oxc_ast_visit::walk::walk_arrow_function_expression(self, expr);
    }
}

pub fn extract_worker_config_from_source(source: &str) -> Result<Option<WorkerConfig>> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path("registrations.ts").unwrap_or_default();
    let ParserReturn {
        program, errors, ..
    } = Parser::new(&allocator, source, source_type).parse();

    if !errors.is_empty() {
        return Ok(None);
    }

    let mut visitor = WorkerConfigVisitor::new();
    visitor.visit_program(&program);

    Ok(visitor.config)
}

pub fn find_all_worker_configs(
    modules_path: &Path,
    rendered_templates_cache: &RenderedTemplatesCache,
) -> Result<HashMap<String, WorkerConfig>> {
    let mut configs = HashMap::new();

    if !modules_path.exists() {
        return Ok(configs);
    }

    for entry in fs::read_dir(modules_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let registrations_path = path.join("registrations.ts");
            if registrations_path.exists() {
                let project_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Try to get from cache first, then fall back to reading file
                let content =
                    if let Ok(Some(cached)) = rendered_templates_cache.get(&registrations_path) {
                        cached.content
                    } else {
                        fs::read_to_string(&registrations_path)?
                    };

                if let Some(config) = extract_worker_config_from_source(&content)? {
                    configs.insert(project_name, config);
                }
            }
        }
    }

    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_worker_config_from_factory() {
        let source = r#"
const runtimeDependencies = environmentConfig.chain({
  WorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: BullMqWorkerSchemas({
      validator: SchemaValidator(),
    }),
    factory: ({ REDIS_URL }) => ({
      backoffType: "exponential" as const,
      queueOptions: {
        connection: {
          url: REDIS_URL,
        },
        name: "my-queue"
      },
      retries: 3,
      interval: 5000,
      jobTimeout: 1800000,
      concurrency: 5,
      priority: "high",
      deadLetterQueue: true
    }),
  },
});
"#;

        let config = extract_worker_config_from_source(source).unwrap();
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.max_retries, Some(3));
        assert_eq!(config.timeout, Some(1800000));
        assert_eq!(config.concurrency, Some(5));
        assert_eq!(config.priority, Some("high".to_string()));
        assert_eq!(config.dead_letter_queue, Some(true));
        assert_eq!(config.queue, Some("my-queue".to_string()));
    }

    #[test]
    fn test_extract_worker_config_from_value() {
        let source = r#"
const runtimeDependencies = environmentConfig.chain({
  DatabaseWorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: DatabaseWorkerOptionsSchema,
    value: {
      retries: 3,
      interval: 5000,
      timeout: 30000
    }
  },
});
"#;

        let config = extract_worker_config_from_source(source).unwrap();
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.max_retries, Some(3));
        assert_eq!(config.timeout, Some(30000));
    }

    #[test]
    fn test_extract_worker_config_partial() {
        let source = r#"
const runtimeDependencies = environmentConfig.chain({
  WorkerOptions: {
    factory: () => ({
      retries: 5
    })
  },
});
"#;

        let config = extract_worker_config_from_source(source).unwrap();
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.max_retries, Some(5));
        assert_eq!(config.timeout, None);
        assert_eq!(config.concurrency, None);
    }
}
