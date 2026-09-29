use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// annotate a function to register it as a stdlib native function.
///
/// the function takes `(&mut Executor, usize)` and returns
/// `Result<Value, ExecutorError>`.
///
/// a plain `fn` is additionally registered as a synchronous entry point, which
/// lets the interpreter call it without allocating and polling a future — worth
/// doing for anything that does not invoke user code or render text. an
/// `async fn` gets one only when the attribute names an attempt,
/// `#[stdlib_func(sync = op_shift_sync)]`: a fn of the same arguments returning
/// `Option<Result<Value, ExecutorError>>` that handles the call when nothing
/// needs evaluating and returns `None` to hand it to the async body
#[proc_macro_attribute]
pub fn stdlib_func(attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let sync_attempt: Option<syn::Ident> = if attr.is_empty() {
        None
    } else {
        let meta = parse_macro_input!(attr as syn::MetaNameValue);
        if !meta.path.is_ident("sync") {
            return syn::Error::new_spanned(meta.path, "expected `sync = <fn>`")
                .into_compile_error()
                .into();
        }
        match meta.value {
            syn::Expr::Path(path) => match path.path.get_ident() {
                Some(ident) => Some(ident.clone()),
                None => {
                    return syn::Error::new_spanned(path, "expected a function name")
                        .into_compile_error()
                        .into();
                }
            },
            expr => {
                return syn::Error::new_spanned(expr, "expected a function name")
                    .into_compile_error()
                    .into();
            }
        }
    };
    if sync_attempt.is_some() && func.sig.asyncness.is_none() {
        return syn::Error::new_spanned(&func.sig.ident, "sync attempts require an async fn")
            .into_compile_error()
            .into();
    }
    let ident = &func.sig.ident;
    // raw identifiers (e.g. `r#mod`) stringify with the `r#` prefix — strip it
    // so the registered monocurl name is just `mod`.
    let raw_name = ident.to_string();
    let name_str = raw_name.trim_start_matches("r#").to_string();

    let wrapper_ident = syn::Ident::new(&format!("__{name_str}_native_wrapper"), ident.span());
    let sync_wrapper_ident =
        syn::Ident::new(&format!("__{name_str}_native_sync_wrapper"), ident.span());

    let (async_body, sync_entry) = if func.sig.asyncness.is_some() {
        let sync_entry = match &sync_attempt {
            Some(attempt) => quote! { Some(#attempt) },
            None => quote! { None },
        };
        (quote! { ::std::boxed::Box::pin(#ident(executor, stack_idx)) }, sync_entry)
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
            ) -> ::std::option::Option<
                ::std::result::Result<executor::value::Value, executor::error::ExecutorError>,
            > {
                Some(#ident(executor, stack_idx))
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
