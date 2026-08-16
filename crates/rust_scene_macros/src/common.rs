//! Shared codegen for `#[derive(Operator)]`, `#[operator]`/`#[operator(endpoints)]`,
//! and `#[constructor]` — all four target the same `OperatorNode`/`ConstructorNode`
//! shape, so the field/slot machinery lives here once.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Fields, Ident, Type, Visibility};

use crate::naming::to_snake_case;

#[derive(Clone)]
pub struct FieldSpec {
    pub ident: Ident,
    pub ty: Type,
    pub hold: bool,
}

/// Method names already defined on `MeshValue` (inherently or via `Keyed`)
/// that a generated chaining method must not shadow. `#[derive(Operator)]`
/// name collisions are checked against this list at macro-expansion time,
/// since Rust's own method resolution won't reliably catch the collision
/// itself (inherent methods silently win over trait methods rather than
/// erroring).
const RESERVED_MESH_VALUE_METHODS: &[&str] = &[
    "with",
    "map_leaves",
    "evaluate",
    "lerp",
    "clone",
    "leaf",
    "group",
    "fmt",
    "eq",
    "ne",
];

pub fn check_name_collision(method_name: &str, span: Span) -> Option<TokenStream> {
    if RESERVED_MESH_VALUE_METHODS.contains(&method_name) {
        let msg = format!(
            "operator/constructor method name `{method_name}` conflicts with a built-in \
             `MeshValue` method; rename the struct or function so its snake_case name does \
             not collide"
        );
        Some(quote::quote_spanned! { span => compile_error!(#msg); })
    } else {
        None
    }
}

pub fn extract_named_fields(fields: &Fields, hold_fields: &[Ident]) -> syn::Result<Vec<FieldSpec>> {
    let Fields::Named(named) = fields else {
        return Err(syn::Error::new_spanned(
            fields,
            "#[derive(Operator)] / #[derive(Constructor)] only support structs with named fields",
        ));
    };
    named
        .named
        .iter()
        .map(|field| {
            let ident = field.ident.clone().expect("named field has an ident");
            let hold = hold_fields.contains(&ident);
            Ok(FieldSpec {
                ident,
                ty: field.ty.clone(),
                hold,
            })
        })
        .collect()
}

pub fn snake_method_name(struct_ident: &Ident) -> Ident {
    format_ident!("{}", to_snake_case(&struct_ident.to_string()))
}

/// `impl OperatorNode for #struct_ident`, dispatching `endpoints` through the
/// `Apply`/`ApplyEndpoints` autoref trick in `rust_scene::__macro_support`
/// (see that module's doc comment for why it isn't a plain blanket impl).
pub fn operator_node_impl(struct_ident: &Ident, fields: &[FieldSpec]) -> TokenStream {
    let debug_name = struct_ident.to_string();
    let slot_count = fields.len();

    let slot_arms = fields.iter().enumerate().map(|(index, field)| {
        let ident = &field.ident;
        quote! { #index => &self.#ident, }
    });
    let slot_mut_arms = fields.iter().enumerate().map(|(index, field)| {
        let ident = &field.ident;
        quote! { #index => &mut self.#ident, }
    });
    let lerp_fields = fields.iter().enumerate().map(|(index, field)| {
        let ident = &field.ident;
        let name = ident.to_string();
        if field.hold {
            quote! {
                #ident: ::rust_scene::__macro_support::hold_field_lerp(
                    #debug_name, #index, #name, &self.#ident, &other.#ident,
                )?,
            }
        } else {
            quote! {
                #ident: ::rust_scene::Keyed::lerp(&self.#ident, &other.#ident, t)
                    .map_err(|err| ::rust_scene::__macro_support::slot_error(#debug_name, #index, #name, err))?,
            }
        }
    });

    quote! {
        impl ::rust_scene::OperatorNode for #struct_ident {
            fn op_id(&self) -> ::rust_scene::OpId {
                ::rust_scene::OpId::of::<#struct_ident>()
            }

            fn endpoints(&self, target: ::rust_scene::MeshValue) -> (::rust_scene::MeshValue, ::rust_scene::MeshValue) {
                #[allow(unused_imports)]
                use ::rust_scene::__macro_support::{ViaApply, ViaApplyEndpoints};
                (&::rust_scene::__macro_support::Probe(self)).dispatch_endpoints(target)
            }

            fn lerp_slots(
                &self,
                other: &dyn ::rust_scene::OperatorNode,
                t: f64,
            ) -> ::rust_scene::Result<::std::boxed::Box<dyn ::rust_scene::OperatorNode>> {
                let other = ::rust_scene::__macro_support::downcast_operator::<#struct_ident>(other, #debug_name)?;
                Ok(::std::boxed::Box::new(#struct_ident { #(#lerp_fields)* }))
            }

            fn slot(&self, index: usize) -> &dyn ::rust_scene::Slot {
                match index {
                    #(#slot_arms)*
                    _ => panic!("slot index {} out of range for `{}` ({} slots)", index, #debug_name, #slot_count),
                }
            }

            fn slot_mut(&mut self, index: usize) -> &mut dyn ::rust_scene::Slot {
                match index {
                    #(#slot_mut_arms)*
                    _ => panic!("slot index {} out of range for `{}` ({} slots)", index, #debug_name, #slot_count),
                }
            }

            fn slot_count(&self) -> usize {
                #slot_count
            }

            fn clone_node(&self) -> ::std::boxed::Box<dyn ::rust_scene::OperatorNode> {
                ::std::boxed::Box::new(::std::clone::Clone::clone(self))
            }

            fn debug_name(&self) -> &'static str {
                #debug_name
            }

            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }
        }
    }
}

/// `impl ConstructorNode for #struct_ident`, dispatching `evaluate` straight
/// through `Construct` (a single trait, so no autoref trick is needed).
pub fn constructor_node_impl(struct_ident: &Ident, fields: &[FieldSpec]) -> TokenStream {
    let debug_name = struct_ident.to_string();
    let slot_count = fields.len();

    let slot_arms = fields.iter().enumerate().map(|(index, field)| {
        let ident = &field.ident;
        quote! { #index => &self.#ident, }
    });
    let slot_mut_arms = fields.iter().enumerate().map(|(index, field)| {
        let ident = &field.ident;
        quote! { #index => &mut self.#ident, }
    });
    let lerp_fields = fields.iter().enumerate().map(|(index, field)| {
        let ident = &field.ident;
        let name = ident.to_string();
        if field.hold {
            quote! {
                #ident: ::rust_scene::__macro_support::hold_field_lerp(
                    #debug_name, #index, #name, &self.#ident, &other.#ident,
                )?,
            }
        } else {
            quote! {
                #ident: ::rust_scene::Keyed::lerp(&self.#ident, &other.#ident, t)
                    .map_err(|err| ::rust_scene::__macro_support::slot_error(#debug_name, #index, #name, err))?,
            }
        }
    });

    quote! {
        impl ::rust_scene::ConstructorNode for #struct_ident {
            fn op_id(&self) -> ::rust_scene::OpId {
                ::rust_scene::OpId::of::<#struct_ident>()
            }

            fn evaluate(&self) -> ::rust_scene::MeshValue {
                ::rust_scene::Construct::construct(self)
            }

            fn lerp_slots(
                &self,
                other: &dyn ::rust_scene::ConstructorNode,
                t: f64,
            ) -> ::rust_scene::Result<::std::boxed::Box<dyn ::rust_scene::ConstructorNode>> {
                let other = ::rust_scene::__macro_support::downcast_constructor::<#struct_ident>(other, #debug_name)?;
                Ok(::std::boxed::Box::new(#struct_ident { #(#lerp_fields)* }))
            }

            fn slot(&self, index: usize) -> &dyn ::rust_scene::Slot {
                match index {
                    #(#slot_arms)*
                    _ => panic!("slot index {} out of range for `{}` ({} slots)", index, #debug_name, #slot_count),
                }
            }

            fn slot_mut(&mut self, index: usize) -> &mut dyn ::rust_scene::Slot {
                match index {
                    #(#slot_mut_arms)*
                    _ => panic!("slot index {} out of range for `{}` ({} slots)", index, #debug_name, #slot_count),
                }
            }

            fn slot_count(&self) -> usize {
                #slot_count
            }

            fn clone_node(&self) -> ::std::boxed::Box<dyn ::rust_scene::ConstructorNode> {
                ::std::boxed::Box::new(::std::clone::Clone::clone(self))
            }

            fn debug_name(&self) -> &'static str {
                #debug_name
            }

            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }
        }
    }
}

/// The chaining extension trait + `impl for MeshValue`, taking every field in
/// declaration order (the "full arity" spelling).
pub fn chaining_ext(
    struct_ident: &Ident,
    trait_ident: &Ident,
    method_name: &Ident,
    fields: &[FieldSpec],
    vis: &Visibility,
) -> TokenStream {
    let params: Vec<_> = fields
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            quote! { #ident: #ty }
        })
        .collect();
    let field_inits = fields.iter().map(|field| {
        let ident = &field.ident;
        quote! { #ident }
    });

    quote! {
        #vis trait #trait_ident {
            fn #method_name(self, #(#params),*) -> ::rust_scene::MeshValue;
        }

        impl #trait_ident for ::rust_scene::MeshValue {
            fn #method_name(self, #(#params),*) -> ::rust_scene::MeshValue {
                ::rust_scene::MeshValue::with(self, #struct_ident { #(#field_inits),* })
            }
        }
    }
}

/// The "defaults dropped from the right" chaining extension trait: same
/// method name, only the required (non-defaulted) fields as parameters.
/// Deliberately a *separate* trait from `chaining_ext`'s — see
/// `rust_scene_macros::attr_operator` module doc for why arity overloading
/// under one trait/method name isn't possible in stable Rust, and why callers
/// must `use` only one of the two traits per scope.
pub fn chaining_ext_with_defaults(
    struct_ident: &Ident,
    trait_ident: &Ident,
    method_name: &Ident,
    required: &[FieldSpec],
    defaulted: &[(FieldSpec, TokenStream)],
    vis: &Visibility,
) -> TokenStream {
    let params: Vec<_> = required
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            quote! { #ident: #ty }
        })
        .collect();
    let required_inits = required.iter().map(|field| {
        let ident = &field.ident;
        quote! { #ident }
    });
    let defaulted_inits = defaulted.iter().map(|(field, expr)| {
        let ident = &field.ident;
        quote! { #ident: (#expr) }
    });

    quote! {
        #vis trait #trait_ident {
            fn #method_name(self, #(#params),*) -> ::rust_scene::MeshValue;
        }

        impl #trait_ident for ::rust_scene::MeshValue {
            fn #method_name(self, #(#params),*) -> ::rust_scene::MeshValue {
                ::rust_scene::MeshValue::with(self, #struct_ident {
                    #(#required_inits,)*
                    #(#defaulted_inits,)*
                })
            }
        }
    }
}
