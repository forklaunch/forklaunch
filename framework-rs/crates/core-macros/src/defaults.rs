//! Literal-free Rust Default support with compile-time constraint compatibility.
use quote::{ToTokens, quote};
use syn::parse_quote;

pub(crate) fn is_default(attribute: &syn::Attribute) -> syn::Result<bool> {
    let options = attribute.parse_args_with(
        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
    )?;
    Ok(options
        .iter()
        .any(|option| option.path().is_ident("default")))
}
pub(crate) fn apply(
    field: &mut syn::Field,
    attribute: &syn::Attribute,
    path: &syn::Path,
) -> syn::Result<()> {
    let name = default_type(field)?;
    let options = attribute.parse_args_with(
        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
    )?;
    let mut seen = false;
    let mut remaining = Vec::new();
    for option in options {
        if option.path().is_ident("default") {
            if seen || !matches!(option, syn::Meta::Path(_)) {
                return Err(syn::Error::new_spanned(
                    option,
                    "use one bare default marker; configured defaults are not implemented",
                ));
            }
            seen = true;
            continue;
        }
        let key = option
            .path()
            .get_ident()
            .map(ToString::to_string)
            .unwrap_or_default();
        let allowed = match name.as_str() {
            "String" => matches!(key.as_str(), "min_length" | "max_length"),
            "i64" | "u64" => matches!(key.as_str(), "min" | "max"),
            _ => false,
        };
        if !allowed {
            return Err(syn::Error::new_spanned(
                option,
                "unsupported default/codec combination",
            ));
        }
        let value = option.require_name_value()?;
        let expression = &value.value;
        let literal = quote!(#expression)
            .to_string()
            .replace([' ', '_'], "")
            .parse::<i64>()
            .map_err(|_| {
                syn::Error::new_spanned(expression, "default constraints require integer literals")
            })?;
        if (matches!(key.as_str(), "min" | "min_length") && literal > 0)
            || (matches!(key.as_str(), "max" | "max_length") && literal < 0)
        {
            return Err(syn::Error::new_spanned(
                option,
                "Rust Default violates the declared constraint",
            ));
        }
        remaining.push(option);
    }
    if matches!(name.as_str(), "i64" | "u64") && remaining.is_empty() {
        return Err(syn::Error::new_spanned(
            attribute,
            "wide integer defaults require explicit portable min/max bounds",
        ));
    }
    field
        .attrs
        .retain(|attribute| !attribute.path().is_ident("field"));
    if !remaining.is_empty() {
        field.attrs.push(parse_quote!(#[field(#(#remaining),*)]));
    }
    super::constrain_field(field, path, false)?;
    // The scalar default owns its schema adapter; retain its generated constraints.
    field.attrs.retain(|attribute| {
        !attribute.path().is_ident("schemars")
            || !attribute
                .to_token_stream()
                .to_string()
                .contains("NativeMember")
    });
    let ty = &field.ty;
    let schema = syn::LitStr::new(
        &quote!(#path::codec::default_schema::<#ty>).to_string(),
        proc_macro2::Span::call_site(),
    );
    field.attrs.push(parse_quote!(#[serde(default)]));
    field
        .attrs
        .push(parse_quote!(#[schemars(schema_with=#schema)]));
    Ok(())
}

fn default_type(field: &syn::Field) -> syn::Result<String> {
    if field.ident.is_none() {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "default requires a named object member",
        ));
    }
    let name = match &field.ty {
        syn::Type::Path(p) if p.qself.is_none() => p
            .path
            .segments
            .last()
            .filter(|segment| segment.arguments.is_empty())
            .map(|segment| segment.ident.to_string()),
        _ => None,
    }
    .ok_or_else(|| {
        syn::Error::new_spanned(&field.ty, "default requires a supported direct scalar type")
    })?;
    if !matches!(
        name.as_str(),
        "String" | "bool" | "i8" | "i16" | "i32" | "u8" | "u16" | "u32" | "i64" | "u64"
    ) {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "default requires a supported direct scalar type",
        ));
    }
    Ok(name)
}
