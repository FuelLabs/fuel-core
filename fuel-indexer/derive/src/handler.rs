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

    let mut sig: Punctuated<FnArg, Token![,]> =
        parse_quote! { blobs: *mut *mut u8, lens: *mut usize, len: usize };
    std::mem::swap(&mut sig, &mut item_fn.sig.inputs);

    let mut block: Block = parse_quote! {
        {
            use fuel_indexer::types::*;
            use fuels_core::abi_decoder::ABIDecoder;

            let mut decoder = ABIDecoder::new();
            let (blobs, lens) = unsafe { (Vec::from_raw_parts(blobs, len, len), Vec::from_raw_parts(lens, len, len)) };
        }
    };

    let mut byteses: Vec<_> = parse_quote! {};
    let mut decoded: Vec<_> = parse_quote! {};
    for (idx, item) in sig.iter().enumerate() {
        match item {
            FnArg::Typed(PatType { pat, ty, .. }) => {
                let bytes: Vec<_> = parse_quote! {
                    let tokens = unsafe { Vec::from_raw_parts(blobs[#idx], lens[#idx], lens[#idx]) };
                    let #pat = decoder.decode(&#ty::param_types(), &tokens).expect("Failed decoding");
                    core::mem::forget(tokens);
                };

                let dec: Vec<_> = parse_quote! {
                    let #pat = #ty::new_from_tokens(&#pat);
                };

                byteses.extend(bytes);
                decoded.extend(dec);
            }
            FnArg::Receiver(_) => {
                proc_macro_error::abort_call_site!("'self' in function signature not allowed here")
            }
        }
    }
    block.stmts.extend(byteses);
    block.stmts.extend(decoded);
    let forgets: Vec<_> = parse_quote! {
        core::mem::forget(blobs);
        core::mem::forget(lens);
    };
    block.stmts.extend(forgets);

    std::mem::swap(&mut block, &mut item_fn.block);
    item_fn.block.stmts.extend(block.stmts);

    let output = quote! {
        #item_fn
    };

    proc_macro::TokenStream::from(output)
}
