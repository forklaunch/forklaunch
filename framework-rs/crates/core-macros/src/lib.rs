#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, LitStr, Path, parse_macro_input};

#[proc_macro_derive(ComplianceMetadata, attributes(compliance))]
pub fn derive_compliance_metadata(input: TokenStream) -> TokenStream {
    expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn assign<T>(
    slot: &mut Option<T>,
    value: T,
    meta: &syn::meta::ParseNestedMeta<'_>,
) -> syn::Result<()> {
    if slot.replace(value).is_some() {
        return Err(meta.error("duplicate compliance option"));
    }
    Ok(())
}

fn checked_name(value: LitStr) -> syn::Result<LitStr> {
    if value.value().trim().is_empty() || value.value().chars().any(char::is_control) {
        return Err(syn::Error::new(
            value.span(),
            "expected a nonempty compliance name without control characters",
        ));
    }
    Ok(value)
}

#[allow(clippy::too_many_lines)] // Parse correlated classification and persisted-binding declarations together.
fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let mut persisted = None;
    let mut entity = None;
    let mut crate_path = None;
    for attr in &input.attrs {
        if attr.path().is_ident("compliance") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("entity") {
                    let value = checked_name(meta.value()?.parse()?)?;
                    assign(&mut entity, value, &meta)
                } else if meta.path.is_ident("crate_path") {
                    let value: LitStr = meta.value()?.parse()?;
                    assign(&mut crate_path, value.parse::<Path>()?, &meta)
                } else if meta.path.is_ident("persisted") {
                    assign(&mut persisted, true, &meta)
                } else {
                    Err(meta.error("expected entity, crate_path, or persisted on a struct"))
                }
            })?;
        }
    }
    let name = input.ident;
    let entity = entity
        .unwrap_or_else(|| LitStr::new(name.to_string().trim_start_matches("r#"), name.span()));
    let crate_path = crate_path.unwrap_or_else(|| syn::parse_quote!(::forklaunch_core));
    let Data::Struct(data) = input.data else {
        return Err(syn::Error::new(
            name.span(),
            "ComplianceMetadata requires a named struct",
        ));
    };
    let Fields::Named(fields) = data.fields else {
        return Err(syn::Error::new(
            name.span(),
            "ComplianceMetadata requires named fields",
        ));
    };
    let mut storage_names = BTreeSet::new();
    let persisted = persisted.unwrap_or(false);
    let mut entries = Vec::new();
    let mut bindings = Vec::new();
    let mut methods = Vec::new();
    let mut checks = Vec::new();
    for field in fields.named {
        let ident = field.ident.expect("named field");
        let rust_name = LitStr::new(ident.to_string().trim_start_matches("r#"), ident.span());
        let mut purpose = None;
        let mut level = None;
        let mut storage_name = None;
        let mut nullable = None;
        for attr in &field.attrs {
            if attr.path().is_ident("compliance") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("level") {
                        let value: LitStr = meta.value()?.parse()?;
                        let variant = match value.value().as_str() {
                            "none" => quote!(None),
                            "pii" => quote!(Pii),
                            "phi" => quote!(Phi),
                            "pci" => quote!(Pci),
                            _ => {
                                return Err(syn::Error::new(
                                    value.span(),
                                    "expected none, pii, phi, or pci",
                                ));
                            }
                        };
                        assign(&mut level, variant, &meta)
                    } else if meta.path.is_ident("name") {
                        let value = checked_name(meta.value()?.parse()?)?;
                        assign(&mut storage_name, value, &meta)
                    } else if meta.path.is_ident("purpose") {
                        let value = checked_name(meta.value()?.parse()?)?;
                        if value.value().len() > 128 {
                            return Err(meta.error("protected purpose must be at most 128 bytes"));
                        }
                        assign(&mut purpose, value, &meta)
                    } else if meta.path.is_ident("nullable") {
                        assign(&mut nullable, true, &meta)
                    } else {
                        Err(meta.error("expected level, name, nullable, or purpose on a field"))
                    }
                })?;
            }
        }
        let storage_name = storage_name.unwrap_or_else(|| rust_name.clone());
        if !storage_names.insert(storage_name.value()) {
            return Err(syn::Error::new(
                storage_name.span(),
                "duplicate compliance storage name",
            ));
        }
        let level = level.ok_or_else(|| syn::Error::new(
            ident.span(),
            "every persisted field requires #[compliance(level = \"none\" | \"pii\" | \"phi\" | \"pci\")]; classification cannot be inferred",
        ))?;
        let protected = level.to_string() != "None";
        if purpose.is_some() && (!persisted || !protected) {
            return Err(syn::Error::new(
                ident.span(),
                "purpose requires a classified field on a persisted model",
            ));
        }
        if persisted && protected {
            let purpose = purpose.ok_or_else(|| {
                syn::Error::new(
                    ident.span(),
                    "persisted protected fields require an explicit purpose",
                )
            })?;
            let method = quote::format_ident!("protected_{}", rust_name.value());
            bindings.push(quote! { #crate_path::persistence::compliance::ProtectedDeclaration { member: #rust_name, purpose: #purpose } });
            methods.push(quote! {
                /// Resolve this stored member's declared encryption policy.
                /// # Errors
                /// Rejects invalid protected metadata; authorization and context remain explicit.
                pub fn #method() -> #crate_path::common::FrameworkResult<#crate_path::persistence::protected::ProtectedMember> {
                    #crate_path::persistence::protected::ProtectedMember::from_entity::<Self>(#rust_name, #purpose)
                }
            });
            checks.push(quote! {
                #crate_path::persistence::protected::storage::ProtectedStorage::validate_binding(&self.#ident, &Self::#method()?)?;
            });
        }
        let nullable = nullable.unwrap_or(false);
        entries.push(quote! {
            #crate_path::persistence::compliance::ComplianceField {
                rust_name: #rust_name, storage_name: #storage_name,
                level: #crate_path::persistence::compliance::ComplianceLevel::#level,
                nullable: #nullable,
            }
        });
    }
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let protection = if persisted {
        quote! { Some(&[#(#bindings),*]) }
    } else {
        quote! { None }
    };
    let protected_impl = persisted.then(|| quote! {
        impl #impl_generics #name #ty_generics #where_clause { #(#methods)* }
        impl #impl_generics #crate_path::persistence::protected::storage::ProtectedModel for #name #ty_generics #where_clause {
            fn validate_protected_storage(&self) -> #crate_path::common::FrameworkResult<()> {
                #(#checks)*
                Ok(())
            }
        }
    });
    Ok(quote! {
        impl #impl_generics #crate_path::persistence::compliance::ComplianceMetadata
            for #name #ty_generics #where_clause {
            const ENTITY: &'static str = #entity;
            const PROTECTED_MEMBERS: Option<&'static [#crate_path::persistence::compliance::ProtectedDeclaration]> = #protection;
            const FIELDS: &'static [#crate_path::persistence::compliance::ComplianceField] = &[#(#entries),*];
        }
        #protected_impl
    })
}

use proc_macro::{Delimiter, TokenTree};

use forklaunch_common::to_pretty_camel_case;

mod api;
mod controller_contract;
mod responses;
mod socket_api;
mod typed;

/// Collect a static set of typed endpoint declarations and generate complete bindings.
#[proc_macro_attribute]
pub fn api(arguments: TokenStream, input: TokenStream) -> TokenStream {
    api::expand(&arguments, input)
}

/// Collect typed socket declarations and require a complete authorized binding set.
#[proc_macro_attribute]
pub fn socket_api(arguments: TokenStream, input: TokenStream) -> TokenStream {
    socket_api::expand(&arguments, input)
}

#[proc_macro_derive(ForklaunchResponses, attributes(response))]
pub fn derive_responses(input: TokenStream) -> TokenStream {
    responses::derive(input)
}

#[proc_macro_derive(ForklaunchContract)]
pub fn derive_forklaunch_contract(input: TokenStream) -> TokenStream {
    typed::derive(input)
}

/// Derive wire serialization and JSON Schema from a struct or enum.
/// Object constraints use Rust member references, resolved through wire renames.
/// `required_with = "other"` makes this member required when `other` is present.
/// Bounds count present properties and must be in `0..=4096`. Dependencies use
/// Rust member names; container and member wire renames are resolved automatically.
/// These constraints are enforced by contract validators (including typed query
/// and header groups), not by standalone Serde deserialization.
/// ```
/// use forklaunch_core::contract::{schema, schema::Optional};
/// #[schema(rename_all = "camelCase", min_properties = 1, max_properties = 2)]
/// struct Pagination {
///     #[field(optional, required_with = "cursor")]
///     page_size: Optional<String>,
///     #[field(optional, rename = "after")]
///     cursor: Optional<String>,
/// }
/// ```
/// Invalid references fail compilation:
/// ```compile_fail
/// #[forklaunch_core::contract::schema]
/// struct Query { #[field(required_with = "missing")] value: String }
/// ```
/// Self references fail compilation:
/// ```compile_fail
/// #[forklaunch_core::contract::schema]
/// struct Query { #[field(required_with = "value")] value: String }
/// ```
/// Inverted or excessive bounds fail compilation:
/// ```compile_fail
/// #[forklaunch_core::contract::schema(min_properties = 3, max_properties = 2)]
/// struct Query { value: String }
/// ```
/// ```compile_fail
/// #[forklaunch_core::contract::schema(max_properties = 4097)]
/// struct Query { value: String }
/// ```
/// Ambiguous serialized names cannot be used for dependencies:
/// ```compile_fail
/// #[forklaunch_core::contract::schema(min_properties = 1)]
/// struct Query { #[field(rename = "same")] first: String, #[field(rename = "same")] second: String }
/// ```
#[proc_macro_attribute]
pub fn schema(arguments: TokenStream, input: TokenStream) -> TokenStream {
    typed::schema(arguments, input)
}

#[proc_macro]
pub fn route_operation_name(input: TokenStream) -> TokenStream {
    let Some(name) = first_string_literal(input) else {
        return compile_error("route_operation_name! expects a single string literal");
    };

    parse_or_compile_error(&format!("{:?}", to_pretty_camel_case(&name)))
}

#[proc_macro]
pub fn assert_route_params(input: TokenStream) -> TokenStream {
    let tokens = input.into_iter().collect::<Vec<_>>();
    let Some(path) = tokens.iter().find_map(string_literal_from_token) else {
        return compile_error("assert_route_params! expects a route path string literal");
    };
    let Some(expected_group) = tokens.iter().find_map(|token| match token {
        TokenTree::Group(group) if group.delimiter() == Delimiter::Bracket => Some(group),
        _ => None,
    }) else {
        return compile_error("assert_route_params! expects expected params in brackets");
    };

    let actual = route_params(&path);
    let expected = expected_group
        .stream()
        .into_iter()
        .filter_map(|token| string_literal_from_token(&token))
        .collect::<Vec<_>>();

    if actual == expected {
        TokenStream::new()
    } else {
        compile_error(&format!(
            "route params do not match path: expected {actual:?}, found {expected:?}"
        ))
    }
}

fn first_string_literal(input: TokenStream) -> Option<String> {
    input
        .into_iter()
        .find_map(|token| string_literal_from_token(&token))
}

fn string_literal_from_token(token: &TokenTree) -> Option<String> {
    let TokenTree::Literal(literal) = token else {
        return None;
    };

    unquote_string_literal(&literal.to_string())
}

fn unquote_string_literal(input: &str) -> Option<String> {
    let input = input.strip_prefix('"')?.strip_suffix('"')?;
    let mut output = String::new();
    let mut chars = input.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        match chars.next()? {
            '"' => output.push('"'),
            '\\' => output.push('\\'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            escaped => output.push(escaped),
        }
    }

    Some(output)
}

fn route_params(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            if let Some(name) = segment.strip_prefix(':') {
                return Some(name.to_owned());
            }

            segment
                .strip_prefix('{')
                .and_then(|name| name.strip_suffix('}'))
                .map(str::to_owned)
        })
        .collect()
}

fn parse_or_compile_error(input: &str) -> TokenStream {
    input
        .parse()
        .unwrap_or_else(|_| compile_error("failed to parse generated ForkLaunch macro output"))
}

fn compile_error(message: &str) -> TokenStream {
    format!("compile_error!({message:?});")
        .parse()
        .unwrap_or_else(|_| TokenStream::new())
}

/// Export only the explicit inline contract module from a controller source file.
#[proc_macro]
pub fn controller_contract(input: TokenStream) -> TokenStream {
    controller_contract::expand(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_metadata_fails_closed() {
        for source in [
            "struct Entity { id: String }",
            "struct Entity { #[compliance(nullable)] email: Option<String> }",
            "enum Entity { A }",
            "struct Entity(String);",
            "struct Entity;",
            "#[compliance(level = \"pii\")] struct Entity { id: String }",
            "#[compliance(entity = \"\")] struct Entity { id: String }",
            "#[compliance(entity = \"One\", entity = \"Two\")] struct Entity { id: String }",
            "struct Entity { #[compliance(level = \"unknown\")] id: String }",
            "struct Entity { #[compliance(nullable, nullable)] id: String }",
            "struct Entity { #[compliance(nullable = false)] id: String }",
            "struct Entity { #[compliance(encrypted)] id: String }",
            "struct Entity { #[compliance(name = \"\")] id: String }",
            "struct Entity { id: String, #[compliance(name = \"id\")] other: String }",
        ] {
            assert!(expand(syn::parse_str(source).unwrap()).is_err(), "{source}");
        }
    }
}
