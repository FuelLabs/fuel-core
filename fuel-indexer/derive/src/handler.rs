use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, parse_quote, Attribute, Block, FnArg, ItemFn, PatType, Token};

pub fn process_handler_attr(attrs: TokenStream, item: TokenStream) -> TokenStream {
    if !attrs.is_empty() {
        proc_macro_error::abort_call_site!("handler macro does not take arguments")
    }
    let mut item_fn = parse_macro_input!(item as ItemFn);

    let has_nomangle = item_fn
        .attrs
        .iter()
        .find(|attr| {
            let path = attr.path.get_ident();
            path.is_some() && path.unwrap().to_string() == String::from("no_mangle")
        })
        .is_some();

    if !has_nomangle {
        let no_mangle: Attribute = parse_quote! { #[no_mangle] };
        item_fn.attrs.push(no_mangle);
    };

    let mut block: Block = parse_quote! {
        {
            use fuel_indexer::types::{serialize, deserialize};
        }
    };

    let mut final_sig: Punctuated<FnArg, Token![,]> = parse_quote! {};

    for (idx, item) in item_fn.sig.inputs.iter().enumerate() {
        match item {
            FnArg::Typed(PatType { pat, ty, .. }) => {
                let ptr = format_ident! {"ptr{}", idx};
                let len = format_ident! {"len{}", idx};

                final_sig = parse_quote! { #final_sig #ptr: *mut u8, #len: usize, };

                let stmts: Vec<_> = parse_quote! {
                    let vec = unsafe { Vec::from_raw_parts(#ptr, #len, #len) };
                    let #pat: #ty = deserialize(&vec);
                };

                block.stmts.extend(stmts);
            }
            FnArg::Receiver(_) => {
                proc_macro_error::abort_call_site!("'self' in function signature not allowed here")
            }
        }
    }

    std::mem::swap(&mut final_sig, &mut item_fn.sig.inputs);
    std::mem::swap(&mut block, &mut item_fn.block);
    item_fn.block.stmts.extend(block.stmts);

    let output = quote! {
        #item_fn
    };

    proc_macro::TokenStream::from(output)
}
