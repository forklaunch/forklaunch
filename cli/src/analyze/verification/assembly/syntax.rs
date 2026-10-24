use super::{Assembly, RawRoute, Value, Verifier};
use anyhow::{Result, ensure};
use std::{collections::BTreeMap, path::Path};
use syn::Expr;
fn last(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_default()
}
impl Verifier {
    pub(super) fn expression(
        &mut self,
        expression: &Expr,
        variables: &mut BTreeMap<String, Value>,
        source: &Path,
    ) -> Result<Value> {
        match expression {
            Expr::Paren(value) => self.expression(&value.expr, variables, source),
            Expr::Try(value) => self.expression(&value.expr, variables, source),
            Expr::Path(value) if value.path.segments.len() == 1 && value.qself.is_none() => {
                Ok(variables
                    .get(&last(&value.path))
                    .cloned()
                    .unwrap_or(Value::Unknown))
            }
            Expr::Struct(value) if last(&value.path) == "Bindings" => {
                ensure!(
                    !value
                        .fields
                        .iter()
                        .any(|field| touches_tracked(&field.expr, variables)),
                    "Binding constructor touches tracked router state"
                );
                ensure!(
                    value.rest.is_none(),
                    "Binding update expressions need explicit resolution"
                );
                let mut fields: Vec<_> = value
                    .fields
                    .iter()
                    .map(|field| match &field.member {
                        syn::Member::Named(name) => name.to_string(),
                        syn::Member::Unnamed(index) => index.index.to_string(),
                    })
                    .collect();
                fields.sort();
                let group = self.groups.get(&fields).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Binding fields do not identify a complete exported operation set"
                    )
                })?;
                let resolved = self.resolver.binding(source, &value.path)?;
                ensure!(
                    self.group_sources.get(&resolved) == Some(group),
                    "Binding type resolves to a different compiled declaration"
                );
                Ok(Value::Bindings(group.clone()))
            }
            Expr::Call(call) => {
                let Expr::Path(path) = call.func.as_ref() else {
                    return Ok(Value::Unknown);
                };
                let name = last(&path.path);
                let owner = path
                    .path
                    .segments
                    .iter()
                    .rev()
                    .nth(1)
                    .map(|segment| segment.ident.to_string());
                if name == "new" && matches!(owner.as_deref(), Some("Router" | "ContractRouter")) {
                    ensure!(
                        self.resolver.framework_type(
                            source,
                            &path.path,
                            owner.as_deref().unwrap()
                        )?,
                        "Unresolved router constructor owner"
                    );
                    return Ok(Value::Router(Assembly {
                        finished: owner.as_deref() == Some("Router"),
                        ..Assembly::default()
                    }));
                }
                if name == "new" && owner.as_deref() == Some("NativeApplication") {
                    ensure!(
                        !call
                            .args
                            .iter()
                            .skip(1)
                            .any(|argument| touches_tracked(argument, variables)),
                        "Application metadata touches tracked assembly state"
                    );
                    ensure!(
                        self.resolver
                            .framework_type(source, &path.path, "NativeApplication")?,
                        "Unresolved application constructor owner"
                    );
                    let value = self.expression(
                        call.args
                            .first()
                            .ok_or_else(|| anyhow::anyhow!("Application router missing"))?,
                        variables,
                        source,
                    )?;
                    let Value::Router(mut router) = value else {
                        anyhow::bail!("Unresolved managed application router")
                    };
                    router.application = true;
                    return Ok(Value::Router(router));
                }
                if name == "with_openapi" {
                    ensure!(
                        !call
                            .args
                            .iter()
                            .skip(1)
                            .any(|argument| touches_tracked(argument, variables)),
                        "Docs metadata touches tracked assembly state"
                    );
                    ensure!(
                        self.resolver
                            .routing_method(source, &path.path, "with_openapi")?,
                        "Unresolved framework docs adapter"
                    );
                    return self.expression(
                        call.args
                            .first()
                            .ok_or_else(|| anyhow::anyhow!("Docs router missing"))?,
                        variables,
                        source,
                    );
                }
                if name == "Ok" && path.path.segments.len() == 1 {
                    ensure!(
                        !self.functions.contains_key("Ok") && self.resolver.standard_ok(source)?,
                        "Shadowed Result constructor"
                    );
                    return self.expression(
                        call.args
                            .first()
                            .ok_or_else(|| anyhow::anyhow!("Empty Result constructor"))?,
                        variables,
                        source,
                    );
                }
                if self.functions.contains_key(&name) {
                    let (resolved, kind) = self.resolver.binding(source, &path.path)?;
                    ensure!(
                        kind == "$function",
                        "Unresolved router assembly function path"
                    );
                    let arguments = call
                        .args
                        .iter()
                        .map(|argument| self.expression(argument, variables, source))
                        .collect::<Result<Vec<_>>>()?;
                    self.function(&name, Some(&resolved), arguments)
                } else {
                    ensure!(
                        !call
                            .args
                            .iter()
                            .any(|argument| touches_tracked(argument, variables)),
                        "Unknown call receives tracked router or bindings"
                    );
                    Ok(Value::Unknown)
                }
            }
            Expr::MethodCall(call) => {
                let value = self.expression(&call.receiver, variables, source)?;
                let method = call.method.to_string();
                match (value, method.as_str()) {
                    (Value::Bindings(group), "mount") => {
                        let argument = call
                            .args
                            .first()
                            .ok_or_else(|| anyhow::anyhow!("Binding router missing"))?;
                        let Value::Router(mut router) =
                            self.expression(argument, variables, source)?
                        else {
                            anyhow::bail!("Unresolved binding mount router")
                        };
                        ensure!(
                            router.groups.insert(group),
                            "Static operation set mounted more than once"
                        );
                        router.finished = false;
                        Ok(Value::Router(router))
                    }
                    (Value::Router(mut router), "finish") => {
                        router.finished = true;
                        Ok(Value::Router(router))
                    }
                    (Value::Router(mut router), "merge") => {
                        let argument = call
                            .args
                            .first()
                            .ok_or_else(|| anyhow::anyhow!("Merged router missing"))?;
                        let Value::Router(other) = self.expression(argument, variables, source)?
                        else {
                            anyhow::bail!("Unresolved merged router")
                        };
                        ensure!(
                            other.finished,
                            "Merged managed router has no finish boundary"
                        );
                        for group in other.groups {
                            ensure!(
                                router.groups.insert(group),
                                "Static operation set mounted more than once"
                            );
                        }
                        router.raw.extend(other.raw);
                        Ok(Value::Router(router))
                    }
                    (Value::Router(mut router), "route") => {
                        let Some(Expr::Lit(path)) = call.args.first() else {
                            anyhow::bail!("Dynamic unmanaged route path")
                        };
                        let syn::Lit::Str(path) = &path.lit else {
                            anyhow::bail!("Unmanaged route path must be a literal")
                        };
                        let Some(Expr::Call(handler)) = call.args.iter().nth(1) else {
                            anyhow::bail!("Unmanaged route methods need explicit resolution")
                        };
                        let Expr::Path(handler) = handler.func.as_ref() else {
                            anyhow::bail!("Unmanaged route method needs explicit resolution")
                        };
                        ensure!(
                            self.resolver.routing_method(
                                source,
                                &handler.path,
                                &last(&handler.path)
                            )?,
                            "Unmanaged method constructor owner is unresolved"
                        );
                        let method = last(&handler.path).to_ascii_uppercase();
                        ensure!(
                            matches!(
                                method.as_str(),
                                "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
                            ),
                            "Unsupported unmanaged route method"
                        );
                        router.raw.push(RawRoute {
                            method,
                            path: path.value(),
                            source: source.into(),
                        });
                        Ok(Value::Router(router))
                    }
                    (value, "expect" | "unwrap") => {
                        ensure!(
                            call.args
                                .iter()
                                .all(|argument| matches!(argument, Expr::Lit(_))),
                            "Nonliteral unwrap/expect arguments need explicit effect resolution"
                        );
                        Ok(value)
                    }
                    // Neither changes which routes are assembled.
                    (Value::Router(router), "with_state" | "with_cors_origins") => {
                        ensure!(
                            !call
                                .args
                                .iter()
                                .any(|argument| touches_tracked(argument, variables)),
                            "State constructor touches tracked assembly state"
                        );
                        Ok(Value::Router(router))
                    }
                    (Value::Router(_), _) => {
                        anyhow::bail!("Unsupported router assembly method {method}")
                    }
                    _ => {
                        ensure!(
                            !call
                                .args
                                .iter()
                                .any(|argument| touches_tracked(argument, variables)),
                            "Unknown method receives tracked assembly state"
                        );
                        Ok(Value::Unknown)
                    }
                }
            }
            Expr::If(_)
            | Expr::Match(_)
            | Expr::Loop(_)
            | Expr::While(_)
            | Expr::ForLoop(_)
            | Expr::Assign(_)
            | Expr::Unsafe(_)
            | Expr::Macro(_) => {
                anyhow::bail!("Unsupported control flow or mutation in router assembly")
            }
            // Unknown expressions cannot borrow or consume tracked assembly state.
            _ => {
                ensure!(
                    !touches_tracked(expression, variables),
                    "Unsupported expression touches tracked assembly state"
                );
                Ok(Value::Unknown)
            }
        }
    }
}

fn touches_tracked(expression: &Expr, variables: &BTreeMap<String, Value>) -> bool {
    struct Find<'a> {
        variables: &'a BTreeMap<String, Value>,
        found: bool,
    }
    impl<'ast> syn::visit::Visit<'ast> for Find<'_> {
        fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
            if path.path.segments.len() == 1
                && self
                    .variables
                    .get(&last(&path.path))
                    .is_some_and(|value| !matches!(value, Value::Unknown))
            {
                self.found = true;
            }
            syn::visit::visit_expr_path(self, path);
        }
    }
    let mut find = Find {
        variables,
        found: false,
    };
    syn::visit::Visit::visit_expr(&mut find, expression);
    find.found
}
