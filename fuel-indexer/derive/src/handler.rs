use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, parse_quote, Attribute, Block, FnArg, ItemFn, PatType, Token};

pub fn process_handler_attr(attrs: TokenStream, item: TokenStream) -> TokenStream {
    if !attrs.is_empty() {
        proc_macro_error::abort_call_site!("handler macro does not take arguments")
    }
    let mut item_fn = parse_macro_input!(item as ItemFn);

    let has_nomangle = item_fn.attrs.iter().any(|attr| {
        let path = attr.path.get_ident();
        path.is_some() && path.unwrap() == "no_mangle"
    });

    if !has_nomangle {
        let no_mangle: Attribute = parse_quote! { #[no_mangle] };
        item_fn.attrs.push(no_mangle);
    };

    let mut sig: Punctuated<FnArg, Token![,]> = parse_quote! { bytes: *mut u8, len: usize };
    std::mem::swap(&mut sig, &mut item_fn.sig.inputs);
    let num_args = sig.len();

    let mut param_types = Vec::new();
    for item in sig.iter() {
        match item {
            FnArg::Typed(PatType { ty, .. }) => {
                param_types.push(quote! { #ty::param_types() });
            }
            FnArg::Receiver(_) => {
                proc_macro_error::abort_call_site!("'self' in function signature not allowed here")
            }
        }
    }

    let mut block: Block = parse_quote! {
        {
            use fuel_indexer::types::*;
            use fuels_core::abi_decoder::ABIDecoder;
            // TODO: make log macros for this....
            Logger::info("Running handler");

            let mut decoder = ABIDecoder::new();
            let bytes = unsafe { Vec::from_raw_parts(bytes, len, len) };
            let mut tokens = decoder.decode(&[#( #param_types, )*], &bytes).expect("Failed to decode tokens");
            if #num_args != tokens.len() {
                panic!("Handler called with invalid args! Required {:?} != Input {:?}", #num_args, tokens.len());
            }
        }
    };

    let mut decoded: Vec<_> = parse_quote! {};
    // reverse order of arg list to pop them off.
    for item in sig.iter().rev() {
        match item {
            FnArg::Typed(PatType { pat, ty, .. }) => {
                let decode: Vec<_> = parse_quote! {
                    let tok = tokens.pop().expect("Not enough inputs!");
                    let st = format!("{:?}", tok);
                    let #pat = #ty::new_from_token(&tok);
                };

                decoded.extend(decode);
            }
            FnArg::Receiver(_) => {
                proc_macro_error::abort_call_site!("'self' in function signature not allowed here")
            }
        }
    }
    block.stmts.extend(decoded);
    let forgets: Vec<_> = parse_quote! {
        core::mem::forget(bytes);
    };
    block.stmts.extend(forgets);


    std::mem::swap(&mut block, &mut item_fn.block);
    item_fn.block.stmts.extend(block.stmts);

    let output = quote! {
        #item_fn
    };

    proc_macro::TokenStream::from(output)
}
