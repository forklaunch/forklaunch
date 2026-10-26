//! Native object constraints resolved before member codecs consume attributes.
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::{BTreeMap, BTreeSet};
use syn::{DeriveInput, LitInt, LitStr, Meta, Token, parse_quote, punctuated::Punctuated};
#[derive(Default)]
pub(super) struct Options {
    pub min: Option<LitInt>,
    pub max: Option<LitInt>,
}
impl Options {
    pub fn set(&mut self, meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<()> {
        let target = if meta.path.is_ident("min_properties") {
            &mut self.min
        } else {
            &mut self.max
        };
        if target.is_some() {
            return Err(meta.error("duplicate property bound"));
        }
        let value: LitInt = meta.value()?.parse()?;
        if value.base10_parse::<u32>()? > 4096 {
            return Err(meta.error("property bounds must be between 0 and 4096"));
        }
        *target = Some(value);
        Ok(())
    }
    pub fn expand(
        &self,
        input: &mut DeriveInput,
        rename: Option<&LitStr>,
        supported: bool,
        path: &syn::Path,
    ) -> syn::Result<TokenStream> {
        let dependencies = extract_dependencies(input)?;
        if self.min.is_none() && self.max.is_none() && dependencies.is_empty() {
            return Ok(quote!());
        }
        if !supported {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "object constraints require a named struct without raw serde/schemars overrides",
            ));
        }
        let names = wire_names(input, rename)?;
        if let (Some(min), Some(max)) = (&self.min, &self.max)
            && min.base10_parse::<u32>()? > max.base10_parse::<u32>()?
        {
            return Err(syn::Error::new_spanned(
                max,
                "min_properties exceeds max_properties",
            ));
        }
        let mut by_trigger: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (member, trigger) in dependencies {
            let Some(trigger_wire) = names.get(&trigger.value()) else {
                return Err(syn::Error::new_spanned(
                    trigger,
                    "required_with must reference a Rust member name",
                ));
            };
            if member == trigger.value() {
                return Err(syn::Error::new_spanned(
                    trigger,
                    "required_with cannot reference itself",
                ));
            }
            by_trigger
                .entry(trigger_wire.clone())
                .or_default()
                .insert(names[&member].clone());
        }
        let min = self
            .min
            .as_ref()
            .map(|value| quote!(schema.insert("minProperties".to_owned(), #path::JsonValue::from(#value));));
        let max = self
            .max
            .as_ref()
            .map(|value| quote!(schema.insert("maxProperties".to_owned(), #path::JsonValue::from(#value));));
        let entries = by_trigger.iter().map(|(trigger,members)|quote! {
            dependencies.insert(#trigger.to_owned(), #path::JsonValue::Array(vec![#(#path::JsonValue::String(#members.to_owned())),*]));
        });
        let dependencies = if by_trigger.is_empty() {
            quote!()
        } else {
            quote! {
                let mut dependencies = ::std::collections::BTreeMap::new();
                #(#entries)*
                schema.insert("dependencies".to_owned(), #path::JsonValue::Object(dependencies.into_iter().collect()));
            }
        };
        Ok(quote! {
            let mut schema = schema;
            #min #max #dependencies
        })
    }
}
fn extract_dependencies(input: &mut DeriveInput) -> syn::Result<Vec<(String, LitStr)>> {
    let fields: Vec<_> = match &mut input.data {
        syn::Data::Struct(data) => data.fields.iter_mut().collect(),
        syn::Data::Enum(data) => data
            .variants
            .iter_mut()
            .flat_map(|variant| variant.fields.iter_mut())
            .collect(),
        syn::Data::Union(_) => Vec::new(),
    };
    let mut result = Vec::new();
    for field in fields {
        let mut seen = false;
        let mut attributes = Vec::new();
        for attribute in &field.attrs {
            if !attribute.path().is_ident("field") {
                attributes.push(attribute.clone());
                continue;
            }
            let options =
                attribute.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
            let mut remaining = Vec::new();
            for option in options {
                if option.path().is_ident("required_with") {
                    if seen {
                        return Err(syn::Error::new_spanned(option, "duplicate required_with"));
                    }
                    seen = true;
                    let value = option.require_name_value()?;
                    let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(name),
                        ..
                    }) = &value.value
                    else {
                        return Err(syn::Error::new_spanned(
                            value,
                            "required_with requires a Rust member name string",
                        ));
                    };
                    let member = field.ident.as_ref().ok_or_else(|| {
                        syn::Error::new_spanned(&*field, "required_with requires a named member")
                    })?;
                    result.push((
                        member.to_string().trim_start_matches("r#").to_owned(),
                        name.clone(),
                    ));
                } else {
                    remaining.push(option);
                }
            }
            if !remaining.is_empty() {
                attributes.push(parse_quote!(#[field(#(#remaining),*)]));
            }
        }
        field.attrs = attributes;
    }
    Ok(result)
}
fn wire_names(
    input: &DeriveInput,
    rename: Option<&LitStr>,
) -> syn::Result<BTreeMap<String, String>> {
    let syn::Data::Struct(data) = &input.data else {
        unreachable!()
    };
    let mut names = BTreeMap::new();
    let mut unique = BTreeSet::new();
    for field in &data.fields {
        let member = field.ident.as_ref().expect("named struct");
        let rust = member.to_string().trim_start_matches("r#").to_owned();
        let mut wire = rename_field(&rust, rename)?;
        for attribute in field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("field"))
        {
            for option in
                attribute.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?
            {
                if option.path().is_ident("rename") {
                    let value = option.require_name_value()?;
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(name),
                        ..
                    }) = &value.value
                    {
                        wire = name.value();
                    }
                }
            }
        }
        if !unique.insert(wire.clone()) {
            return Err(syn::Error::new_spanned(
                member,
                "duplicate serialized member name in object constraints",
            ));
        }
        names.insert(rust, wire);
    }
    Ok(names)
}
fn rename_field(name: &str, rule: Option<&LitStr>) -> syn::Result<String> {
    let Some(rule) = rule else {
        return Ok(name.to_owned());
    };
    let pascal = || {
        name.split('_')
            .filter(|s| !s.is_empty())
            .map(|word| {
                let mut chars = word.chars();
                chars.next().map_or_else(String::new, |first| {
                    ::std::iter::once(first.to_ascii_uppercase())
                        .chain(chars)
                        .collect::<String>()
                })
            })
            .collect::<String>()
    };
    Ok(match rule.value().as_str() {
        "lowercase" | "snake_case" => name.to_owned(),
        "UPPERCASE" | "SCREAMING_SNAKE_CASE" => name.to_ascii_uppercase(),
        "PascalCase" => pascal(),
        "camelCase" => {
            let value = pascal();
            let mut chars = value.chars();
            chars.next().map_or_else(String::new, |first| {
                ::std::iter::once(first.to_ascii_lowercase())
                    .chain(chars)
                    .collect()
            })
        }
        "kebab-case" => name.replace('_', "-"),
        "SCREAMING-KEBAB-CASE" => name.to_ascii_uppercase().replace('_', "-"),
        _ => {
            return Err(syn::Error::new_spanned(
                rule,
                "unsupported rename_all for object constraints",
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_dependencies_report_the_actual_problem() {
        for (source, diagnostic) in [
            (
                "struct Input { #[field(required_with=\"missing\")] value: String }",
                "reference a Rust member",
            ),
            (
                "struct Input { #[field(required_with=\"value\")] value: String }",
                "reference itself",
            ),
            (
                "struct Input { #[field(required_with=\"b\", required_with=\"b\")] a: String, b: String }",
                "duplicate required_with",
            ),
            (
                "struct Input { #[field(rename=\"same\", required_with=\"b\")] a: String, #[field(rename=\"same\")] b: String }",
                "duplicate serialized",
            ),
        ] {
            let mut input = syn::parse_str(source).unwrap();
            let error = Options::default()
                .expand(&mut input, None, true, &syn::parse_quote!(crate))
                .unwrap_err();
            assert!(error.to_string().contains(diagnostic), "{error}");
        }
    }
}
