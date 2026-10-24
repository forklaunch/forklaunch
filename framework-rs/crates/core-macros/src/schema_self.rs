//! Resolve schema-mirror Self references to the user's actual DTO.
use quote::ToTokens as _;
use syn::{
    parse::Parser as _,
    punctuated::Punctuated,
    visit_mut::{self, VisitMut},
};

pub(super) fn rewrite(input: &mut syn::DeriveInput, original: syn::Path) -> syn::Result<()> {
    let mut rewrite = Rewrite {
        original,
        error: None,
    };
    rewrite.visit_derive_input_mut(input);
    rewrite.error.map_or(Ok(()), Err)
}
struct Rewrite {
    original: syn::Path,
    error: Option<syn::Error>,
}
impl VisitMut for Rewrite {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        visit_mut::visit_path_mut(self, path);
        if path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "Self")
        {
            let mut replacement = self.original.clone();
            replacement
                .segments
                .extend(path.segments.iter().skip(1).cloned());
            *path = replacement;
        }
    }
    fn visit_attribute_mut(&mut self, attribute: &mut syn::Attribute) {
        // These are generated schema type references, not arbitrary user strings.
        if !attribute.path().is_ident("schemars") {
            return;
        }
        let syn::Meta::List(list) = &mut attribute.meta else {
            return;
        };
        let result = (|| -> syn::Result<()> {
            let mut items = Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                .parse2(list.tokens.clone())?;
            for item in &mut items {
                if let syn::Meta::NameValue(value) = item
                    && value.path.is_ident("with")
                    && let syn::Expr::Lit(expression) = &mut value.value
                    && let syn::Lit::Str(literal) = &mut expression.lit
                {
                    let mut ty: syn::Type = syn::parse_str(&literal.value())?;
                    self.visit_type_mut(&mut ty);
                    *literal = syn::LitStr::new(&ty.to_token_stream().to_string(), literal.span());
                }
            }
            list.tokens = items.to_token_stream();
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error);
        }
    }
}
