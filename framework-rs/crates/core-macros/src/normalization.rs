//! Explicit portable string transformation grammar; no executable user callbacks.
use quote::quote;
use syn::parse_quote;

pub(crate) fn is_normalized(attribute: &syn::Attribute) -> syn::Result<bool> {
    let options = attribute.parse_args_with(
        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
    )?;
    Ok(options.iter().any(|option| {
        option.path().is_ident("normalize") || option.path().is_ident("wire_max_length")
    }))
}

pub(crate) fn apply(
    field: &mut syn::Field,
    attribute: &syn::Attribute,
    path: &syn::Path,
) -> syn::Result<()> {
    if !matches!(&field.ty, syn::Type::Path(p) if p.qself.is_none() && p.path.segments.last().is_some_and(|s| s.ident=="String" && s.arguments.is_empty()))
    {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "normalization currently requires a direct String field",
        ));
    }
    let mut normalization = None;
    let mut wire = None;
    let mut minimum = None;
    let mut maximum = None;
    attribute.parse_nested_meta(|meta| {
        if meta.path.is_ident("normalize") {
            if normalization.is_some() { return Err(meta.error("duplicate normalization")); }
            let expression: syn::ExprPath = meta.value()?.parse()?;
            let segments = expression.path.segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>();
            if expression.qself.is_some() || segments.len()<2 || segments[segments.len()-2..] != ["Normalization", "TrimAscii"] {
                return Err(meta.error("only Normalization::TrimAscii is supported; arbitrary transformations are not portable"));
            }
            normalization = Some(expression);
            return Ok(());
        }
        let target = if meta.path.is_ident("wire_max_length") { &mut wire }
            else if meta.path.is_ident("min_length") { &mut minimum }
            else if meta.path.is_ident("max_length") { &mut maximum }
            else { return Err(meta.error("unsupported normalized field option; defaults and optional normalization are not implemented")); };
        if target.is_some() { return Err(meta.error("duplicate normalized string bound")); }
        let literal: syn::LitInt = meta.value()?.parse()?;
        *target = Some(literal.base10_parse::<usize>()?);
        Ok(())
    })?;
    let normalization = normalization.ok_or_else(|| {
        syn::Error::new_spanned(
            attribute,
            "wire_max_length requires an explicit normalization",
        )
    })?;
    let wire = wire.ok_or_else(|| {
        syn::Error::new_spanned(attribute, "normalization requires wire_max_length")
    })?;
    let minimum = minimum.unwrap_or(0);
    let maximum = maximum.unwrap_or(wire);
    if wire > u32::MAX as usize || minimum > maximum || maximum > wire {
        return Err(syn::Error::new_spanned(
            attribute,
            "normalized bounds must satisfy min_length <= max_length <= wire_max_length <= u32::MAX",
        ));
    }
    // Preserve name resolution/type checking of the author's enum expression.
    let wire_argument = quote!({let _: #path::codec::Normalization = #normalization; #wire});
    let deserialize = syn::LitStr::new(
        &quote!(#path::codec::deserialize_trim_ascii::<_, #wire_argument, #minimum, #maximum>)
            .to_string(),
        proc_macro2::Span::call_site(),
    );
    let serialize = syn::LitStr::new(
        &quote!(#path::codec::serialize_trim_ascii::<_, #minimum, #maximum>).to_string(),
        proc_macro2::Span::call_site(),
    );
    let schema = syn::LitStr::new(
        &quote!(#path::codec::trim_ascii_schema::<#wire_argument, #minimum, #maximum>).to_string(),
        proc_macro2::Span::call_site(),
    );
    field
        .attrs
        .retain(|attribute| !attribute.path().is_ident("field"));
    field
        .attrs
        .push(parse_quote!(#[serde(deserialize_with=#deserialize,serialize_with=#serialize)]));
    field
        .attrs
        .push(parse_quote!(#[schemars(schema_with=#schema)]));
    Ok(())
}
