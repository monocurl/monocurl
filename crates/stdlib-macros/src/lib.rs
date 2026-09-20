use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// annotate a function to register it as a stdlib native function.
///
/// the function takes `(&mut Executor, usize)` and returns
/// `Result<Value, ExecutorError>`.
///
/// an `async fn` is registered only through the boxed-future entry point. a
/// plain `fn` is additionally registered as a synchronous entry point, which
/// lets the interpreter call it without allocating and polling a future — worth
/// doing for anything that does not invoke user code or render text.
#[proc_macro_attribute]
pub fn stdlib_func(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let ident = &func.sig.ident;
    // raw identifiers (e.g. `r#mod`) stringify with the `r#` prefix — strip it
    // so the registered monocurl name is just `mod`.
    let raw_name = ident.to_string();
    let name_str = raw_name.trim_start_matches("r#").to_string();

    let wrapper_ident = syn::Ident::new(&format!("__{name_str}_native_wrapper"), ident.span());
    let sync_wrapper_ident =
        syn::Ident::new(&format!("__{name_str}_native_sync_wrapper"), ident.span());

    let (async_body, sync_entry) = if func.sig.asyncness.is_some() {
        (quote! { ::std::boxed::Box::pin(#ident(executor, stack_idx)) }, quote! { None })
    } else {
        (
            quote! {
                let result = #ident(executor, stack_idx);
                ::std::boxed::Box::pin(async move { result })
            },
            quote! { Some(#sync_wrapper_ident) },
        )
    };

    let sync_wrapper = if func.sig.asyncness.is_some() {
        quote! {}
    } else {
        quote! {
            fn #sync_wrapper_ident(
                executor: &mut executor::executor::Executor,
                stack_idx: usize,
            ) -> ::std::result::Result<executor::value::Value, executor::error::ExecutorError> {
                #ident(executor, stack_idx)
            }
        }
    };

    quote! {
        #func

        fn #wrapper_ident(
            executor: &mut executor::executor::Executor,
            stack_idx: usize,
        ) -> executor::executor::StdlibReturn<'_> {
            #async_body
        }

        #sync_wrapper

        ::inventory::submit! {
            crate::registry::FunctionEntry {
                name: #name_str,
                func: #wrapper_ident,
                sync_func: #sync_entry,
            }
        }
    }
    .into()
}
