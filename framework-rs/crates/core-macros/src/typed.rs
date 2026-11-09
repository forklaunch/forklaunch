#[path = "datetime.rs"]
mod datetime;
#[path = "defaults.rs"]
mod defaults;
#[path = "model_identity.rs"]
mod model_identity;
#[path = "normalization.rs"]
mod normalization;
#[path = "object_constraints.rs"]
mod object_constraints;
use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::{format_ident, quote};
use syn::{DeriveInput, parse_macro_input, parse_quote};

pub(crate) fn contract_path() -> syn::Path {
    match crate_name("forklaunch-core") {
        Ok(FoundCrate::Name(name)) => {
            let name = format_ident!("{}", name);
            parse_quote!(::#name::contract)
        }
        Ok(FoundCrate::Itself) | Err(_) => parse_quote!(::forklaunch_core::contract),
    }
}

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    if matches!(input.data, syn::Data::Union(_)) {
        return syn::Error::new_spanned(input.ident, "wire schemas support structs and enums")
            .to_compile_error()
            .into();
    }
    let path = contract_path();
    let name = &input.ident;
    let mut generics = input.generics.clone();
    let (_, type_generics, _) = input.generics.split_for_impl();
    generics.make_where_clause().predicates.push(parse_quote!(
        #name #type_generics: #path::__private::schemars::JsonSchema
    ));
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    quote! {
        impl #impl_generics #path::ContractSchema for #name #type_generics #where_clause {
            fn schema_ref() -> #path::SchemaRef {
                #path::schema_for::<Self>()
            }
        }
    }
    .into()
}

#[allow(clippy::needless_pass_by_value)] // Mirrors the proc-macro entrypoint signature.
pub fn schema(arguments: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_schema(arguments.into(), input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_schema(
    arguments: proc_macro2::TokenStream,
    mut input: DeriveInput,
) -> syn::Result<proc_macro2::TokenStream> {
    use syn::parse::Parser as _;
    let mut rename = None;
    let mut unknown = None;
    let mut object_options = object_constraints::Options::default();
    syn::meta::parser(|meta| {
        if meta.path.is_ident("rename_all") {
            if rename.is_some() { return Err(meta.error("duplicate rename_all")); }
            rename = Some(meta.value()?.parse::<syn::LitStr>()?);
        } else if meta.path.is_ident("unknown_fields") {
            if unknown.is_some() { return Err(meta.error("duplicate unknown_fields")); }
            let value: syn::LitStr = meta.value()?.parse()?;
            if value.value() != "deny" {
                return Err(meta.error("only unknown_fields = \"deny\" is supported; retain/discard require an explicit codec"));
            }
            unknown = Some(value);
        } else if meta.path.is_ident("min_properties") || meta.path.is_ident("max_properties") {
            object_options.set(&meta)?;
        } else {
            return Err(meta.error("unsupported schema option"));
        }
        Ok(())
    }).parse2(arguments)?;
    if matches!(input.data, syn::Data::Union(_)) {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "wire schemas support structs and enums",
        ));
    }
    let named_identity = model_identity::supported(&input);
    let path = contract_path();
    let object_schema =
        object_options.expand(&mut input, rename.as_ref(), named_identity, &path)?;
    let legacy_container = input
        .attrs
        .iter()
        .any(|a| a.path().is_ident("serde") || a.path().is_ident("schemars"));
    if (rename.is_some() || unknown.is_some()) && legacy_container {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "schema options cannot be combined with container serde/schemars overrides",
        ));
    }
    let opaque_container = input.attrs.iter().any(|attribute| {
        (attribute.path().is_ident("serde") || attribute.path().is_ident("schemars"))
            && !simple_legacy_rename(attribute)
    });
    let fields: Vec<_> = match &mut input.data {
        syn::Data::Struct(data) => data.fields.iter_mut().collect(),
        syn::Data::Enum(data) => data
            .variants
            .iter_mut()
            .flat_map(|v| v.fields.iter_mut())
            .collect(),
        syn::Data::Union(_) => unreachable!(),
    };
    for field in fields {
        constrain_field(field, &path, opaque_container)?;
    }
    if let Some(rename) = rename {
        input
            .attrs
            .push(parse_quote!(#[serde(rename_all = #rename)]));
    }
    if unknown.is_some() {
        input
            .attrs
            .push(parse_quote!(#[serde(deny_unknown_fields)]));
    }
    let serde_path = quote!(#path::__private::serde).to_string().replace(' ', "");
    let schema_path = quote!(#path::__private::schemars)
        .to_string()
        .replace(' ', "");
    if named_identity {
        return model_identity::expand(input, &path, &serde_path, &schema_path, &object_schema);
    }
    Ok(quote! {
        #[derive(
            #path::__private::serde::Serialize,
            #path::__private::serde::Deserialize,
            #path::__private::schemars::JsonSchema,
            #path::ForklaunchContract
        )]
        #[serde(crate = #serde_path)]
        #[schemars(crate = #schema_path)]
        #input
    })
}

// A single bidirectional rename policy does not replace field validation/codecs.
// Direction-specific names, flattening and arbitrary schema overrides stay opaque.
fn simple_legacy_rename(attribute: &syn::Attribute) -> bool {
    if !attribute.path().is_ident("serde") {
        return false;
    }
    let mut seen = false;
    attribute
        .parse_nested_meta(|meta| {
            if !meta.path.is_ident("rename_all") || seen {
                return Err(meta.error("not a simple rename policy"));
            }
            let _: syn::LitStr = meta.value()?.parse()?;
            seen = true;
            Ok(())
        })
        .is_ok()
        && seen
}

fn unconstrained_field(
    field: &mut syn::Field,
    path: &syn::Path,
    legacy_container: bool,
) -> syn::Result<()> {
    if contains_optional(&field.ty) {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "Optional requires #[field(optional)] on a direct object member",
        ));
    }
    if contains_patch(&field.ty) {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "Patch requires #[field(patch)] on a direct object member",
        ));
    }
    if !field
        .attrs
        .iter()
        .any(|a| a.path().is_ident("serde") || a.path().is_ident("schemars"))
    {
        if !legacy_container && let Some(attribute) = small_integer_range(&field.ty) {
            field.attrs.push(attribute);
        }
        let ty = &field.ty;
        let wrapper = syn::LitStr::new(
            &quote!(#path::__private::NativeMember<#ty>).to_string(),
            proc_macro2::Span::call_site(),
        );
        field.attrs.push(parse_quote!(#[schemars(with=#wrapper)]));
    }
    Ok(())
}

fn constrain_field(
    field: &mut syn::Field,
    path: &syn::Path,
    legacy_container: bool,
) -> syn::Result<()> {
    let constraints = field
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("field"))
        .collect::<Vec<_>>();
    if constraints.is_empty() {
        if datetime::apply(field, path)? {
            return Ok(());
        }
        return unconstrained_field(field, path, legacy_container);
    }
    if legacy_container
        || field
            .attrs
            .iter()
            .any(|a| a.path().is_ident("serde") || a.path().is_ident("schemars"))
    {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "field constraints cannot be combined with serde/schemars overrides",
        ));
    }
    if constraints.len() != 1 {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "use one field declaration per member",
        ));
    }
    if let Some((rename, remainder)) = field_rename(constraints[0])? {
        field
            .attrs
            .retain(|attribute| !attribute.path().is_ident("field"));
        if !remainder.is_empty() {
            field.attrs.push(parse_quote!(#[field(#remainder)]));
        }
        constrain_field(field, path, legacy_container)?;
        field.attrs.push(parse_quote!(#[serde(rename = #rename)]));
        return Ok(());
    }
    if datetime::requested(field) || datetime::contains_date(&field.ty) {
        datetime::apply(field, path)?;
        return Ok(());
    }
    if defaults::is_default(constraints[0])? {
        let constraint = (*constraints[0]).clone();
        return defaults::apply(field, &constraint, path);
    }
    if normalization::is_normalized(constraints[0])? {
        let constraint = (*constraints[0]).clone();
        return normalization::apply(field, &constraint, path);
    }
    if contains_optional(&field.ty) && has_optional_string_bounds(constraints[0])? {
        let constraint = (*constraints[0]).clone();
        return optional_string_field(field, &constraint, path);
    }
    if has_numeric_bounds(constraints[0])? {
        let constraint = (*constraints[0]).clone();
        return numeric_field(field, &constraint, path);
    }
    if constraints[0]
        .meta
        .require_list()?
        .tokens
        .to_string()
        .trim_end_matches(',')
        .trim()
        == "optional"
    {
        return optional_field(field, path);
    }
    if contains_optional(&field.ty) {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "optional cannot be combined with other codec options",
        ));
    }
    if constraints[0]
        .meta
        .require_list()?
        .tokens
        .to_string()
        .trim_end_matches(',')
        .trim()
        == "patch"
    {
        return patch_field(field, path);
    }
    if contains_patch(&field.ty) {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "patch cannot be combined with other field options",
        ));
    }
    let constraint = (*constraints[0]).clone();
    constrain_string_field(field, &constraint, path)
}

// Field wire names belong to the same declaration as their codec constraints.
fn field_rename(
    attribute: &syn::Attribute,
) -> syn::Result<Option<(syn::LitStr, proc_macro2::TokenStream)>> {
    use syn::punctuated::Punctuated;
    let options =
        attribute.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)?;
    let mut rename = None;
    let mut remaining = Vec::new();
    for option in options {
        if option.path().is_ident("rename") {
            if rename.is_some() {
                return Err(syn::Error::new_spanned(option, "duplicate field rename"));
            }
            let value = option.require_name_value()?;
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(name),
                ..
            }) = &value.value
            else {
                return Err(syn::Error::new_spanned(
                    value,
                    "field rename requires a string",
                ));
            };
            if name.value().is_empty() {
                return Err(syn::Error::new_spanned(
                    name,
                    "field rename cannot be empty",
                ));
            }
            rename = Some(name.clone());
        } else {
            remaining.push(option);
        }
    }
    Ok(rename.map(|rename| (rename, quote!(#(#remaining),*))))
}

fn has_optional_string_bounds(attribute: &syn::Attribute) -> syn::Result<bool> {
    use syn::punctuated::Punctuated;
    let options =
        attribute.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)?;
    Ok(options
        .iter()
        .any(|option| option.path().is_ident("optional"))
        && options.iter().any(|option| {
            option.path().is_ident("min_length") || option.path().is_ident("max_length")
        }))
}

fn optional_string_field(
    field: &mut syn::Field,
    attribute: &syn::Attribute,
    path: &syn::Path,
) -> syn::Result<()> {
    use syn::punctuated::Punctuated;
    let options =
        attribute.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)?;
    if options
        .iter()
        .filter(|o| o.path().is_ident("optional"))
        .count()
        != 1
    {
        return Err(syn::Error::new_spanned(
            attribute,
            "duplicate optional codec",
        ));
    }
    let limits = options.iter().filter(|o| !o.path().is_ident("optional"));
    let constraint: syn::Attribute = parse_quote!(#[field(#(#limits),*)]);
    // Reuse the authoritative String constraint implementation to validate limits.
    let mut string: syn::Field = parse_quote!(value: String);
    constrain_string_field(&mut string, &constraint, path)?;
    let (min, max) = length_limits(&constraint)?;
    let min = min.unwrap_or(0);
    let max = max.unwrap_or(u32::MAX as usize);
    let deserialize = syn::LitStr::new(
        &quote!(#path::__private::deserialize_optional_string::<_,#min,#max>).to_string(),
        proc_macro2::Span::call_site(),
    );
    let serialize = syn::LitStr::new(
        &quote!(#path::__private::serialize_optional_string::<_,#min,#max>).to_string(),
        proc_macro2::Span::call_site(),
    );
    optional_field(field, path)?;
    field.attrs.extend(
        string
            .attrs
            .into_iter()
            .filter(|a| a.path().is_ident("schemars")),
    );
    field
        .attrs
        .push(parse_quote!(#[serde(deserialize_with=#deserialize,serialize_with=#serialize)]));
    Ok(())
}

fn has_numeric_bounds(attribute: &syn::Attribute) -> syn::Result<bool> {
    use syn::punctuated::Punctuated;
    let options =
        attribute.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)?;
    Ok(options
        .iter()
        .any(|option| option.path().is_ident("min") || option.path().is_ident("max")))
}

fn numeric_field(
    field: &mut syn::Field,
    attribute: &syn::Attribute,
    path: &syn::Path,
) -> syn::Result<()> {
    const SAFE: i64 = 9_007_199_254_740_991;
    let mut min = None;
    let mut max = None;
    let mut optional = false;
    attribute.parse_nested_meta(|meta| {
        if meta.path.is_ident("optional") {
            if optional {
                return Err(meta.error("duplicate optional codec"));
            }
            optional = true;
            return Ok(());
        }
        let target = if meta.path.is_ident("min") {
            &mut min
        } else if meta.path.is_ident("max") {
            &mut max
        } else {
            return Err(meta.error("integer bounds support min, max and optional only"));
        };
        if target.is_some() {
            return Err(meta.error("duplicate numeric bound"));
        }
        let expression: syn::Expr = meta.value()?.parse()?;
        let text = quote!(#expression).to_string().replace([' ', '_'], "");
        *target = Some(
            text.parse::<i64>()
                .map_err(|_| meta.error("numeric bound requires an i64 literal"))?,
        );
        Ok(())
    })?;
    let min = min.unwrap_or(-SAFE);
    let max = max.unwrap_or(SAFE);
    if min < -SAFE || max > SAFE || min > max {
        return Err(syn::Error::new_spanned(
            attribute,
            "integer bounds must be ordered within JavaScript's exact integer range",
        ));
    }
    let ty = &field.ty;
    let deserialize = syn::LitStr::new(
        &quote!(#path::__private::deserialize_integer::<_, #ty, #min, #max>).to_string(),
        proc_macro2::Span::call_site(),
    );
    let serialize = syn::LitStr::new(
        &quote!(#path::__private::serialize_integer::<_, #ty, #min, #max>).to_string(),
        proc_macro2::Span::call_site(),
    );
    if optional {
        optional_field(field, path)?;
    } else if contains_optional(&field.ty) || contains_patch(&field.ty) {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "omission codecs require their explicit field binding",
        ));
    }
    field.attrs.retain(|a| !a.path().is_ident("field"));
    if matches!(&field.ty, syn::Type::Path(p) if p.path.segments.last().is_some_and(|s|s.ident=="Option"))
    {
        field.attrs.push(parse_quote!(#[serde(default)]));
    }
    field
        .attrs
        .push(parse_quote!(#[serde(deserialize_with=#deserialize, serialize_with=#serialize)]));
    field
        .attrs
        .push(parse_quote!(#[schemars(range(min=#min,max=#max))]));
    Ok(())
}

fn constrain_string_field(
    field: &mut syn::Field,
    constraint: &syn::Attribute,
    path: &syn::Path,
) -> syn::Result<()> {
    let (minimum, maximum) = length_limits(constraint)?;
    if minimum.is_none() && maximum.is_none() {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "field requires a supported constraint",
        ));
    }
    if !matches!(&field.ty, syn::Type::Path(p) if p.qself.is_none() && p.path.segments.last().is_some_and(|s| s.ident == "String" && s.arguments.is_empty()))
    {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "length constraints currently require String",
        ));
    }
    let min = minimum.unwrap_or(0);
    let max = maximum.unwrap_or(u32::MAX as usize);
    if max > u32::MAX as usize || min > u32::MAX as usize {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "length limits must fit u32 for portable schema compilation",
        ));
    }
    if min > max {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "min_length exceeds max_length",
        ));
    }
    let deserialize = syn::LitStr::new(
        &quote!(#path::__private::deserialize_string::<_, #min, #max>).to_string(),
        proc_macro2::Span::call_site(),
    );
    let serialize = syn::LitStr::new(
        &quote!(#path::__private::serialize_string::<_, #min, #max>).to_string(),
        proc_macro2::Span::call_site(),
    );
    field.attrs.retain(|a| !a.path().is_ident("field"));
    field
        .attrs
        .push(parse_quote!(#[serde(deserialize_with = #deserialize, serialize_with = #serialize)]));
    // Schemars accepts one `length` item per field.
    match (minimum, maximum) {
        (Some(min), Some(max)) => field
            .attrs
            .push(parse_quote!(#[schemars(length(min = #min, max = #max))])),
        (Some(min), None) => field
            .attrs
            .push(parse_quote!(#[schemars(length(min = #min))])),
        (None, Some(max)) => field
            .attrs
            .push(parse_quote!(#[schemars(length(max = #max))])),
        (None, None) => {}
    }
    Ok(())
}

fn contains_patch(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(p) => p.path.segments.iter().any(|segment| {
            segment.ident == "Patch" || match &segment.arguments {
                syn::PathArguments::AngleBracketed(args) => args.args.iter().any(
                    |arg| matches!(arg, syn::GenericArgument::Type(inner) if contains_patch(inner)),
                ),
                _ => false,
            }
        }),
        syn::Type::Reference(reference) => contains_patch(&reference.elem),
        syn::Type::Tuple(tuple) => tuple.elems.iter().any(contains_patch),
        syn::Type::Array(array) => contains_patch(&array.elem),
        _ => false,
    }
}

fn contains_optional(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(p) => p.path.segments.iter().any(|segment| {
            segment.ident == "Optional" || match &segment.arguments {
                syn::PathArguments::AngleBracketed(args) => args.args.iter().any(
                    |arg| matches!(arg, syn::GenericArgument::Type(inner) if contains_optional(inner)),
                ),
                _ => false,
            }
        }),
        syn::Type::Reference(reference) => contains_optional(&reference.elem),
        syn::Type::Tuple(tuple) => tuple.elems.iter().any(contains_optional),
        syn::Type::Array(array) => contains_optional(&array.elem),
        _ => false,
    }
}

fn optional_field(field: &mut syn::Field, path: &syn::Path) -> syn::Result<()> {
    let inner = match &field.ty {
        syn::Type::Path(p) if p.qself.is_none() => p
            .path
            .segments
            .last()
            .filter(|s| s.ident == "Optional")
            .and_then(|s| match &s.arguments {
                syn::PathArguments::AngleBracketed(args) if args.args.len() == 1 => {
                    match args.args.first() {
                        Some(syn::GenericArgument::Type(inner)) => Some(inner),
                        _ => None,
                    }
                }
                _ => None,
            }),
        _ => None,
    }
    .ok_or_else(|| {
        syn::Error::new_spanned(
            &field.ty,
            "#[field(optional)] requires a direct Optional<T> object member",
        )
    })?;
    if field.ident.is_none() || contains_optional(inner) || contains_patch(inner) {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "nested omission codecs or unnamed optional members are not supported",
        ));
    }
    let schema_type = syn::LitStr::new(&quote!(#inner).to_string(), proc_macro2::Span::call_site());
    let skip = syn::LitStr::new(
        &quote!(#path::schema::Optional::is_missing).to_string(),
        proc_macro2::Span::call_site(),
    );
    field.attrs.retain(|a| !a.path().is_ident("field"));
    field
        .attrs
        .push(parse_quote!(#[serde(default, skip_serializing_if = #skip)]));
    field
        .attrs
        .push(parse_quote!(#[schemars(with = #schema_type)]));
    Ok(())
}

fn patch_field(field: &mut syn::Field, path: &syn::Path) -> syn::Result<()> {
    let inner = match &field.ty {
        syn::Type::Path(p) if p.qself.is_none() => p
            .path
            .segments
            .last()
            .filter(|s| s.ident == "Patch")
            .and_then(|s| match &s.arguments {
                syn::PathArguments::AngleBracketed(args) if args.args.len() == 1 => {
                    match args.args.first() {
                        Some(syn::GenericArgument::Type(inner)) => Some(inner),
                        _ => None,
                    }
                }
                _ => None,
            }),
        _ => None,
    }
    .ok_or_else(|| {
        syn::Error::new_spanned(
            &field.ty,
            "#[field(patch)] requires a direct Patch<T> object member",
        )
    })?;
    if field.ident.is_none() || contains_patch(inner) || contains_optional(inner) {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "nested Patch or unnamed patch members are not supported",
        ));
    }
    let schema_type = syn::LitStr::new(
        &quote!(::core::option::Option<#inner>).to_string(),
        proc_macro2::Span::call_site(),
    );
    let skip = syn::LitStr::new(
        &quote!(#path::schema::Patch::is_keep).to_string(),
        proc_macro2::Span::call_site(),
    );
    let integer_range = small_integer_range(inner);
    field.attrs.retain(|a| !a.path().is_ident("field"));
    if let Some(attribute) = integer_range {
        field.attrs.push(attribute);
    }
    field
        .attrs
        .push(parse_quote!(#[serde(default, skip_serializing_if = #skip)]));
    field
        .attrs
        .push(parse_quote!(#[schemars(with = #schema_type)]));
    Ok(())
}

// Schemars formats alone do not constrain numeric validation. These bounds are
// exactly representable in both JSON Schema's numeric projection and JavaScript.
fn small_integer_range(ty: &syn::Type) -> Option<syn::Attribute> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if path.qself.is_some() || !segment.arguments.is_empty() {
        return None;
    }
    let names = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    if names.len() != 1
        && !(names.len() == 3
            && matches!(names[0].as_str(), "std" | "core")
            && names[1] == "primitive")
    {
        return None;
    }
    let (min, max): (i64, i64) = match segment.ident.to_string().as_str() {
        "u8" => (0, 255),
        "u16" => (0, 65_535),
        "u32" => (0, 4_294_967_295),
        "i8" => (-128, 127),
        "i16" => (-32_768, 32_767),
        "i32" => (-2_147_483_648, 2_147_483_647),
        _ => return None,
    };
    Some(parse_quote!(#[schemars(range(min = #min, max = #max))]))
}

fn length_limits(attribute: &syn::Attribute) -> syn::Result<(Option<usize>, Option<usize>)> {
    let mut minimum = None;
    let mut maximum = None;
    attribute.parse_nested_meta(|meta| {
        let target = if meta.path.is_ident("min_length") {
            &mut minimum
        } else if meta.path.is_ident("max_length") {
            &mut maximum
        } else {
            return Err(meta.error(
                "unsupported field option; defaults and arbitrary transformations are not implemented",
            ));
        };
        if target.is_some() {
            return Err(meta.error("duplicate field constraint"));
        }
        let literal: syn::LitInt = meta.value()?.parse()?;
        *target = Some(literal.base10_parse::<usize>()?);
        Ok(())
    })?;
    Ok((minimum, maximum))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[allow(clippy::too_many_lines)] // One table of cases.
    fn invalid_constraints_fail_for_the_declared_reason() {
        for (source, diagnostic) in [
            (
                "struct Input { #[field(default,min_length=1)] value: String }",
                "Rust Default violates",
            ),
            (
                "struct Input { #[field(default,min=1)] value: i64 }",
                "Rust Default violates",
            ),
            (
                "struct Input { #[field(default)] value: i64 }",
                "wide integer defaults require",
            ),
            (
                "struct Input { #[field(default,optional)] value: Optional<String> }",
                "default requires a supported direct scalar",
            ),
            (
                "struct Input(#[field(default)] String);",
                "default requires a named object member",
            ),
            (
                "struct Input { #[field(default=\"hello\")] value: String }",
                "configured defaults are not implemented",
            ),
            (
                "struct Input { #[field(normalize=Normalization::TrimAscii)] value: String }",
                "normalization requires wire_max_length",
            ),
            (
                "struct Input { #[field(wire_max_length=4)] value: String }",
                "requires an explicit normalization",
            ),
            (
                "struct Input { #[field(wire_max_length=4, normalize=Normalization::TrimAscii,max_length=5)] value: String }",
                "normalized bounds must satisfy",
            ),
            (
                "struct Input { #[field(wire_max_length=4, normalize=Normalization::Unicode)] value: String }",
                "only Normalization::TrimAscii",
            ),
            (
                "struct Input { #[field(optional,wire_max_length=4, normalize=Normalization::TrimAscii)] value: Optional<String> }",
                "requires a direct String",
            ),
            (
                "struct Input { #[field(min=9007199254740992)] value: i64 }",
                "JavaScript's exact integer range",
            ),
            (
                "struct Input { #[field(min=2,max=1)] value: i64 }",
                "must be ordered",
            ),
            (
                "struct Input { #[field(min=0,min=1)] value: i64 }",
                "duplicate numeric bound",
            ),
            (
                "struct Input { value: Optional<String> }",
                "Optional requires #[field(optional)]",
            ),
            (
                "struct Input { #[field(optional)] value: String }",
                "requires a direct Optional<T>",
            ),
            (
                "struct Input { #[field(optional, patch)] value: Optional<String> }",
                "optional cannot be combined",
            ),
            (
                "struct Input { #[field(optional)] value: Optional<Patch<String>> }",
                "nested omission codecs",
            ),
            (
                "struct Input { #[field(optional)] #[serde(default)] value: Optional<String> }",
                "cannot be combined with serde/schemars",
            ),
            (
                "struct Input { #[field(rename = \"\")] value: String }",
                "field rename cannot be empty",
            ),
            (
                "struct Input { #[field(rename = 1)] value: String }",
                "field rename requires a string",
            ),
            (
                "struct Input { #[field(rename = \"a\", rename = \"b\")] value: String }",
                "duplicate field rename",
            ),
            (
                "struct Input { #[field(patch)] value: String }",
                "requires a direct Patch<T>",
            ),
            (
                "struct Input { value: Patch<String> }",
                "Patch requires #[field(patch)]",
            ),
            (
                "struct Input { #[field(patch)] value: Vec<Patch<String>> }",
                "requires a direct Patch<T>",
            ),
            (
                "struct Input { #[field(patch)] value: Patch<Patch<String>> }",
                "nested Patch",
            ),
            (
                "struct Input { #[field(patch, min_length=1)] value: Patch<String> }",
                "cannot be combined",
            ),
            (
                "struct Input { #[field(patch)] #[serde(default)] value: Patch<String> }",
                "cannot be combined",
            ),
            (
                "struct Input { #[field(min_length = 4, max_length = 2)] title: String }",
                "min_length exceeds max_length",
            ),
            (
                "struct Input { #[field(min_length = 1)] title: u32 }",
                "length constraints currently require String",
            ),
            (
                "struct Input { #[field(normalize = TrimAscii)] title: String }",
                "only Normalization::TrimAscii is supported",
            ),
            (
                "struct Input { #[field(max_length = 2, max_length = 3)] title: String }",
                "duplicate field constraint",
            ),
            (
                "struct Input { #[field(min_length = 1)] #[serde(skip)] title: String }",
                "field constraints cannot be combined",
            ),
        ] {
            let error = expand_schema(
                proc_macro2::TokenStream::new(),
                syn::parse_str(source).unwrap(),
            )
            .unwrap_err();
            assert!(error.to_string().contains(diagnostic), "{error}");
        }
    }
    #[test]
    fn utc_date_fields_own_their_codec() {
        let expanded = expand_schema(
            proc_macro2::TokenStream::new(),
            syn::parse_str("struct Input { date: DateTime<Utc> }").unwrap(),
        )
        .unwrap()
        .to_string();
        assert!(expanded.contains("datetime :: deserialize"), "{expanded}");
        for source in [
            "struct Input { date: Optional<DateTime<Utc>> }",
            "struct Input(DateTime<Utc>);",
        ] {
            assert!(
                expand_schema(
                    proc_macro2::TokenStream::new(),
                    syn::parse_str(source).unwrap()
                )
                .is_err()
            );
        }
    }
}
