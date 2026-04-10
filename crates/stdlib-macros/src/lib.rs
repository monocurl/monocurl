use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// annotate a `fn(i32) -> i32` to register it in the global function inventory.
///
/// the calling crate must have `FunctionEntry` at `crate::registry::FunctionEntry`
/// and `inventory::collect!(FunctionEntry)` declared (stdlib::registry handles this).
#[proc_macro_attribute]
pub fn stdlib_func(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let ident = &func.sig.ident;
    let name_str = ident.to_string();

    let expanded = quote! {
        #func

        ::inventory::submit! {
            crate::registry::FunctionEntry {
                name: #name_str,
                func: #ident,
            }
        }
    };

    expanded.into()
}
