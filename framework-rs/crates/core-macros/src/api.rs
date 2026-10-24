//! Static operation sets without a runtime dependency in the contract package.
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Item, ItemMod, ReturnType, Type, Visibility, parse_macro_input};

pub fn expand(arguments: &TokenStream, input: TokenStream) -> TokenStream {
    let source = if arguments.is_empty() {
        None
    } else {
        match syn::parse::<syn::MetaNameValue>(arguments.clone()) {
            Ok(value) if value.path.is_ident("source") => match value.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(value),
                    ..
                }) => Some(value),
                other => {
                    return syn::Error::new_spanned(other, "api source requires a string literal")
                        .to_compile_error()
                        .into();
                }
            },
            _ => {
                return syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "api accepts only the generated source metadata option",
                )
                .to_compile_error()
                .into();
            }
        }
    };
    let mut module = parse_macro_input!(input as ItemMod);
    match generate(&mut module, source.as_ref()) {
        Ok(()) => quote!(#module).into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn generate(module: &mut ItemMod, source: Option<&syn::LitStr>) -> syn::Result<()> {
    let source = source.map_or_else(|| quote!(file!()), |source| quote!(#source));
    let Some((_, items)) = &mut module.content else {
        return Err(syn::Error::new_spanned(
            &module.ident,
            "api requires an inline module; contract collection never reads source files",
        ));
    };
    let path = super::typed::contract_path();
    let group_name = module.ident.to_string();
    let mut evidence = Vec::new();
    let mut declarations = Vec::new();
    let mut fields = Vec::new();
    let mut bounds = Vec::new();
    let mut parameters = Vec::new();
    let mut register = Vec::new();
    let mut mount = Vec::new();
    let mut mount_bounds = Vec::new();
    let mut shapes = Vec::new();
    let occupied = module_type_names(items);
    for item in items.iter_mut() {
        let Item::Fn(function) = item else { continue };
        if !matches!(function.vis, Visibility::Public(_)) {
            continue;
        }
        let (public, policy_bound) = apply_access(function, &path)?;
        let name = &function.sig.ident;
        evidence.push(quote!(#path::assembly::BindingDeclaration::http(#source, #group_name, stringify!(#name), &#name().contract())?));
        let descriptor = operation_type(function)?;
        let tag = operation_tag(name)?;
        reject_collision(&occupied, &tag, name)?;
        let parameter = format_ident!("Binding{}", parameters.len());
        let public_constructor = public_constructor(public, &tag, &path);
        declarations.push(quote! {
            pub struct #tag;
            impl #path::bindings::EndpointDefinition for #tag {
                type Input = <#descriptor as #path::bindings::EndpointDescriptor>::Input;
                type Output = <#descriptor as #path::bindings::EndpointDescriptor>::Output;
                fn endpoint() -> #path::endpoint::Endpoint<Self::Input, Self::Output> { #path::bindings::EndpointDescriptor::into_endpoint(#name()) }
            }
            impl<P #policy_bound> #path::bindings::EndpointPolicyFor<P> for #tag {}
            #public_constructor
            impl #tag {
                pub fn bind_sse_authorized<P, C, H>(policy: P, configuration: C, handler: H) -> #path::bindings::SseBinding<Self, P, C, H> where Self: #path::bindings::EndpointPolicyFor<P> {
                    #path::bindings::SseBinding::new(policy, configuration, handler)
                }
                pub fn bind_sse_authorized_with_headers<P, C, H>(policy: P, configuration: C, handler: H) -> #path::bindings::SseBinding<Self, P, C, H, #path::bindings::SseHeaders> where Self: #path::bindings::EndpointPolicyFor<P> {
                    #path::bindings::SseBinding::new(policy, configuration, handler)
                }
                pub fn bind_authorized<P, H>(policy: P, handler: H) -> #path::bindings::AuthorizedBinding<Self, P, H> where Self: #path::bindings::EndpointPolicyFor<P> {
                    #path::bindings::AuthorizedBinding::new(policy, handler)
                }
            }
        });
        fields.push(quote!(pub #name: #parameter));
        bounds.push(quote!(#parameter: #path::bindings::EndpointBinding<Definition = #tag>));
        register.push(quote!(registry.register(#name().contract());));
        let shape = format_ident!("HandlerShape{}", shapes.len());
        mount_bounds.push(quote!(R: #path::bindings::MountBinding<#parameter, #shape>));
        shapes.push(shape);
        mount.push(
            quote!(router = #path::bindings::MountBinding::mount_binding(router, self.#name)?;),
        );
        parameters.push(parameter);
    }
    if parameters.is_empty() {
        return Err(syn::Error::new_spanned(
            &module.ident,
            "api needs at least one public endpoint declaration",
        ));
    }
    let generated = quote! {
        #(#declarations)*
        pub fn binding_manifest() -> Result<Vec<#path::assembly::BindingDeclaration>, String> {
            Ok(vec![#(#evidence),*])
        }
        pub fn registry() -> #path::ContractRegistry {
            let mut registry = #path::ContractRegistry::new();
            #(#register)*
            registry
        }
        pub struct Bindings<#(#parameters),*> where #(#bounds),* { #(#fields),* }
        impl<#(#parameters),*> Bindings<#(#parameters),*> where #(#bounds),* {
            pub fn mount<R, #(#shapes),*>(self, mut router: R) -> Result<R, String> where #(#mount_bounds),* {
                #(#mount)*
                Ok(router)
            }
        }
    };
    let generated: syn::File = syn::parse2(generated)?;
    items.extend(generated.items);
    Ok(())
}

/// The generated operation type must not shadow a DTO already named in the module.
fn reject_collision(
    occupied: &[syn::Ident],
    tag: &syn::Ident,
    name: &syn::Ident,
) -> syn::Result<()> {
    let Some(existing) = occupied.iter().find(|existing| *existing == tag) else {
        return Ok(());
    };
    let mut error = syn::Error::new_spanned(
        name,
        format!(
            "#[contract::api] generates the operation type `{tag}` for endpoint `{name}`, \
             which collides with `{existing}` already named in this module; refer to the \
             DTO through its module path (e.g. `dto::{existing}`) or import it under \
             another name (`use ...::{existing} as {existing}Body`)"
        ),
    );
    error.combine(syn::Error::new_spanned(
        existing,
        format!("`{existing}` is named here; #[contract::api] also defines `{tag}`"),
    ));
    Err(error)
}

/// Type names a module item already binds (explicit imports and local types).
/// Glob imports are shadowed by the generated operation types, so they never collide.
fn module_type_names(items: &[Item]) -> Vec<syn::Ident> {
    fn tree(use_tree: &syn::UseTree, names: &mut Vec<syn::Ident>) {
        match use_tree {
            syn::UseTree::Path(path) => tree(&path.tree, names),
            syn::UseTree::Name(name) if name.ident != "self" => names.push(name.ident.clone()),
            syn::UseTree::Rename(rename) => names.push(rename.rename.clone()),
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    tree(item, names);
                }
            }
            _ => {}
        }
    }
    let mut names = Vec::new();
    for item in items {
        match item {
            Item::Use(item) => tree(&item.tree, &mut names),
            Item::Struct(item) => names.push(item.ident.clone()),
            Item::Enum(item) => names.push(item.ident.clone()),
            Item::Type(item) => names.push(item.ident.clone()),
            Item::Union(item) => names.push(item.ident.clone()),
            Item::Trait(item) => names.push(item.ident.clone()),
            _ => {}
        }
    }
    names
}

fn operation_type(function: &syn::ItemFn) -> syn::Result<&Type> {
    if function.sig.asyncness.is_some()
        || matches!(function.sig.safety, syn::Safety::Unsafe(_))
        || function.sig.abi.is_some()
        || !function.sig.inputs.is_empty()
        || !function.sig.generics.params.is_empty()
        || function.sig.generics.where_clause.is_some()
        || function.attrs.iter().any(|attribute| {
            attribute.path().is_ident("cfg") || attribute.path().is_ident("cfg_attr")
        })
    {
        return Err(syn::Error::new_spanned(
            &function.sig,
            "api operations must be unconditional, safe, non-generic zero-argument endpoint declarations; use a private function for helpers",
        ));
    }
    let ReturnType::Type(_, output) = &function.sig.output else {
        return Err(syn::Error::new_spanned(
            &function.sig,
            "api operation must return Endpoint<Input, Output>",
        ));
    };
    Ok(output.as_ref())
}

// Access annotations alter the declaration itself, so host registry and runtime
// binding admission share one policy level. Unannotated legacy declarations
// retain runtime access checks and the public constructor for compatibility.
fn apply_access(
    function: &mut syn::ItemFn,
    path: &syn::Path,
) -> syn::Result<(bool, proc_macro2::TokenStream)> {
    let mut access = None;
    for attribute in &function.attrs {
        if attribute.path().is_ident("access") {
            if access.is_some() {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "duplicate access declaration",
                ));
            }
            let level: syn::Ident = attribute.parse_args()?;
            if !matches!(
                level.to_string().as_str(),
                "Public" | "Authenticated" | "Protected" | "Internal"
            ) {
                return Err(syn::Error::new_spanned(
                    level,
                    "expected Public, Authenticated, Protected or Internal",
                ));
            }
            access = Some(level);
        }
    }
    let Some(level) = access else {
        return Ok((true, quote!()));
    };
    function
        .attrs
        .retain(|attribute| !attribute.path().is_ident("access"));
    let block = &function.block;
    *function.block = syn::parse2(quote!({
        let endpoint = #block;
        endpoint.with_access(#path::AccessLevel::#level)
    }))?;
    let bound = match level.to_string().as_str() {
        "Internal" => quote!(: #path::bindings::OriginalBodySignedPolicy),
        "Protected" => quote!(: #path::bindings::AuthorizationPolicy),
        _ => quote!(),
    };
    Ok((level == "Public", bound))
}

fn public_constructor(
    public: bool,
    tag: &syn::Ident,
    path: &syn::Path,
) -> Option<proc_macro2::TokenStream> {
    public.then(|| {
        quote! {
            impl #path::bindings::PublicEndpointDefinition for #tag {}
            impl #tag {
                pub fn bind<H>(handler: H) -> #path::bindings::PublicBinding<Self, H> {
                    #path::bindings::PublicBinding::new(handler)
                }
            }
        }
    })
}

fn operation_tag(name: &syn::Ident) -> syn::Result<syn::Ident> {
    let mut tag = String::new();
    for word in name
        .to_string()
        .trim_start_matches("r#")
        .split('_')
        .filter(|word| !word.is_empty())
    {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            tag.extend(first.to_uppercase());
            tag.extend(chars);
        }
    }
    if tag.is_empty() {
        return Err(syn::Error::new_spanned(
            name,
            "operation name needs a named identity",
        ));
    }
    Ok(format_ident!("{tag}", span = name.span()))
}
