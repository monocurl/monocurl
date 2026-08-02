use proc_macro::TokenStream;

use quote::{format_ident, quote};
use syn::{
    FnArg, Ident, ItemFn, Pat,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

struct LiveInput {
    operator: bool,
    function: ItemFn,
}

impl Parse for LiveInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let operator = if input.peek(Ident) {
            let fork = input.fork();
            let ident: Ident = fork.parse()?;
            if ident == "operator" {
                input.parse::<Ident>()?;
                true
            } else {
                false
            }
        } else {
            false
        };
        let function = input.parse::<ItemFn>()?;
        Ok(Self { operator, function })
    }
}

#[proc_macro]
pub fn live(input: TokenStream) -> TokenStream {
    let LiveInput {
        operator,
        mut function,
    } = parse_macro_input!(input as LiveInput);
    let name = function.sig.ident.clone();
    let type_name = pascal(&name.to_string());
    let recipe = format_ident!("{type_name}");
    let mesh_ext = format_ident!("{type_name}MeshExt");
    let nested_ext = format_ident!("{type_name}NestedExt");
    let implementation = format_ident!("__{name}_implementation");
    function.sig.ident = implementation.clone();

    let mut fields = Vec::new();
    for input in &function.sig.inputs {
        let FnArg::Typed(argument) = input else {
            return syn::Error::new_spanned(input, "methods are not supported")
                .into_compile_error()
                .into();
        };
        let Pat::Ident(pattern) = &*argument.pat else {
            return syn::Error::new_spanned(&argument.pat, "arguments must be identifiers")
                .into_compile_error()
                .into();
        };
        fields.push((pattern.ident.clone(), (*argument.ty).clone()));
    }
    if operator && fields.is_empty() {
        return syn::Error::new_spanned(&function.sig, "operators need a target argument")
            .into_compile_error()
            .into();
    }

    match &function.sig.output {
        syn::ReturnType::Type(_, _) => {}
        syn::ReturnType::Default => {
            return syn::Error::new_spanned(&function.sig, "live definitions need a return type")
                .into_compile_error()
                .into();
        }
    }
    let (target, target_ty, args) = if operator {
        let (target, target_ty) = fields.remove(0);
        (Some(target), Some(target_ty), fields)
    } else {
        (None, None, fields)
    };
    let arg_names: Vec<_> = args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = args.iter().map(|(_, ty)| ty).collect();
    let field_methods: Vec<_> = arg_names.iter().zip(&arg_types).map(|(field, ty)| quote! {
        fn #field(&self) -> ::rust_scene::Attribute<#recipe, #ty> {
            self.attribute(|recipe| recipe.#field.clone(), |recipe, value| recipe.#field = value)
        }
    }).collect();
    let field_signatures: Vec<_> = arg_names
        .iter()
        .zip(&arg_types)
        .map(|(field, ty)| quote! { fn #field(&self) -> ::rust_scene::Attribute<#recipe, #ty>; })
        .collect();
    let operator_field_methods: Vec<_> = arg_names.iter().zip(&arg_types).map(|(field, ty)| quote! {
        fn #field(&self) -> ::rust_scene::Attribute<#recipe<Target>, #ty> {
            self.attribute(|recipe| recipe.#field.clone(), |recipe, value| recipe.#field = value)
        }
    }).collect();
    let operator_field_signatures: Vec<_> = arg_names.iter().zip(&arg_types).map(|(field, ty)| quote! { fn #field(&self) -> ::rust_scene::Attribute<#recipe<Target>, #ty>; }).collect();
    let nested_methods: Vec<_> = arg_names.iter().zip(&arg_types).map(|(field, ty)| quote! {
        fn #field(&self) -> ::rust_scene::NestedAttribute<Parent, #recipe, #ty> {
            self.attribute(|recipe| recipe.#field.clone(), |recipe, value| recipe.#field = value)
        }
    }).collect();
    let nested_signatures: Vec<_> = arg_names.iter().zip(&arg_types).map(|(field, ty)| quote! { fn #field(&self) -> ::rust_scene::NestedAttribute<Parent, #recipe, #ty>; }).collect();

    if let (Some(_target), Some(_target_ty)) = (target, target_ty) {
        quote! {
            #function
            #[derive(Clone)]
            pub struct #recipe<Target: ::rust_scene::Recipe> { pub target: Target, #(pub #arg_names: #arg_types),* }
            pub fn #name<Target: ::rust_scene::Recipe>(#(#arg_names: #arg_types,)* target: Target) -> #recipe<Target> { #recipe { target, #(#arg_names),* } }
            impl<Target: ::rust_scene::Recipe> ::rust_scene::Recipe for #recipe<Target> {
                fn evaluate(&self) -> ::rust_scene::Result<::rust_scene::MeshValue> { Ok(#implementation(self.target.evaluate()?, #(self.#arg_names.clone()),*)?.modified) }
                fn interpolate_from(&self, source: &::rust_scene::MeshValue, time: f64) -> ::rust_scene::Result<::rust_scene::MeshValue> {
                    let operand = self.target.interpolate_from(source, time)?;
                    let endpoints = #implementation(operand, #(self.#arg_names.clone()),*)?;
                    endpoints.identity.lerp(&endpoints.modified, time)
                }
            }
            pub trait #mesh_ext<Target: ::rust_scene::Recipe> { #(#operator_field_signatures)* fn target(&self) -> ::rust_scene::Nested<#recipe<Target>, Target>; }
            impl<Target: ::rust_scene::Recipe> #mesh_ext<Target> for ::rust_scene::Mesh<#recipe<Target>> { #(#operator_field_methods)*
                fn target(&self) -> ::rust_scene::Nested<#recipe<Target>, Target> { self.nested(|recipe| &recipe.target, |recipe| &mut recipe.target) }
            }
        }
    } else {
        quote! {
            #function
            #[derive(Clone)]
            pub struct #recipe { #(pub #arg_names: #arg_types),* }
            pub fn #name(#(#arg_names: #arg_types),*) -> #recipe { #recipe { #(#arg_names),* } }
            impl ::rust_scene::Recipe for #recipe {
                fn evaluate(&self) -> ::rust_scene::Result<::rust_scene::MeshValue> { #implementation(#(self.#arg_names.clone()),*) }
            }
            pub trait #mesh_ext { #(#field_signatures)* }
            impl #mesh_ext for ::rust_scene::Mesh<#recipe> { #(#field_methods)* }
            pub trait #nested_ext<Parent: ::rust_scene::Recipe> { #(#nested_signatures)* }
            impl<Parent: ::rust_scene::Recipe> #nested_ext<Parent> for ::rust_scene::Nested<Parent, #recipe> { #(#nested_methods)* }
        }
    }.into()
}

fn pascal(value: &str) -> String {
    value
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .into_iter()
                .flat_map(char::to_uppercase)
                .chain(chars)
                .collect::<String>()
        })
        .collect()
}
