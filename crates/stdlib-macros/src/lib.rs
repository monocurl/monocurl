use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// annotate a function to register it as a stdlib native function.
///
/// the function must be an async fn with signature:
///   `async fn(args: Vec<Value>) -> Result<Value, ExecutorError>`
///
/// the generated wrapper matches `NativeFunc`:
///   `fn(Vec<Value>) -> NativeFuture`
///
/// and submits a `FunctionEntry` to the inventory collector.
#[proc_macro_attribute]
pub fn stdlib_func(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let ident = &func.sig.ident;
    let name_str = ident.to_string();

    let wrapper_ident = syn::Ident::new(
        &format!("__{}_native_wrapper", ident),
        ident.span(),
    );

    let expanded = quote! {
        #func

        fn #wrapper_ident(
            args: ::std::vec::Vec<executor::value::Value>,
        ) -> ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = ::std::result::Result<executor::value::Value, executor::error::ExecutorError>>>> {
            ::std::boxed::Box::pin(#ident(args))
        }

        ::inventory::submit! {
            crate::registry::FunctionEntry {
                name: #name_str,
                func: #wrapper_ident,
            }
        }
    };

    expanded.into()
}
