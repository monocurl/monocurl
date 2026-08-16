//! `#[operator]` / `#[operator(endpoints)]` — form A of `OPERATORS.md`.
//! Desugars a plain function into the same shape `#[derive(Operator)]`
//! produces: a synthesized struct (one field per non-operand parameter),
//! `impl Apply` (plain) or `impl ApplyEndpoints` (`endpoints`), and the
//! generated `OperatorNode` impl + chaining trait from `common.rs`.
//!
//! ## `#[default(expr)]` arity
//!
//! `OPERATORS.md` asks for both `wobble(amount)` and `wobble(amount,
//! frequency)` to compile as chained calls with the *same* method name. That
//! specific pair of call spellings is not achievable in stable Rust: Rust has
//! no argument-count overloading, and (verified empirically while
//! implementing this) two trait methods of the same name and different arity
//! visible in the same scope produce an unconditional "multiple applicable
//! items" error (E0034) *regardless* of the arg count at the call site —
//! arity is never used to disambiguate candidates; the same is true for an
//! inherent-vs-trait-method pair (the inherent method wins outright and a
//! mismatched call reports "wrong number of arguments" rather than falling
//! back to the trait method).
//!
//! Given the spec's own fallback ("implemented as one method per arity ... or
//! via a small generated args-tuple impl; pick whichever is simpler"), this
//! implements the closest compiling approximation: **two separate extension
//! traits**, `#[default(..)]`-free
//! `{Struct}ChainExt::{name}(self, <all fields>)` and, only when there are
//! defaulted fields, `{Struct}DefaultsChainExt::{name}(self, <required
//! fields>)` (defaulted fields filled from their `#[default(..)]`
//! expressions). Each compiles and is callable with the exact bare syntax
//! shown in the spec — but only one of the two traits can be imported into a
//! given scope at a time (the standard extension-trait limitation); importing
//! both produces the same E0034 ambiguity described above. This is a
//! documented deviation from the literal "both spellings above must compile"
//! wording, not a silent one.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Attribute, Expr, FnArg, Ident, ItemFn, Pat, Type, parse_macro_input};

use crate::common::{
    FieldSpec, chaining_ext, chaining_ext_with_defaults, check_name_collision, operator_node_impl,
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

fn extract_default_attr(attrs: &[Attribute]) -> syn::Result<Option<Expr>> {
    for attr in attrs {
        if attr.path().is_ident("default") {
            return Ok(Some(attr.parse_args()?));
        }
    }
    Ok(None)
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
    let mut defaults: Vec<Option<Expr>> = Vec::new();
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

    let defaults_chain = match first_default {
        Some(split) => {
            let required = fields[..split].to_vec();
            let defaulted: Vec<(FieldSpec, TokenStream2)> = fields[split..]
                .iter()
                .cloned()
                .zip(defaults[split..].iter().map(|expr| {
                    let expr = expr.as_ref().expect("validated trailing defaults");
                    quote! { #expr }
                }))
                .collect();
            let defaults_trait_ident = format_ident!("{}DefaultsChainExt", struct_ident);
            chaining_ext_with_defaults(
                &struct_ident,
                &defaults_trait_ident,
                &fn_ident,
                &required,
                &defaulted,
                &vis,
            )
        }
        None => TokenStream2::new(),
    };

    Ok(quote! {
        #struct_def
        #body_impl
        #node_impl
        #chain
        #defaults_chain
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
