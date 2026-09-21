//! `#[derive(Operator)]` and `#[derive(Constructor)]` — form B of
//! `OPERATORS.md`. Users implement `Apply`/`ApplyEndpoints` (operator) or
//! `Construct` (constructor) on their struct by hand; the derive generates
//! the object-safe `OperatorNode`/`ConstructorNode` impl plus the chaining
//! extension trait / free function.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Ident, parse_macro_input};

use crate::common::{
    chaining_ext, check_name_collision, constructor_node_impl, extract_named_fields,
    operator_node_impl, snake_method_name,
};

fn hold_field_idents(input: &DeriveInput) -> syn::Result<Vec<Ident>> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "this derive only supports structs",
        ));
    };
    let syn::Fields::Named(named) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &data.fields,
            "this derive only supports structs with named fields",
        ));
    };
    let mut holds = Vec::new();
    for field in &named.named {
        if field.attrs.iter().any(|attr| attr.path().is_ident("hold")) {
            holds.push(field.ident.clone().expect("named field"));
        }
    }
    Ok(holds)
}

pub fn derive_operator(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_operator_impl(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn derive_operator_impl(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let struct_ident = &input.ident;
    let vis = &input.vis;

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(Operator)] only supports structs",
        ));
    };

    let holds = hold_field_idents(input)?;
    let fields = extract_named_fields(&data.fields, &holds)?;

    let method_name = snake_method_name(struct_ident);
    if let Some(err) = check_name_collision(&method_name.to_string(), struct_ident.span()) {
        return Ok(err);
    }

    let trait_ident = format_ident!("{}ChainExt", struct_ident);
    let node_impl = operator_node_impl(struct_ident, &fields);
    let chain = chaining_ext(struct_ident, &trait_ident, &method_name, &fields, vis);

    Ok(quote! {
        #node_impl
        #chain
    })
}

pub fn derive_constructor(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_constructor_impl(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn derive_constructor_impl(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let struct_ident = &input.ident;
    let vis = &input.vis;

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(Constructor)] only supports structs",
        ));
    };

    let holds = hold_field_idents(input)?;
    let fields = extract_named_fields(&data.fields, &holds)?;

    let fn_name = snake_method_name(struct_ident);
    if let Some(err) = check_name_collision(&fn_name.to_string(), struct_ident.span()) {
        return Ok(err);
    }

    let node_impl = constructor_node_impl(struct_ident, &fields);
    let params = fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        quote! { #ident: #ty }
    });
    let field_inits = fields.iter().map(|field| {
        let ident = &field.ident;
        quote! { #ident }
    });

    Ok(quote! {
        #node_impl

        #vis fn #fn_name(#(#params),*) -> ::rust_scene::MeshValue {
            ::rust_scene::MeshValue::from_constructor_node(::std::boxed::Box::new(#struct_ident { #(#field_inits),* }))
        }
    })
}
