use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Fields, parse_macro_input};

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
#[allow(clippy::too_many_lines)]
fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let path = crate::typed::contract_path();
    let syn::Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "response variants require an enum",
        ));
    };
    let mut statuses = std::collections::BTreeSet::new();
    let mut contracts = Vec::new();
    let mut encoders = Vec::new();
    for variant in &data.variants {
        let mut status = None;
        let mut media = None;
        let mut media_override: Option<syn::Ident> = None;
        let mut custom_media: Option<syn::LitStr> = None;
        for attr in variant
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("response"))
        {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("status") {
                    if status.is_some() {
                        return Err(meta.error("duplicate status"));
                    }
                    status = Some(
                        meta.value()?
                            .parse::<syn::LitInt>()?
                            .base10_parse::<u16>()?,
                    );
                } else if meta.path.is_ident("content_type") {
                    if media.is_some() {
                        return Err(meta.error("duplicate content type"));
                    }
                    media = Some(meta.value()?.parse::<syn::Ident>()?.to_string());
                } else if meta.path.is_ident("media_type") {
                    if media_override.is_some() || custom_media.is_some() {
                        return Err(meta.error("duplicate media override"));
                    }
                    let input = meta.value()?;
                    if input.peek(syn::LitStr) {
                        custom_media = Some(input.parse()?);
                    } else {
                        media_override = Some(input.parse()?);
                    }
                } else {
                    return Err(meta.error("expected status, content_type or media_type"));
                }
                Ok(())
            })?;
        }
        let code = status.ok_or_else(|| {
            syn::Error::new_spanned(variant, "every response variant requires status")
        })?;
        let representation = if matches!(variant.fields, Fields::Unit) {
            "Empty"
        } else {
            media.as_deref().unwrap_or("Json")
        };
        let effective_media = if let Some(value) = &custom_media {
            value
                .value()
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        } else {
            media_override
                .as_ref()
                .map_or_else(|| representation.to_owned(), ToString::to_string)
        };
        if !(200..600).contains(&code) || !statuses.insert((code, effective_media)) {
            return Err(syn::Error::new_spanned(
                variant,
                "response status/content-type pairs must be unique and statuses between 200 and 599",
            ));
        }
        let name = &variant.ident;
        match &variant.fields {
            Fields::Unit => {
                if media.is_some() || media_override.is_some() || custom_media.is_some() {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "empty response has no content type",
                    ));
                }
                contracts.push(quote! { #path::ResponseContract::new(#code, None) });
                encoders.push(
                    quote! { Self::#name => Ok(#path::endpoint::EncodedResponse::empty(#code)) },
                );
            }
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                if matches!(code, 204 | 205 | 304) {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "204, 205 and 304 responses cannot contain a body",
                    ));
                }
                let value = &fields.unnamed[0].ty;
                let (body, encode) = match media.as_deref().unwrap_or("Json") {
                    "Json" => (
                        quote! { #path::BodyContract::json_type::<#value>() },
                        quote! { #path::endpoint::EncodedResponse::json(#code, &value) },
                    ),
                    "Text" => (
                        quote! { #path::BodyContract::typed::<String>(#path::ContentType::Text) },
                        quote! { Ok(#path::endpoint::EncodedResponse::text(#code, value)) },
                    ),
                    "Html" => (
                        quote! { #path::BodyContract::typed::<String>(#path::ContentType::Html) },
                        quote! { Ok(#path::endpoint::EncodedResponse::html(#code, value)) },
                    ),
                    "Binary" => (
                        quote! { #path::BodyContract::typed::<String>(#path::ContentType::Binary) },
                        quote! { Ok(#path::endpoint::EncodedResponse::binary(#code, value)) },
                    ),
                    _ => {
                        return Err(syn::Error::new_spanned(
                            variant,
                            "response enums support Json, Text, Html, Binary, or empty unit variants",
                        ));
                    }
                };
                let (body, encode) = if let Some(custom) = &custom_media {
                    let mime = custom.value();
                    let normalized = mime.split(';').next().unwrap_or("").trim();
                    if !normalized
                        .split_once('/')
                        .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty() && !b.contains('/'))
                        || !normalized.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || b"/!#$&^_.+-".contains(&byte)
                        })
                        || mime.chars().any(char::is_control)
                    {
                        return Err(syn::Error::new_spanned(
                            custom,
                            "invalid custom response MIME type",
                        ));
                    }
                    let codec = syn::Ident::new(media.as_deref().unwrap_or("Json"), custom.span());
                    (
                        quote! { (#body).with_media_type(#path::MediaType::Custom {representation:#path::ContentType::#codec,value:#custom}).expect("derived valid custom media") },
                        quote! { (#encode).map(|mut encoded| { encoded.content_type=#custom.split(';').next().unwrap_or("").trim().to_ascii_lowercase(); encoded }) },
                    )
                } else if let Some(override_name) = &media_override {
                    let expected = match override_name.to_string().as_str() {
                        "Json" => "Json",
                        "Text" | "Html" | "Xml" | "Css" | "JavaScript" | "Csv" | "Markdown"
                        | "TextXml" | "Rtf" | "Yaml" | "XYaml" => "Text",
                        "Binary" | "Pdf" | "Excel" | "Spreadsheet" | "Word" | "WordDocument"
                        | "Zip" | "Jpeg" | "Png" | "Gif" | "Mp3" | "Wav" | "Mp4" => "Binary",
                        _ => {
                            return Err(syn::Error::new_spanned(
                                override_name,
                                "unsupported finite response media override",
                            ));
                        }
                    };
                    let actual = media.as_deref().unwrap_or("Json");
                    if expected != actual && !(expected == "Text" && actual == "Html") {
                        return Err(syn::Error::new_spanned(
                            override_name,
                            "media override must preserve the payload codec",
                        ));
                    }
                    (
                        quote! { (#body).with_media_type(#path::MediaType::#override_name).expect("derived compatible media") },
                        quote! { (#encode).map(|mut encoded| { encoded.content_type = #path::MediaType::#override_name.as_str().split(';').next().unwrap_or("").trim().to_ascii_lowercase(); encoded }) },
                    )
                } else {
                    (body, encode)
                };
                contracts.push(quote! { #path::ResponseContract::new(#code, Some(#body)) });
                encoders.push(quote! { Self::#name(value) => #encode });
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    variant,
                    "response variants must be unit variants or contain exactly one typed payload",
                ));
            }
        }
    }
    if contracts.is_empty() {
        return Err(syn::Error::new_spanned(
            input.ident,
            "response enum must not be empty",
        ));
    }
    let name = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #path::endpoint::ResponseVariants for #name #type_generics #where_clause {
            fn responses() -> Vec<#path::ResponseContract> { vec![#(#contracts),*] }
            fn encode(self) -> Result<#path::endpoint::EncodedResponse, #path::endpoint::ResponseEncodingError> {
                match self { #(#encoders),* }
            }
        }
    })
}
