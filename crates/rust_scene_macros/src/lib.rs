//! Proc macros for `rust_scene`'s custom-operator layer. See
//! `crates/rust_scene/OPERATORS.md` for the spec these implement.

use proc_macro::TokenStream;

mod attr_constructor;
mod attr_operator;
mod common;
mod derive;
mod naming;

/// `#[derive(Operator)]` — form B. See `OPERATORS.md` "Authoring form B".
#[proc_macro_derive(Operator, attributes(hold))]
pub fn derive_operator(input: TokenStream) -> TokenStream {
    derive::derive_operator(input)
}

/// `#[derive(Constructor)]` — the leaf-node analog of `#[derive(Operator)]`.
#[proc_macro_derive(Constructor, attributes(hold))]
pub fn derive_constructor(input: TokenStream) -> TokenStream {
    derive::derive_constructor(input)
}

/// `#[operator]` / `#[operator(endpoints)]` — form A. See `OPERATORS.md`
/// "Authoring form A".
#[proc_macro_attribute]
pub fn operator(attr: TokenStream, item: TokenStream) -> TokenStream {
    attr_operator::operator_attr(attr, item)
}

/// `#[constructor]` — the leaf-node analog of `#[operator]`.
#[proc_macro_attribute]
pub fn constructor(attr: TokenStream, item: TokenStream) -> TokenStream {
    attr_constructor::constructor_attr(attr, item)
}
