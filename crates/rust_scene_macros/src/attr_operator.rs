//! `#[operator]` / `#[operator(endpoints)]` — form A of `OPERATORS.md`.
//! Desugars a plain function into the same shape `#[derive(Operator)]`
//! produces: a synthesized struct (one field per non-operand parameter),
//! `impl Apply` (plain) or `impl ApplyEndpoints` (`endpoints`), and the
//! generated `OperatorNode` impl + chaining trait from `common.rs`.
//!
//! ## No default arguments
//!
//! Every parameter is required and positional; `#[default(..)]` is rejected.
//! Rust has no optional parameters and no overloading, so each way of faking
//! them costs something un-Rust-like: two same-named trait methods of
//! differing arity are an unconditional E0034 "multiple applicable items"
//! error whenever both are in scope (arity is never used to disambiguate
//! candidates), an args-tuple `impl` forces `.wobble((0.2, 3.0))`, and a
//! separately-named short function gives one operation two names. Callers who
//! want defaults use form B's struct literal —
//! `mesh.with(Wobble { amount: 0.2, ..Default::default() })` — which is the
//! idiomatic Rust spelling and needs no macro support at all.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ItemFn, Pat, Type, parse_macro_input};

use crate::common::{
    FieldSpec, chaining_ext, check_name_collision, operator_node_impl, reject_default_attr,
};
use crate::naming::to_pascal_case;

pub fn operator_attr(attr: TokenStream, item: TokenStream) -> TokenStream {
    let use_endpoints = match parse_mode(attr) {
        Ok(v) => v,
        Err(err) => return err.into_compile_error().into(),
    };
    let func = parse_macro_input!(item as ItemFn);
    match expand(use_endpoints, func) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn parse_mode(attr: TokenStream) -> syn::Result<bool> {
    if attr.is_empty() {
        return Ok(false);
    }
    let ident: Ident = syn::parse(attr)?;
    if ident == "endpoints" {
        Ok(true)
    } else {
        Err(syn::Error::new(
            ident.span(),
            "expected `#[operator]` or `#[operator(endpoints)]`",
        ))
    }
}

fn expand(use_endpoints: bool, func: ItemFn) -> syn::Result<TokenStream2> {
    let vis = func.vis.clone();
    let fn_ident = func.sig.ident.clone();
    let struct_ident = format_ident!("{}", to_pascal_case(&fn_ident.to_string()));

    if let Some(err) = check_name_collision(&fn_ident.to_string(), fn_ident.span()) {
        return Ok(err);
    }

    let mut inputs = func.sig.inputs.iter();
    let Some(FnArg::Typed(target_arg)) = inputs.next() else {
        return Err(syn::Error::new_spanned(
            &func.sig,
            "operators need a `target: MeshValue` first argument",
        ));
    };
    let target_pat = (*target_arg.pat).clone();
    let target_ty = (*target_arg.ty).clone();

    let mut fields = Vec::new();
    for arg in inputs {
        let FnArg::Typed(typed) = arg else {
            return Err(syn::Error::new_spanned(
                arg,
                "operators do not take a `self` parameter",
            ));
        };
        let Pat::Ident(pat_ident) = typed.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "operator arguments must be simple identifiers",
            ));
        };
        reject_default_attr(&typed.attrs)?;
        fields.push(FieldSpec {
            ident: pat_ident.ident.clone(),
            ty: (*typed.ty).clone(),
            hold: false,
        });
    }

    let inner_fn = build_inner_fn(&fn_ident, &target_pat, &target_ty, &fields, &func);
    let call_args = fields.iter().map(|field| {
        let ident = &field.ident;
        quote! { ::std::clone::Clone::clone(&self.#ident) }
    });
    let inner_ident = format_ident!("__{}_body", fn_ident);

    let body_impl = if use_endpoints {
        quote! {
            impl ::rust_scene::ApplyEndpoints for #struct_ident {
                fn endpoints(&self, target: ::rust_scene::MeshValue) -> (::rust_scene::MeshValue, ::rust_scene::MeshValue) {
                    #inner_fn
                    #inner_ident(target, #(#call_args),*)
                }
            }
        }
    } else {
        quote! {
            impl ::rust_scene::Apply for #struct_ident {
                fn apply(&self, target: ::rust_scene::MeshValue) -> ::rust_scene::MeshValue {
                    #inner_fn
                    #inner_ident(target, #(#call_args),*)
                }
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

    let node_impl = operator_node_impl(&struct_ident, &fields);
    let trait_ident = format_ident!("{}ChainExt", struct_ident);
    let chain = chaining_ext(&struct_ident, &trait_ident, &fn_ident, &fields, &vis);

    Ok(quote! {
        #struct_def
        #body_impl
        #node_impl
        #chain
    })
}

fn build_inner_fn(
    fn_ident: &Ident,
    target_pat: &Pat,
    target_ty: &Type,
    fields: &[FieldSpec],
    func: &ItemFn,
) -> TokenStream2 {
    let inner_ident = format_ident!("__{}_body", fn_ident);
    let output = &func.sig.output;
    let body = &func.block;
    let param_idents = fields.iter().map(|field| &field.ident);
    let param_tys = fields.iter().map(|field| &field.ty);

    quote! {
        fn #inner_ident(#target_pat: #target_ty, #(#param_idents: #param_tys),*) #output {
            #body
        }
    }
}
