//! Preserve a DTO's declared identity when Schemars embeds it in another DTO.
use quote::quote;
use syn::{DeriveInput, parse_quote};
#[path = "schema_self.rs"]
mod schema_self;

// Raw schema overrides and lifetime declarations retain their existing derive.
// Their identities are only authoritative at the explicit schema root for now.
pub(super) fn supported(input: &DeriveInput) -> bool {
    input.generics.lifetimes().next().is_none()
        && matches!(&input.data, syn::Data::Struct(data) if matches!(data.fields, syn::Fields::Named(_)))
        && !input.attrs.iter().any(raw_override)
        && match &input.data {
            syn::Data::Struct(data) => !data
                .fields
                .iter()
                .any(|field| field.attrs.iter().any(raw_override)),
            _ => false,
        }
}
fn raw_override(attribute: &syn::Attribute) -> bool {
    attribute.path().is_ident("serde") || attribute.path().is_ident("schemars")
}

pub(super) fn expand(
    mut input: DeriveInput,
    path: &syn::Path,
    serde_path: &str,
    schema_path: &str,
    object_schema: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let name = input.ident.clone();
    let schema_name = name.to_string();
    let type_parameters = input
        .generics
        .type_params()
        .map(|parameter| &parameter.ident)
        .collect::<Vec<_>>();
    let const_parameters = input
        .generics
        .const_params()
        .map(|parameter| &parameter.ident)
        .collect::<Vec<_>>();
    let count = type_parameters.len() + const_parameters.len();
    let mut identity_format = schema_name.clone();
    if count > 0 {
        identity_format.push_str("_for_{}");
        identity_format.push_str(&"_and_{}".repeat(count - 1));
    }
    let mut generics = input.generics.clone();
    for parameter in input.generics.type_params() {
        let ident = &parameter.ident;
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ident: #path::__private::schemars::JsonSchema));
    }
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let mut mirror = input.clone();
    mirror.ident = parse_quote!(__ForklaunchSchema);
    mirror.vis = syn::Visibility::Inherited;
    schema_self::rewrite(&mut mirror, parse_quote!(#name #type_generics))?;
    mirror
        .attrs
        .retain(|attribute| !attribute.path().is_ident("derive"));
    if let syn::Data::Struct(data) = &mut input.data {
        for field in &mut data.fields {
            field
                .attrs
                .retain(|attribute| !attribute.path().is_ident("schemars"));
        }
    }
    Ok(quote! {
        #[derive(#path::__private::serde::Serialize, #path::__private::serde::Deserialize, #path::ForklaunchContract)]
        #[serde(crate = #serde_path)]
        #input
        const _: () = {
            #[allow(dead_code)]
            #[derive(#path::__private::schemars::JsonSchema)]
            #[schemars(crate = #schema_path, rename = #schema_name)]
            #mirror
            impl #impl_generics #path::__private::schemars::JsonSchema for #name #type_generics #where_clause {
                fn schema_name() -> ::std::borrow::Cow<'static, str> { ::std::borrow::Cow::Owned(format!(#identity_format #(, #path::__private::legacy_schema_name(&<#type_parameters as #path::__private::schemars::JsonSchema>::schema_name()))* #(, #const_parameters)*)) }
                fn schema_id() -> ::std::borrow::Cow<'static, str> { ::std::borrow::Cow::Owned(format!(concat!(module_path!(), "::", #identity_format) #(, #path::__private::legacy_schema_name(&<#type_parameters as #path::__private::schemars::JsonSchema>::schema_id()))* #(, #const_parameters)*)) }
                fn json_schema(generator: &mut #path::__private::schemars::SchemaGenerator) -> #path::__private::schemars::Schema {
                    let schema = <__ForklaunchSchema #type_generics as #path::__private::schemars::JsonSchema>::json_schema(generator);
                    #object_schema
                    #path::__private::named_schema::<Self>(schema)
                }
            }
        };
    })
}
