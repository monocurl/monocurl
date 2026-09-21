//! `#[constructor]` — the leaf-node analog of `#[operator]`: same slot
//! machinery, no operand, `Construct` in place of `Apply`/`ApplyEndpoints`,
//! and a plain free function as the entry point instead of a chaining method.
//! Like `#[operator]`, it takes no default arguments — see that module's
//! "No default arguments" note.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, Pat, parse_macro_input};

use crate::common::{FieldSpec, check_name_collision, constructor_node_impl, reject_default_attr};
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

fn expand(func: ItemFn) -> syn::Result<TokenStream2> {
    let vis = func.vis.clone();
    let fn_ident = func.sig.ident.clone();
    let struct_ident = format_ident!("{}", to_pascal_case(&fn_ident.to_string()));

    if let Some(err) = check_name_collision(&fn_ident.to_string(), fn_ident.span()) {
        return Ok(err);
    }

    let mut fields = Vec::new();
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
        reject_default_attr(&typed.attrs)?;
        fields.push(FieldSpec {
            ident: pat_ident.ident.clone(),
            ty: (*typed.ty).clone(),
            hold: false,
        });
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

    Ok(quote! {
        #struct_def
        #construct_impl
        #node_impl
        #full_fn
    })
}
