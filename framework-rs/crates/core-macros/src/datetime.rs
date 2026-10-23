//! Checked native date shapes and explicit alias policy.
use quote::{ToTokens, quote};
use syn::parse_quote;

pub(crate) fn requested(field: &syn::Field) -> bool {
    field
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("field"))
        .any(|a| a.to_token_stream().to_string().contains("codec"))
}

pub(crate) fn apply(field: &mut syn::Field, path: &syn::Path) -> syn::Result<bool> {
    let direct = contains_date(&field.ty);
    if !direct && !requested(field) {
        return Ok(false);
    }
    if field.ident.is_none()
        || field
            .attrs
            .iter()
            .any(|a| a.path().is_ident("serde") || a.path().is_ident("schemars"))
    {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "date codecs require a named member without serde/schemars overrides",
        ));
    }
    let mut optional = false;
    let mut expression = None;
    for attribute in field.attrs.iter().filter(|a| a.path().is_ident("field")) {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("optional") && !optional {optional=true;return Ok(())}
            if meta.path.is_ident("codec") && expression.is_none() {
                let codec:syn::ExprPath=meta.value()?.parse()?;
                if codec.path.segments.last().is_none_or(|s|s.ident!="UtcMilliseconds") {return Err(meta.error("only DateCodec::UtcMilliseconds is supported"));}
                expression=Some(codec);return Ok(())
            }
            Err(meta.error("date codec supports only codec and optional; constraints/defaults/PATCH are unsupported"))
        })?;
    }
    let outer = match &field.ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    };
    if optional != (outer.as_deref() == Some("Optional")) {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "Optional date members require #[field(optional)] and a direct Optional<T>",
        ));
    }
    let ty = &field.ty;
    let schema_function = if let Some(expression) = expression {
        quote!(#path::datetime::checked_shape_schema::<#ty, { let _: #path::datetime::DateCodec = #expression; true }>)
    } else {
        quote!(#path::datetime::shape_schema::<#ty>)
    };
    let deserialize = syn::LitStr::new(
        &quote!(#path::datetime::deserialize).to_string(),
        proc_macro2::Span::call_site(),
    );
    let serialize = syn::LitStr::new(
        &quote!(#path::datetime::serialize).to_string(),
        proc_macro2::Span::call_site(),
    );
    let schema = syn::LitStr::new(&schema_function.to_string(), proc_macro2::Span::call_site());
    field.attrs.retain(|a| !a.path().is_ident("field"));
    field
        .attrs
        .push(parse_quote!(#[serde(deserialize_with=#deserialize,serialize_with=#serialize)]));
    field
        .attrs
        .push(parse_quote!(#[schemars(schema_with=#schema)]));
    if optional {
        let skip = syn::LitStr::new(
            &quote!(#path::schema::Optional::is_missing).to_string(),
            proc_macro2::Span::call_site(),
        );
        field
            .attrs
            .push(parse_quote!(#[serde(default,skip_serializing_if=#skip)]));
    } else if outer.as_deref() == Some("Option") {
        field.attrs.push(parse_quote!(#[serde(default)]));
    }
    Ok(true)
}

pub(crate) fn contains_date(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(path) => path.path.segments.iter().any(|segment| segment.ident == "DateTime" || match &segment.arguments {
            syn::PathArguments::AngleBracketed(args) => args.args.iter().any(|argument| matches!(argument,syn::GenericArgument::Type(ty) if contains_date(ty))),
            _=>false,
        }),
        syn::Type::Array(array)=>contains_date(&array.elem),
        syn::Type::Reference(reference)=>contains_date(&reference.elem),
        syn::Type::Tuple(tuple)=>tuple.elems.iter().any(contains_date),
        _=>false,
    }
}
