//! Host extraction of an explicit contract module; handler code never enters the host.
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Ident, Item, LitStr, Token,
    parse::{Parse, ParseStream},
};
struct Export {
    name: Ident,
    source: LitStr,
}
impl Parse for Export {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![,]>()?;
        Ok(Self {
            name,
            source: input.parse()?,
        })
    }
}
pub fn expand(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as Export);
    generate(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
fn generate(input: &Export) -> syn::Result<proc_macro2::TokenStream> {
    let fail = |message: String| syn::Error::new(input.source.span(), message);
    let directory = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").map_err(|error| fail(error.to_string()))?,
    );
    let allowed = if directory.ends_with(".forklaunch/contracts") {
        directory
            .parent()
            .and_then(std::path::Path::parent)
            .unwrap()
    } else {
        &directory
    };
    let path = directory
        .join(input.source.value())
        .canonicalize()
        .map_err(|error| fail(error.to_string()))?;
    let allowed = allowed
        .canonicalize()
        .map_err(|error| fail(error.to_string()))?;
    if !path.starts_with(&allowed) {
        return Err(fail(
            "Controller source must remain inside its owning service".into(),
        ));
    }
    let metadata = std::fs::metadata(&path).map_err(|error| fail(error.to_string()))?;
    if metadata.len() > 2 * 1024 * 1024 {
        return Err(fail("Controller source exceeds 2 MiB".into()));
    }
    let text = std::fs::read_to_string(&path).map_err(|error| fail(error.to_string()))?;
    let file = syn::parse_file(&text)?;
    let mut contracts = file.items.into_iter().filter_map(|item| match item {
        Item::Mod(module) if module.ident == "contract" => Some(module),
        _ => None,
    });
    let mut contract = contracts
        .next()
        .ok_or_else(|| fail("Controller requires an inline #[api] pub mod contract".into()))?;
    if contracts.next().is_some()
        || contract.content.is_none()
        || !matches!(contract.vis, syn::Visibility::Public(_))
    {
        return Err(fail(
            "Controller requires exactly one public inline contract module".into(),
        ));
    }
    if contract
        .attrs
        .iter()
        .any(|attribute| attribute.path().is_ident("cfg"))
    {
        return Err(fail(
            "Conditional controller contracts require an explicit supported profile".into(),
        ));
    }
    let api = contract
        .attrs
        .iter_mut()
        .find(|attribute| {
            attribute
                .path()
                .segments
                .last()
                .is_some_and(|part| part.ident == "api")
        })
        .ok_or_else(|| fail("Controller contract module requires #[api]".into()))?;
    let source = LitStr::new(
        path.strip_prefix(&allowed)
            .map_err(|error| fail(error.to_string()))?
            .to_str()
            .ok_or_else(|| fail("Controller source must be UTF-8".into()))?,
        input.source.span(),
    );
    let contract_path = super::typed::contract_path();
    *api = syn::parse_quote!(#[#contract_path::api(source = #source)]);
    let name = &input.name;
    let include = &input.source;
    Ok(quote! {
        pub mod #name {
            // Track controller contents while keeping binding provenance relative
            // to its owner, so cached hosts remain valid after workspace relocation.
            const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #include));
            #contract
        }
    })
}
