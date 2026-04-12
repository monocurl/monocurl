use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// annotate a function to register it as a stdlib native function.
///
/// the function must be an async fn with signature:
///   `async fn(args: Vec<Value>) -> Result<Value, ExecutorError>`
///
/// the generated wrapper matches `StdlibFunc`:
///   `fn(&mut ExecutionState, usize, u16) -> StdlibReturn`
///
/// args are drained directly from the execution stack (no intermediate clone),
/// lvalues are resolved before being passed in.
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
            state: &mut executor::state::ExecutionState,
            stack_idx: usize,
        ) -> executor::executor::StdlibReturn {
            ::std::boxed::Box::pin(#ident(state, stack_idx))
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
