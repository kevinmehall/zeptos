use darling::export::NestedMeta;
use darling::FromMeta;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{ReturnType, Type};

use crate::ctxt::Ctxt;

#[derive(Debug, FromMeta)]
struct Args {
}

pub fn cortex_m() -> TokenStream {
    quote! {
        #[::zeptos::internal::cortex_m_rt::entry]
        fn main() -> ! {
            unsafe { zeptos::internal::pre_init(); }
            let spawner = unsafe { ::zeptos::Spawner::steal() };
            __main_task(spawner).spawn(spawner);
            unsafe { zeptos::internal::post_init(); }
        }
    }
}

pub fn run(args: &[NestedMeta], f: syn::ItemFn, main: TokenStream) -> Result<TokenStream, TokenStream> {
    #[allow(unused_variables)]
    let _args = Args::from_list(args).map_err(|e| e.write_errors())?;

    let fargs = f.sig.inputs.clone();

    let ctxt = Ctxt::new();

    if f.sig.asyncness.is_none() {
        ctxt.error_spanned_by(&f.sig, "main function must be async");
    }
    if !f.sig.generics.params.is_empty() {
        ctxt.error_spanned_by(&f.sig, "main function must not be generic");
    }
    if !f.sig.generics.where_clause.is_none() {
        ctxt.error_spanned_by(&f.sig, "main function must not have `where` clauses");
    }
    if !f.sig.abi.is_none() {
        ctxt.error_spanned_by(&f.sig, "main function must not have an ABI qualifier");
    }
    if !f.sig.variadic.is_none() {
        ctxt.error_spanned_by(&f.sig, "main function must not be variadic");
    }
    match &f.sig.output {
        ReturnType::Default => {}
        ReturnType::Type(_, ty) => match &**ty {
            Type::Tuple(tuple) if tuple.elems.is_empty() => {}
            Type::Never(_) => {}
            _ => ctxt.error_spanned_by(
                &f.sig,
                "main function must either not return a value, return `()` or return `!`",
            ),
        },
    }

    if fargs.len() != 1 {
        ctxt.error_spanned_by(&f.sig, "main function must have 1 argument: the spawner.");
    }

    ctxt.check()?;

    let f_body = f.block;
    let out = &f.sig.output;

    let result = quote! {
        #[doc(hidden)]
        #[::zeptos::task]
        async fn __main_task(#fargs) #out {
            #f_body
        }
        
        #main
    };

    Ok(result)
}