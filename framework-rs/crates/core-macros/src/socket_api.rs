//! Complete static socket declarations without runtime coupling.
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Item, ItemMod, ReturnType, Type, Visibility, parse_macro_input};

pub fn expand(arguments: &TokenStream, input: TokenStream) -> TokenStream {
    if !arguments.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "socket_api takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    let mut module = parse_macro_input!(input as ItemMod);
    match generate(&mut module) {
        Ok(()) => quote!(#module).into(),
        Err(error) => error.to_compile_error().into(),
    }
}
fn descriptor(function: &syn::ItemFn) -> syn::Result<&Type> {
    if function.sig.asyncness.is_some()
        || matches!(function.sig.safety, syn::Safety::Unsafe(_))
        || function.sig.abi.is_some()
        || !function.sig.inputs.is_empty()
        || !function.sig.generics.params.is_empty()
        || function.sig.generics.where_clause.is_some()
        || function
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
    {
        return Err(syn::Error::new_spanned(
            &function.sig,
            "socket_api declarations must be unconditional safe non-generic zero-argument functions",
        ));
    }
    let ReturnType::Type(_, result) = &function.sig.output else {
        return Err(syn::Error::new_spanned(
            &function.sig,
            "socket declaration must return TypedSocket",
        ));
    };
    Ok(result)
}
fn generate(module: &mut ItemMod) -> syn::Result<()> {
    let Some((_, items)) = &mut module.content else {
        return Err(syn::Error::new_spanned(
            &module.ident,
            "socket_api requires an inline module",
        ));
    };
    let contract = super::typed::contract_path();
    let mut socket = contract.clone();
    socket.segments.pop();
    socket.segments.push(syn::parse_quote!(ws));
    let group_name = module.ident.to_string();
    let mut evidence = Vec::new();
    let mut declarations = Vec::new();
    let mut fields = Vec::new();
    let mut bounds = Vec::new();
    let mut parameters = Vec::new();
    let mut registrations = Vec::new();
    let mut mounts = Vec::new();
    let mut mount_bounds = Vec::new();
    for item in items.iter() {
        let Item::Fn(function) = item else { continue };
        if !matches!(function.vis, Visibility::Public(_)) {
            continue;
        }
        let output = descriptor(function)?;
        let name = &function.sig.ident;
        evidence.push(quote!(#contract::assembly::BindingDeclaration::socket(file!(), #group_name, stringify!(#name), &#name().contract())?));
        let (wire_name, tag) = identity(name)?;
        let parameter = format_ident!("Binding{}", parameters.len());
        declarations.push(quote! {
            pub struct #tag;
            impl #socket::bindings::SocketDefinition for #tag {
                type Input = <#output as #socket::bindings::SocketDescriptor>::Input;
                type Incoming = <#output as #socket::bindings::SocketDescriptor>::Incoming;
                type Outgoing = <#output as #socket::bindings::SocketDescriptor>::Outgoing;
                const NAME: &'static str = #wire_name;
                fn socket() -> #socket::TypedSocket<Self::Input,Self::Incoming,Self::Outgoing> {
                    #socket::bindings::SocketDescriptor::into_socket(#name())
                }
            }
            impl #tag {
                pub fn bind_authorized<P,C,H>(policy:P,configuration:C,handler:H) -> #socket::bindings::AuthorizedSocketBinding<Self,P,C,H> {
                    #socket::bindings::AuthorizedSocketBinding::new(policy,configuration,handler)
                }
            }
        });
        fields.push(quote!(pub #name:#parameter));
        bounds.push(quote!(#parameter:#socket::bindings::SocketOperationBinding<Definition=#tag>));
        registrations.push(quote!(registry.register_socket(#wire_name,&#name().contract()).map_err(|_|"invalid static socket registry".to_owned())?;));
        mounts.push(
            quote!(router=#contract::bindings::MountBinding::mount_binding(router,self.#name)?;),
        );
        mount_bounds.push(quote!(R:#contract::bindings::MountBinding<#parameter>));
        parameters.push(parameter);
    }
    if parameters.is_empty() {
        return Err(syn::Error::new_spanned(
            &module.ident,
            "socket_api needs at least one public socket declaration",
        ));
    }
    let generated: syn::File = syn::parse2(quote! {
        #(#declarations)*
        pub fn binding_manifest() -> Result<Vec<#contract::assembly::BindingDeclaration>, String> {
            Ok(vec![#(#evidence),*])
        }
        pub fn registry() -> Result<#contract::ContractRegistry,String> {
            let mut registry=#contract::ContractRegistry::new();
            register_into(&mut registry)?;
            Ok(registry)
        }
        pub fn register_into(registry: &mut #contract::ContractRegistry) -> Result<(),String> {
            #(#registrations)*
            Ok(())
        }
        pub struct Bindings<#(#parameters),*> where #(#bounds),* {#(#fields),*}
        impl<#(#parameters),*> Bindings<#(#parameters),*> where #(#bounds),* {
            pub fn mount<R>(self,mut router:R)->Result<R,String> where #(#mount_bounds),* {
                #(#mounts)*
                Ok(router)
            }
        }
    })?;
    items.extend(generated.items);
    Ok(())
}

fn identity(name: &syn::Ident) -> syn::Result<(String, syn::Ident)> {
    let wire_name = name.to_string().trim_start_matches("r#").to_owned();
    if wire_name.len() > 128
        || !wire_name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(syn::Error::new_spanned(
            name,
            "socket names require lowercase ASCII letters, digits or underscores (maximum128 bytes)",
        ));
    }
    let tag: String = wire_name
        .split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .unwrap()
                .to_uppercase()
                .chain(chars)
                .collect::<String>()
        })
        .collect();
    if !tag.starts_with(|character: char| character.is_ascii_alphabetic()) {
        return Err(syn::Error::new_spanned(
            name,
            "socket name requires a named identity",
        ));
    }
    let tag = format_ident!("{tag}", span = name.span());
    Ok((wire_name, tag))
}
