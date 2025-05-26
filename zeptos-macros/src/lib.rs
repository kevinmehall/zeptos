// Based on Embassy, under MIT OR Apache-2.0
extern crate proc_macro;

use darling::ast::NestedMeta;
use proc_macro::TokenStream;
use syn::parse::{Parse, ParseBuffer};
use syn::punctuated::Punctuated;
use syn::Token;

mod ctxt;
mod task_attr;
mod main_attr;

struct Args {
    meta: Vec<NestedMeta>,
}

impl Parse for Args {
    fn parse(input: &ParseBuffer) -> syn::Result<Self> {
        let meta = Punctuated::<NestedMeta, Token![,]>::parse_terminated(input)?;
        Ok(Args {
            meta: meta.into_iter().collect(),
        })
    }
}

/// Declares an async task.
///
///
/// The following restrictions apply:
///
/// * The function must be declared `async`.
/// * The function must not use generics.
///
/// ## Examples
///
/// Declaring a task taking no arguments:
///
/// ``` rust
/// #[zeptos::task]
/// async fn mytask() {
///     // Function body
/// }
/// ```
#[proc_macro_attribute]
pub fn task(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(args as Args);
    let f = syn::parse_macro_input!(item as syn::ItemFn);

    task_attr::run(&args.meta, f).unwrap_or_else(|x| x).into()
}

/// Defines the application entry point, which runs as an async task.
///
/// The following restrictions apply:
///
/// * The function must accept exactly two parameters: `rt: Runtime, hw: Hardware`
/// * The function must be declared `async`.
/// * The function must not use generics.
/// * Only a single `main` task may be declared.
///
/// ## Examples
///
/// ``` rust
/// #[zeptos::main]
/// async fn main(rt: Runtime, hw: Hardware) {
///     // Function body
/// }
/// ```
#[proc_macro_attribute]
pub fn main_cortex_m(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(args as Args);
    let f = syn::parse_macro_input!(item as syn::ItemFn);
    main_attr::run(&args.meta, f, main_attr::cortex_m()).unwrap_or_else(|x| x).into()
}
