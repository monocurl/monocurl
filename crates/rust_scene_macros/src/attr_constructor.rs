//! `#[constructor]` — the leaf-node analog of `#[operator]`: same slot/default
//! machinery, no operand, `Construct` in place of `Apply`/`ApplyEndpoints`,
//! and a plain free function as the entry point instead of a chaining method.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Attribute, Expr, FnArg, ItemFn, Pat, parse_macro_input};

use crate::common::{FieldSpec, check_name_collision, constructor_node_impl};
use crate::naming::to_pascal_case;

pub fn constructor_attr(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[constructor] does not take arguments",
        )
        .into_compile_error()
        .into();
    }
    let func = parse_macro_input!(item as ItemFn);
    match expand(func) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn extract_default_attr(attrs: &[Attribute]) -> syn::Result<Option<Expr>> {
    for attr in attrs {
        if attr.path().is_ident("default") {
            return Ok(Some(attr.parse_args()?));
        }
    }
    Ok(None)
}

fn expand(func: ItemFn) -> syn::Result<TokenStream2> {
    let vis = func.vis.clone();
    let fn_ident = func.sig.ident.clone();
    let struct_ident = format_ident!("{}", to_pascal_case(&fn_ident.to_string()));

    if let Some(err) = check_name_collision(&fn_ident.to_string(), fn_ident.span()) {
        return Ok(err);
    }

    let mut fields = Vec::new();
    let mut defaults: Vec<Option<Expr>> = Vec::new();
    for arg in &func.sig.inputs {
        let FnArg::Typed(typed) = arg else {
            return Err(syn::Error::new_spanned(
                arg,
                "constructors do not take a `self` parameter",
            ));
        };
        let Pat::Ident(pat_ident) = typed.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "constructor arguments must be simple identifiers",
            ));
        };
        let default_expr = extract_default_attr(&typed.attrs)?;
        fields.push(FieldSpec {
            ident: pat_ident.ident.clone(),
            ty: (*typed.ty).clone(),
            hold: false,
        });
        defaults.push(default_expr);
    }

    let first_default = defaults.iter().position(Option::is_some);
    if let Some(first) = first_default
        && defaults[first..].iter().any(Option::is_none)
    {
        return Err(syn::Error::new_spanned(
            &func.sig,
            "#[default(..)] parameters must be trailing (no required parameter after a defaulted one)",
        ));
    }

    let inner_ident = format_ident!("__{}_body", fn_ident);
    let output = &func.sig.output;
    let body = &func.block;
    let param_idents: Vec<_> = fields.iter().map(|field| &field.ident).collect();
    let param_tys: Vec<_> = fields.iter().map(|field| &field.ty).collect();
    let inner_fn = quote! {
        fn #inner_ident(#(#param_idents: #param_tys),*) #output {
            #body
        }
    };
    let call_args = fields.iter().map(|field| {
        let ident = &field.ident;
        quote! { ::std::clone::Clone::clone(&self.#ident) }
    });

    let construct_impl = quote! {
        impl ::rust_scene::Construct for #struct_ident {
            fn construct(&self) -> ::rust_scene::MeshValue {
                #inner_fn
                #inner_ident(#(#call_args),*)
            }
        }
    };

    let field_decls = fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        quote! { pub #ident: #ty }
    });
    let struct_def = quote! {
        #[derive(Clone)]
        #vis struct #struct_ident { #(#field_decls),* }
    };

    let node_impl = constructor_node_impl(&struct_ident, &fields);

    let full_params = fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        quote! { #ident: #ty }
    });
    let full_inits = fields.iter().map(|field| {
        let ident = &field.ident;
        quote! { #ident }
    });
    let full_fn = quote! {
        #vis fn #fn_ident(#(#full_params),*) -> ::rust_scene::MeshValue {
            ::rust_scene::MeshValue::from_constructor_node(::std::boxed::Box::new(#struct_ident { #(#full_inits),* }))
        }
    };

    // Defaulted arity: a second, distinctly-named free function (free
    // functions, unlike trait methods, don't hit the E0034 ambiguity
    // described in `attr_operator.rs` — but they do still need a different
    // *name*, since plain function overloading isn't possible either).
    let defaults_fn = match first_default {
        Some(split) => {
            let required = &fields[..split];
            let required_params = required.iter().map(|field| {
                let ident = &field.ident;
                let ty = &field.ty;
                quote! { #ident: #ty }
            });
            let required_inits = required.iter().map(|field| {
                let ident = &field.ident;
                quote! { #ident }
            });
            let defaulted_inits =
                fields[split..]
                    .iter()
                    .zip(&defaults[split..])
                    .map(|(field, expr)| {
                        let ident = &field.ident;
                        let expr = expr.as_ref().expect("validated trailing defaults");
                        quote! { #ident: (#expr) }
                    });
            let short_ident = format_ident!("{}_defaults", fn_ident);
            quote! {
                #vis fn #short_ident(#(#required_params),*) -> ::rust_scene::MeshValue {
                    ::rust_scene::MeshValue::from_constructor_node(::std::boxed::Box::new(#struct_ident {
                        #(#required_inits,)*
                        #(#defaulted_inits,)*
                    }))
                }
            }
        }
        None => TokenStream2::new(),
    };

    Ok(quote! {
        #struct_def
        #construct_impl
        #node_impl
        #full_fn
        #defaults_fn
    })
}
